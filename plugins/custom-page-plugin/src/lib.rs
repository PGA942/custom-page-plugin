//! みんなのボード — ページ型プラグイン(slot `page`、契約 v0.18、rasq-sdk-sample docs/slots.md §2.13)の shared のサンプル。
//!
//! - カウンター: `+1` → `shared.op {key: "counter", op: "incr", value: 1}`
//! - 投票(3 択): 自分の票を `shared.set {key: "me.vote", value: <0..2>}`(サーバーが `@<accountId>.vote` に書き換える)。
//!   集計は全員の `@<accountId>.vote` から数える(1 人 1 票はサーバーが守る: `@<accountId>.` のキーは本人だけが書ける)
//! - 寄せ書き: `shared.op {key: "guestbook", op: "append", value: {text, by}}`(サーバーが 500 件で先頭から捨てる)
//! - 「🎉」: `shared.call {fn: "celebrate", args: {}}` → 全員(自分も)に `page.event {type: "call"}` が届き、トーストを出す
//!
//! 値は手元で先に変えない: 反映は必ず `page.event {type: "var"}` で届くので、受けてから `page.render` で描き直す
//! (全員が同じ順で同じ値を見る)。セッションが無い(1 人で開いた / 閉じられた)ときは同じ画面を `storage.safe` だけで動かす。
use rasq_plugin_kit::{
    err, host, rasq_plugin, rrl,
    serde_json::{json, Map, Value},
    unsupported,
};
use std::sync::Mutex;

/// 投票の選択肢(値は番号 0..=2)。
const OPTIONS: [&str; 3] = ["😀 元気", "😪 ねむい", "🍙 おなかすいた"];
/// 寄せ書きの定型文(`gb.quick.<n>`)。
const QUICK: [&str; 3] = ["👋 こんにちは", "👍 いいね", "🎊 おめでとう"];
/// 寄せ書き 1 件の文字数の上限。
const MAX_TEXT: usize = 80;
/// 寄せ書きの件数の上限(サーバーの append と同じ。solo でも同じにする)。
const MAX_ENTRIES: usize = 500;
/// 表示する寄せ書きの件数(新しい順)。
const SHOW_ENTRIES: usize = 30;
/// solo の保存キー(`storage.safe`)。
const STORE_KEY: &str = "board";

#[derive(Default, Clone)]
struct Session {
    id: String,
    scope: String,
    role: String,
    participants: Vec<Value>,
    /// キー → `{value, rev, by}`。
    vars: Map<String, Value>,
}

#[derive(Default)]
struct Board {
    /// 開いているページの token(閉じていれば空)。
    token: String,
    surface: String,
    context: Value,
    me: Value,
    width: f64,
    /// shared のセッション(無ければ solo = 端末だけ)。
    session: Option<Session>,
    /// セッションが閉じられたときの知らせ。
    notice: Option<String>,
    /// このページを開いてから届いた 🎉 の数。
    celebrations: u32,
    /// solo の状態(`storage.safe` の `board`): `{counter, vote, guestbook}`。
    local: Value,
}

static BOARD: Mutex<Option<Board>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// 状態の読み出し(shared と solo で同じ形にする)
// ---------------------------------------------------------------------------

impl Board {
    fn var(&self, key: &str) -> Value {
        match &self.session {
            Some(s) => s.vars.get(key).map(|v| v["value"].clone()).unwrap_or(Value::Null),
            None => Value::Null,
        }
    }

    fn my_account(&self) -> &str {
        self.me["accountId"].as_str().unwrap_or("")
    }

    fn my_name(&self) -> String {
        let name = self.me["displayName"].as_str().unwrap_or("");
        if !name.is_empty() {
            return name.to_string();
        }
        match self.me["handle"].as_str() {
            Some(h) if !h.is_empty() => format!("@{h}"),
            _ => "だれか".to_string(),
        }
    }

    fn counter(&self) -> i64 {
        match &self.session {
            Some(_) => self.var("counter").as_i64().unwrap_or(0),
            None => self.local["counter"].as_i64().unwrap_or(0),
        }
    }

    fn my_vote(&self) -> Option<usize> {
        let v = match &self.session {
            Some(_) => self.var(&format!("@{}.vote", self.my_account())),
            None => self.local["vote"].clone(),
        };
        v.as_u64().map(|i| i as usize).filter(|i| *i < OPTIONS.len())
    }

    /// 選択肢ごとの票数。shared は全員の `@<accountId>.vote`、solo は自分の 1 票だけ。
    fn tally(&self) -> [u32; 3] {
        let mut t = [0u32; 3];
        match &self.session {
            Some(s) => {
                for (k, v) in &s.vars {
                    if k.starts_with('@') && k.ends_with(".vote") {
                        if let Some(i) = v["value"].as_u64().map(|i| i as usize).filter(|i| *i < OPTIONS.len()) {
                            t[i] += 1;
                        }
                    }
                }
            }
            None => {
                if let Some(i) = self.my_vote() {
                    t[i] += 1;
                }
            }
        }
        t
    }

    fn guestbook(&self) -> Vec<Value> {
        let v = match &self.session {
            Some(_) => self.var("guestbook"),
            None => self.local["guestbook"].clone(),
        };
        v.as_array().cloned().unwrap_or_default()
    }

    fn save_local(&self) {
        host::storage_set(STORE_KEY, self.local.clone());
    }
}

fn empty_local() -> Value {
    json!({"counter": 0, "vote": null, "guestbook": []})
}

fn load_local() -> Value {
    let stored = host::storage_get(STORE_KEY);
    if stored.is_object() {
        let mut l = empty_local();
        for k in ["counter", "vote", "guestbook"] {
            if !stored[k].is_null() {
                l[k] = stored[k].clone();
            }
        }
        l
    } else {
        empty_local()
    }
}

fn parse_session(v: &Value) -> Option<Session> {
    let id = v["id"].as_str()?;
    Some(Session {
        id: id.to_string(),
        scope: v["scope"].as_str().unwrap_or("").to_string(),
        role: v["role"].as_str().unwrap_or("member").to_string(),
        participants: v["participants"].as_array().cloned().unwrap_or_default(),
        vars: v["vars"].as_object().cloned().unwrap_or_default(),
    })
}

// ---------------------------------------------------------------------------
// ページの RRL
// ---------------------------------------------------------------------------

fn title(b: &Board) -> String {
    match b.surface.as_str() {
        "profile" => match b.context["profileHandle"].as_str() {
            Some(h) if !h.is_empty() => format!("@{h} さんのボード"),
            _ => "みんなのボード".to_string(),
        },
        "astell" => "ルームのボード".to_string(),
        _ => "みんなのボード".to_string(),
    }
}

fn status_line(b: &Board) -> String {
    match &b.session {
        Some(s) => {
            let names: Vec<String> = s
                .participants
                .iter()
                .take(6)
                .map(|p| p["displayName"].as_str().filter(|n| !n.is_empty()).or(p["handle"].as_str()).unwrap_or("?").to_string())
                .collect();
            let more = if s.participants.len() > 6 { format!(" ほか {} 人", s.participants.len() - 6) } else { String::new() };
            let owner = if s.role == "owner" { "(あなたが主催)" } else { "" };
            format!("👥 {} 人が参加中{owner}: {}{more}", s.participants.len(), names.join("、"))
        }
        None => "📱 この端末だけで動いています(ひとりモード)".to_string(),
    }
}

fn section(key: &str, t: &str) -> Value {
    rrl::node("SectionHeader", key, json!({"title": t}), vec![])
}

fn page_root(b: &Board) -> Value {
    let mut c = vec![rrl::styled_text("status", status_line(b), 13.0, false, Some("#667085"))];
    if let Some(n) = &b.notice {
        c.push(rrl::boxed("notice", json!({"color": "#FFF4E5", "radius": 8}), vec![rrl::styled_text("noticeText", n.clone(), 13.0, false, Some("#8A5300"))]));
    }

    // カウンター
    c.push(section("counterHdr", "カウンター"));
    c.push(rrl::row(
        "counterRow",
        vec![
            rrl::styled_text("counter", b.counter().to_string(), 36.0, true, None),
            rrl::spacer("counterGap", 16.0),
            rrl::button("counterInc", "+1", "counter.incr"),
        ],
    ));

    // 投票
    let tally = b.tally();
    let total: u32 = tally.iter().sum();
    let mine = b.my_vote();
    let bar_max = (b.width - 32.0).max(80.0);
    c.push(section("pollHdr", "投票: いまの気分は?"));
    for (i, label) in OPTIONS.iter().enumerate() {
        let pct = (tally[i] * 100).checked_div(total).unwrap_or(0);
        let mut tile = rrl::node(
            "RadioListTile",
            &format!("opt{i}"),
            json!({"title": label, "subtitle": format!("{} 票({pct}%)", tally[i]), "value": i, "group": mine}),
            vec![],
        );
        tile["on"] = json!({"change": "poll.vote"});
        c.push(tile);
        let w = if total == 0 { 0.0 } else { bar_max * tally[i] as f64 / total as f64 };
        c.push(rrl::boxed(&format!("bar{i}"), json!({"width": w.max(2.0), "height": 6, "radius": 3, "color": if mine == Some(i) { "#3355CC" } else { "#98A2B3" }}), vec![]));
    }
    let mut poll_foot = vec![rrl::text("pollTotal", format!("合計 {total} 票"))];
    if mine.is_some() {
        poll_foot.push(rrl::spacer("pollGap", 12.0));
        poll_foot.push(rrl::button("pollClear", "票を取り消す", "poll.clear"));
    }
    c.push(rrl::row("pollFoot", poll_foot));

    // 寄せ書き
    let entries = b.guestbook();
    c.push(section("gbHdr", &format!("寄せ書き({} 件)", entries.len())));
    // 打ちかけの文字はホストが持つ(描き直しても消えない)。送ると `submit {text}` が来て、欄はホストが空にする(clearOnSubmit)
    let mut input = rrl::node(
        "Input",
        "gbInput",
        json!({"hint": format!("ひとこと({MAX_TEXT} 文字まで)"), "maxLength": MAX_TEXT, "submitLabel": "書く", "clearOnSubmit": true}),
        vec![],
    );
    input["on"] = json!({"submit": "gb.send"});
    c.push(input);
    let mut quick = Vec::new();
    for (i, q) in QUICK.iter().enumerate() {
        if i > 0 {
            quick.push(rrl::spacer(&format!("gbGap{i}"), 8.0));
        }
        quick.push(rrl::button(&format!("gbQuick{i}"), q, &format!("gb.quick.{i}")));
    }
    c.push(rrl::scroll("gbQuick", true, quick));
    if entries.is_empty() {
        c.push(rrl::styled_text("gbEmpty", "まだ何もありません。最初のひとことをどうぞ。", 13.0, false, Some("#667085")));
    }
    let start = entries.len().saturating_sub(SHOW_ENTRIES);
    for (n, e) in entries[start..].iter().enumerate().rev() {
        let text = e["text"].as_str().unwrap_or("");
        let by = e["by"].as_str().unwrap_or("");
        c.push(rrl::list_tile(&format!("gb{}", start + n), text, Some(by), None));
    }

    // お祝い
    c.push(section("partyHdr", "お祝い"));
    let party_label = if b.session.is_some() { "🎉 みんなにお祝いを送る" } else { "🎉 お祝い" };
    c.push(rrl::button("party", party_label, "celebrate"));
    if b.celebrations > 0 {
        c.push(rrl::text("partyCount", format!("このページで届いた 🎉: {} 回", b.celebrations)));
    }

    rrl::scroll("page", false, vec![rrl::column("root", c)])
}

fn render(b: &Board) {
    host::page_render(&b.token, page_root(b), Some(&title(b)));
}

// ---------------------------------------------------------------------------
// 操作
// ---------------------------------------------------------------------------

/// ホスト関数が断られた(`ok: false`、E01 など)ときに知らせる。
fn check_sent(r: Value) {
    if r["ok"] == false {
        host::toast("送れませんでした(セッションに入っていないか、送りすぎです)");
    }
}

fn clip(s: &str) -> String {
    s.trim().chars().take(MAX_TEXT).collect()
}

fn post_entry(b: &mut Board, text: &str) {
    let text = clip(text);
    if text.is_empty() {
        host::toast("ひとこと書いてください");
        return;
    }
    let entry = json!({"text": text, "by": b.my_name()});
    if b.session.is_some() {
        // 寄せ書きは var が届いてから増える(入力欄はホストが空にする)
        check_sent(host::shared_op(&b.token, "guestbook", "append", entry));
    } else {
        let mut list = b.local["guestbook"].as_array().cloned().unwrap_or_default();
        list.push(entry);
        if list.len() > MAX_ENTRIES {
            list.drain(..list.len() - MAX_ENTRIES);
        }
        b.local["guestbook"] = Value::Array(list);
        b.save_local();
        render(b);
    }
}

fn set_vote(b: &mut Board, vote: Value) {
    if b.session.is_some() {
        check_sent(host::shared_set(&b.token, "me.vote", vote, None));
    } else {
        b.local["vote"] = vote;
        b.save_local();
        render(b);
    }
}

fn on_event(b: &mut Board, args: &Value) {
    match args["type"].as_str().unwrap_or("") {
        // `{key, value, rev, by}` / `{key, value, rev}`: 変数を置き換える(null = 消えた)
        "var" | "conflict" => {
            let Some(key) = args["key"].as_str() else { return };
            if let Some(s) = b.session.as_mut() {
                if args["value"].is_null() {
                    s.vars.remove(key);
                } else {
                    s.vars.insert(key.to_string(), json!({"value": args["value"], "rev": args["rev"], "by": args["by"]}));
                }
                render(b);
            }
        }
        // `{fn, args, from}`: 誰かの shared.call(自分のも届く)
        "call" => {
            if args["fn"] == "celebrate" {
                let from = &args["from"];
                let name = from["displayName"].as_str().filter(|n| !n.is_empty()).or(from["handle"].as_str()).unwrap_or("だれか");
                b.celebrations += 1;
                host::toast(&format!("🎉 {name} さんがお祝いしました!"));
                render(b);
            }
        }
        // `{participant}`
        "joined" => {
            if let Some(s) = b.session.as_mut() {
                let p = args["participant"].clone();
                s.participants.retain(|q| q["accountId"] != p["accountId"]);
                s.participants.push(p);
                render(b);
            }
        }
        // `{accountId}`
        "left" => {
            if let Some(s) = b.session.as_mut() {
                s.participants.retain(|q| q["accountId"] != args["accountId"]);
                render(b);
            }
        }
        // `{reason}`: 以後は端末だけで続ける
        "closed" => {
            if b.session.take().is_some() {
                let reason = args["reason"].as_str().unwrap_or("");
                let why = if reason.is_empty() { String::new() } else { format!("({reason})") };
                b.notice = Some(format!("セッションが閉じられました{why}。ここからはこの端末だけで動きます。"));
                b.local = load_local();
                render(b);
            }
        }
        // `{size}`
        "resized" => {
            if let Some(w) = args["size"]["width"].as_f64() {
                b.width = w;
                render(b);
            }
        }
        _ => {}
    }
}

/// 開いているページの token と合う呼び出しだけ通す(古いページへの通知・イベントは捨てる)。
/// `lenient` = RRL のイベント: token が**無い**呼び出し(`rasq call --then` など、開発用)は今のページへ通す。違う token は捨てる。
fn with_board<F: FnOnce(&mut Board) -> Result<Value, Value>>(args: &Value, lenient: bool, f: F) -> Result<Value, Value> {
    let mut guard = BOARD.lock().unwrap();
    match guard.as_mut() {
        Some(b) if args["token"].as_str() == Some(b.token.as_str()) || (lenient && args["token"].is_null()) => f(b),
        _ => Ok(json!({})),
    }
}

fn handle(fn_name: &str, args: Value) -> Result<Value, Value> {
    match fn_name {
        "plugin.manifest" => Ok(manifest()),
        "lifecycle" => Ok(json!({})),
        // `{token, surface, context, me, session?, size}` → `{root, title}`
        "page.open" => {
            let token = args["token"].as_str().unwrap_or("");
            if token.is_empty() {
                return Err(err("D01", "token required"));
            }
            let session = parse_session(&args["session"]);
            let mut b = Board {
                token: token.to_string(),
                surface: args["surface"].as_str().unwrap_or("page").to_string(),
                context: args["context"].clone(),
                me: args["me"].clone(),
                width: args["size"]["width"].as_f64().unwrap_or(360.0),
                local: if session.is_none() { load_local() } else { empty_local() },
                session,
                ..Board::default()
            };
            if b.session.as_ref().is_some_and(|s| s.id.is_empty() || s.scope.is_empty()) {
                b.session = None;
                b.local = load_local();
            }
            let out = json!({"root": page_root(&b), "title": title(&b)});
            *BOARD.lock().unwrap() = Some(b);
            Ok(out)
        }
        // `{token, type, ...}`(平ら)
        "page.event" => with_board(&args, false, |b| {
            on_event(b, &args);
            Ok(json!({}))
        }),
        // `{token, reason}`
        "page.close" => {
            let mut guard = BOARD.lock().unwrap();
            if guard.as_ref().is_some_and(|b| args["token"].as_str() == Some(b.token.as_str())) {
                *guard = None;
            }
            Ok(json!({}))
        }

        // ---- RRL のイベント: `{token, nodeId, kind, ...payload}` ----------------------------
        "counter.incr" => with_board(&args, true, |b| {
            if b.session.is_some() {
                check_sent(host::shared_op(&b.token, "counter", "incr", json!(1)));
            } else {
                b.local["counter"] = json!(b.local["counter"].as_i64().unwrap_or(0) + 1);
                b.save_local();
                render(b);
            }
            Ok(json!({}))
        }),
        // RadioListTile の change: `{value: <番号>}`
        "poll.vote" => with_board(&args, true, |b| {
            let i = args["value"].as_u64().filter(|i| (*i as usize) < OPTIONS.len()).ok_or_else(|| err("D01", "value must be 0..=2"))?;
            set_vote(b, json!(i));
            Ok(json!({}))
        }),
        "poll.clear" => with_board(&args, true, |b| {
            set_vote(b, Value::Null);
            Ok(json!({}))
        }),
        // Input の submit: `{text}`
        "gb.send" => with_board(&args, true, |b| {
            post_entry(b, args["text"].as_str().unwrap_or(""));
            Ok(json!({}))
        }),
        f if f.starts_with("gb.quick.") => with_board(&args, true, |b| {
            let i: usize = f["gb.quick.".len()..].parse().map_err(|_| unsupported(f))?;
            let q = QUICK.get(i).ok_or_else(|| unsupported(f))?;
            post_entry(b, q);
            Ok(json!({}))
        }),
        "celebrate" => with_board(&args, true, |b| {
            if b.session.is_some() {
                // 自分にも page.event {type: "call"} が届くので、トーストはそこで出す
                check_sent(host::shared_call(&b.token, "celebrate", json!({})));
            } else {
                b.celebrations += 1;
                host::toast("🎉 おめでとう!");
                render(b);
            }
            Ok(json!({}))
        }),
        _ => Err(unsupported(fn_name)),
    }
}

fn manifest() -> Value {
    rasq_plugin_kit::serde_json::from_str(include_str!("../manifest.json")).expect("manifest.json")
}

fn root() -> Value {
    rrl::column("root", vec![rrl::text("desc", "みんなのボード(ページ型プラグイン、slot page)")])
}

rasq_plugin! { scope: "sample.board:main", root: root, call: handle }

#[cfg(test)]
mod tests {
    use super::*;
    use rasq_plugin_kit::{set_host_hook, take_host_log};

    /// ページの状態(BOARD)はプロセスで 1 つなので、テストは 1 本ずつ走らせる。
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn me() -> Value {
        json!({"accountId": "a1", "handle": "hanako", "displayName": "花子"})
    }

    fn session() -> Value {
        json!({
            "id": "cs_1", "scope": "room:r1", "role": "member",
            "participants": [me(), {"accountId": "a2", "handle": "taro", "displayName": "太郎"}],
            "vars": {
                "counter": {"value": 3, "rev": 3, "by": "a2"},
                "@a2.vote": {"value": 1, "rev": 1, "by": "a2"},
                "guestbook": {"value": [{"text": "やあ", "by": "太郎"}], "rev": 1, "by": "a2"}
            }
        })
    }

    fn open(session: Value) -> Value {
        let mut a = json!({"token": "p1", "surface": "astell", "context": {"roomId": "r1"}, "me": me(), "size": {"width": 400, "height": 800}});
        if !session.is_null() {
            a["session"] = session;
        }
        call_native("page.open", a).unwrap()
    }

    /// RRL の中から `k` のノードを探す。
    fn find<'a>(n: &'a Value, k: &str) -> Option<&'a Value> {
        if n["k"] == k {
            return Some(n);
        }
        n["c"].as_array().into_iter().flatten().find_map(|c| find(c, k))
    }

    fn last_render(log: &[(String, Value)]) -> Value {
        log.iter().rev().find(|(f, _)| f == "page.render").map(|(_, a)| a["root"].clone()).expect("page.render")
    }

    #[test]
    fn manifest_is_shared_page() {
        let _g = serial();
        let m = call_native("plugin.manifest", json!({})).unwrap();
        assert_eq!(m["id"], json!("sample.board"));
        assert_eq!(m["slots"], json!(["page"]));
        assert_eq!(m["page"]["mode"], json!("shared"));
        assert_eq!(m["page"]["surfaces"], json!(["page", "astell", "profile"]));
        assert_eq!(m["permissions"], json!(["render", "shared.session", "storage.safe"]));
        assert_eq!(call_native("nope", json!({})).unwrap_err()["code"], json!("C01"));
        assert_eq!(call_native("page.open", json!({})).unwrap_err()["code"], json!("D01"));
    }

    #[test]
    fn shared_flow() {
        let _g = serial();
        set_host_hook(|_, _| json!({}));
        take_host_log();
        let r = open(session());
        assert_eq!(r["title"], json!("ルームのボード"));
        let root = &r["root"];
        assert_eq!(find(root, "counter").unwrap()["p"]["text"], json!("3"));
        assert_eq!(find(root, "opt1").unwrap()["p"]["subtitle"], json!("1 票(100%)"));
        assert_eq!(find(root, "gb0").unwrap()["p"]["title"], json!("やあ"));
        assert!(find(root, "status").unwrap()["p"]["text"].as_str().unwrap().contains("2 人が参加中"));
        // shared では何も保存しない / 読まない
        assert!(take_host_log().is_empty());

        // +1 → shared.op incr(手元では変えない)
        call_native("counter.incr", json!({"token": "p1", "nodeId": "sample.board:main:counterInc", "kind": "tap"})).unwrap();
        assert_eq!(take_host_log(), vec![("shared.op".to_string(), json!({"token": "p1", "key": "counter", "op": "incr", "value": 1}))]);
        call_native("page.event", json!({"token": "p1", "type": "var", "key": "counter", "value": 4, "rev": 4, "by": "a1"})).unwrap();
        assert_eq!(find(&last_render(&take_host_log()), "counter").unwrap()["p"]["text"], json!("4"));

        // 投票(RadioListTile の change、payload は平ら)→ shared.set me.vote
        call_native("poll.vote", json!({"token": "p1", "nodeId": "x", "kind": "change", "value": 2})).unwrap();
        assert_eq!(take_host_log(), vec![("shared.set".to_string(), json!({"token": "p1", "key": "me.vote", "value": 2}))]);
        call_native("page.event", json!({"token": "p1", "type": "var", "key": "@a1.vote", "value": 2, "rev": 1, "by": "a1"})).unwrap();
        let root = last_render(&take_host_log());
        assert_eq!(find(&root, "opt2").unwrap()["p"]["group"], json!(2));
        assert_eq!(find(&root, "opt2").unwrap()["p"]["subtitle"], json!("1 票(50%)"));
        assert_eq!(find(&root, "pollTotal").unwrap()["p"]["text"], json!("合計 2 票"));
        assert!(find(&root, "pollClear").is_some());
        // 取り消し = null
        call_native("poll.clear", json!({"token": "p1", "nodeId": "x", "kind": "tap"})).unwrap();
        assert_eq!(take_host_log()[0].1, json!({"token": "p1", "key": "me.vote", "value": null}));
        call_native("page.event", json!({"token": "p1", "type": "var", "key": "@a1.vote", "value": null, "rev": 2, "by": "a1"})).unwrap();
        assert_eq!(find(&last_render(&take_host_log()), "pollTotal").unwrap()["p"]["text"], json!("合計 1 票"));
        assert_eq!(call_native("poll.vote", json!({"token": "p1", "value": 7})).unwrap_err()["code"], json!("D01"));

        // 寄せ書き: Input の submit {text} → shared.op append {text, by}(手元では増やさない)
        let input = find(&root, "gbInput").unwrap().clone();
        assert_eq!(input["on"], json!({"submit": "gb.send"}));
        assert_eq!(input["p"]["clearOnSubmit"], json!(true));
        call_native("gb.send", json!({"token": "p1", "nodeId": "x", "kind": "submit", "text": " こんばんは "})).unwrap();
        assert_eq!(take_host_log(), vec![("shared.op".to_string(), json!({"token": "p1", "key": "guestbook", "op": "append", "value": {"text": "こんばんは", "by": "花子"}}))]);
        call_native("gb.quick.1", json!({"token": "p1", "nodeId": "x", "kind": "tap"})).unwrap();
        assert_eq!(take_host_log()[0].1["value"], json!({"text": "👍 いいね", "by": "花子"}));
        call_native("gb.send", json!({"token": "p1", "nodeId": "x", "kind": "submit", "text": "   "})).unwrap();
        assert_eq!(take_host_log()[0].0, "toast");
        call_native("page.event", json!({"token": "p1", "type": "var", "key": "guestbook", "value": [{"text": "やあ", "by": "太郎"}, {"text": "こんばんは", "by": "花子"}], "rev": 2, "by": "a1"})).unwrap();
        let root = last_render(&take_host_log());
        assert_eq!(find(&root, "gb1").unwrap()["p"]["subtitle"], json!("花子"));
        assert_eq!(find(&root, "gbHdr").unwrap()["p"]["title"], json!("寄せ書き(2 件)"));

        // 🎉 → shared.call、届いた call でトースト
        call_native("celebrate", json!({"token": "p1", "nodeId": "x", "kind": "tap"})).unwrap();
        assert_eq!(take_host_log(), vec![("shared.call".to_string(), json!({"token": "p1", "fn": "celebrate", "args": {}}))]);
        call_native("page.event", json!({"token": "p1", "type": "call", "fn": "celebrate", "args": {}, "from": {"accountId": "a2", "handle": "taro", "displayName": "太郎"}})).unwrap();
        let log = take_host_log();
        assert_eq!(log[0], ("toast".to_string(), json!({"message": "🎉 太郎 さんがお祝いしました!"})));
        assert!(find(&last_render(&log), "partyCount").is_some());

        // 参加 / 退出
        call_native("page.event", json!({"token": "p1", "type": "joined", "participant": {"accountId": "a3", "handle": "jiro", "displayName": "次郎"}})).unwrap();
        assert!(find(&last_render(&take_host_log()), "status").unwrap()["p"]["text"].as_str().unwrap().contains("3 人が参加中"));
        call_native("page.event", json!({"token": "p1", "type": "left", "accountId": "a2"})).unwrap();
        assert!(find(&last_render(&take_host_log()), "status").unwrap()["p"]["text"].as_str().unwrap().contains("2 人が参加中"));

        // 古い token は無視
        call_native("page.event", json!({"token": "old", "type": "var", "key": "counter", "value": 99})).unwrap();
        call_native("counter.incr", json!({"token": "old"})).unwrap();
        assert!(take_host_log().is_empty());

        // 閉じられた → 端末だけ(storage.safe を読む)
        set_host_hook(|f, _| if f == "storage.get" { json!({"value": {"counter": 10, "vote": 0, "guestbook": []}}) } else { json!({}) });
        call_native("page.event", json!({"token": "p1", "type": "closed", "reason": "host"})).unwrap();
        let log = take_host_log();
        assert_eq!(log[0].0, "storage.get");
        let root = last_render(&log);
        assert!(find(&root, "notice").is_some());
        assert_eq!(find(&root, "counter").unwrap()["p"]["text"], json!("10"));
        call_native("page.close", json!({"token": "p1", "reason": "back"})).unwrap();
        call_native("counter.incr", json!({"token": "p1"})).unwrap();
        assert!(take_host_log().is_empty());
    }

    #[test]
    fn solo_flow() {
        let _g = serial();
        set_host_hook(|f, _| if f == "storage.get" { json!({"value": null}) } else { json!({}) });
        take_host_log();
        let r = open(Value::Null);
        assert!(find(&r["root"], "status").unwrap()["p"]["text"].as_str().unwrap().contains("ひとりモード"));
        assert_eq!(find(&r["root"], "counter").unwrap()["p"]["text"], json!("0"));
        assert_eq!(take_host_log()[0].0, "storage.get");

        call_native("counter.incr", json!({"token": "p1", "nodeId": "x", "kind": "tap"})).unwrap();
        let log = take_host_log();
        assert_eq!(log[0], ("storage.set".to_string(), json!({"ns": "safe", "key": "board", "value": {"counter": 1, "vote": null, "guestbook": []}})));
        assert_eq!(find(&last_render(&log), "counter").unwrap()["p"]["text"], json!("1"));

        call_native("poll.vote", json!({"token": "p1", "nodeId": "x", "kind": "change", "value": 0})).unwrap();
        assert_eq!(find(&last_render(&take_host_log()), "opt0").unwrap()["p"]["subtitle"], json!("1 票(100%)"));

        call_native("gb.send", json!({"token": "p1", "nodeId": "x", "kind": "submit", "text": "メモ"})).unwrap();
        let log = take_host_log();
        assert_eq!(log[0].1["value"]["guestbook"], json!([{"text": "メモ", "by": "花子"}]));
        assert_eq!(find(&last_render(&log), "gb0").unwrap()["p"]["title"], json!("メモ"));

        call_native("celebrate", json!({"token": "p1", "nodeId": "x", "kind": "tap"})).unwrap();
        let log = take_host_log();
        assert_eq!(log[0], ("toast".to_string(), json!({"message": "🎉 おめでとう!"})));
        // shared.* は呼ばない
        assert!(!log.iter().any(|(f, _)| f.starts_with("shared.")));
    }

    #[test]
    fn denied_shared_call_toasts() {
        let _g = serial();
        set_host_hook(|_, _| json!({}));
        take_host_log();
        // ホストが断った(E01 = セッションが無い / 送りすぎ)ときはトーストで知らせる
        check_sent(json!({"ok": false, "error": {"code": "E01"}}));
        assert_eq!(take_host_log()[0].0, "toast");
        check_sent(json!({"ok": true, "value": {}}));
        assert!(take_host_log().is_empty());
    }
}
