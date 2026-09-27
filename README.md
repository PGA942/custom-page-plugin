# custom-page-plugin — みんなのボード(ページ型プラグイン、スロット `page`)

**ページ型プラグイン(カスタムプラグイン)**のサンプル。契約 v0.18(`rasq-sdk-sample/docs/slots.md` §2.13、
rasq-client D-357 / rasq-central S-048)の **shared**(複数人でサーバー変数 / 関数を使う)の形で、1 人で開いたときは同じ画面を
端末の中(`storage.safe`)だけで動かす。

- **カウンター**: `+1` → `shared.op {key: "counter", op: "incr", value: 1}`
- **投票(3 択)**: 「😀 元気 / 😪 ねむい / 🍙 おなかすいた」。自分の票は `shared.set {key: "me.vote", value: 0..2}`(サーバーが
  `@<accountId>.vote` に書き換える = 本人だけが書ける = 1 人 1 票)。集計は全員の `@<accountId>.vote` から数える。「票を取り消す」は `value: null`
- **寄せ書き**: 入力欄(`Input`)か定型文のボタン → `shared.op {key: "guestbook", op: "append", value: {text, by}}`(`by` = 表示名。
  80 文字まで、サーバーが 500 件で先頭から捨てる)。新しい順に 30 件を表示
- **🎉**: `shared.call {fn: "celebrate", args: {}}` → 参加者全員(自分も)に `page.event {type: "call"}` が届き、
  それぞれの端末が「🎉 <名前> さんがお祝いしました!」のトーストを出す
- **値は手元で先に変えない**: `shared.*` の結果は必ず `page.event {type: "var"}` で届くので、受けてから `page.render` で描き直す
  (全員が同じ順で同じ値を見る)。`joined` / `left` で参加者の行、`closed` で「この端末だけ」に切り替え、`resized` で投票の棒の長さを直す
- **1 人で開いたとき**(`page.open` に `session` が無い / `closed` のあと): 同じ画面を `storage.safe` の `board`
  (`{counter, vote, guestbook}`)で動かす。`shared.*` は呼ばない。🎉 は自分にだけトースト

## このリポジトリの形(`rasq` ワークスペース)

`rasq-sdk-sample` の `docs/cli.md` §2 のワークスペース。`rasq init` / `rasq new page` が作る形に合わせてある。

```
rasq.toml                                # schema = "rasq.workspace/1"、[[plugins]] に name/slot/id、権限の理由
Cargo.toml                               # [workspace](rasq CLI が rasq.toml から作り直す)
kit/                                     # rasq-plugin-kit の写し(正典は SDK の rust/rasq-plugin-kit。v0.18 の host::page_render / shared_* 入り)
plugins/custom-page-plugin/
  manifest.json                          # plugin.manifest の答えそのもの
  src/lib.rs                             # manifest.json を include_str! で返し、page を実装
dist/                                    # rasq build --raw の出力(.rasqpkg と生の .wasm)
```

## マニフェスト

```json
{ "schema": "rasq.plugin-manifest/1", "id": "sample.board", "name": "みんなのボード", "version": "0.1.0",
  "kind": "view", "slots": ["page"],
  "permissions": ["render", "shared.session", "storage.safe"],
  "page": { "title": "みんなのボード", "icon": "dashboard", "mode": "shared",
            "surfaces": ["page", "astell", "profile"], "maxParticipants": 50 } }
```

`surfaces`: 単独のページ(招待リンクで人を呼べる)/ AsTell のルームにデプロイ / RASQ アカウントのプロフィールのタイル(持ち主ごとの
1 セッションを訪問者で共有)。

## 契約(`docs/slots.md` §2.13)

| fn | args | 戻り / 副作用 |
|---|---|---|
| `plugin.manifest` | `{}` | `manifest.json` |
| `page.open` | `{token, surface, context: {roomId?, profileHandle?, profileAccountId?}, me: {accountId, handle, displayName}, session?, size: {width, height}}` | `{root, title}`(`title` = 「みんなのボード」/ ルームでは「ルームのボード」/ プロフィールでは「@<handle> さんのボード」)。`session` 無しなら `storage.get board` |
| `page.event` | `{token, type: "var", key, value, rev, by}` / `{token, type: "conflict", key, value, rev}` | 変数を置き換え(`null` = 消す)→ `page.render` |
| | `{token, type: "call", fn: "celebrate", args, from}` | `toast` → `page.render`(届いた 🎉 の数) |
| | `{token, type: "joined", participant}` / `{token, type: "left", accountId}` | 参加者の行 → `page.render` |
| | `{token, type: "closed", reason}` | お知らせ + `storage.get board` → この端末だけで `page.render` |
| | `{token, type: "resized", size}` | `page.render` |
| `page.close` | `{token, reason}` | 状態を捨てる |
| `counter.incr` | `{token, nodeId, kind: "tap"}` | shared: `shared.op counter incr 1` / solo: `storage.set` → `page.render` |
| `poll.vote` | `{token, nodeId, kind: "change", value: 0..2}`(`RadioListTile`) | shared: `shared.set me.vote` / solo: `storage.set` → `page.render`。範囲外は `D01` |
| `poll.clear` | `{token, nodeId, kind: "tap"}` | shared: `shared.set me.vote null` / solo: 同上 |
| `gb.input` | `{token, nodeId, kind: "change", value}`(`Input`) | 下書きを持つだけ(描き直さない) |
| `gb.send` | `{token, nodeId, kind: "confirm", value}`(`Input`)/ `{token, nodeId, kind: "tap"}`(「書く」= 下書き) | shared: `shared.op guestbook append {text, by}` + 入力欄を空に `page.render` / solo: `storage.set` → `page.render`。空なら `toast` |
| `gb.quick.0..2` | `{token, nodeId, kind: "tap"}` | 定型文で `gb.send` と同じ |
| `celebrate` | `{token, nodeId, kind: "tap"}` | shared: `shared.call celebrate {}` / solo: `toast` → `page.render` |
| その他 | | `C01` |

- RRL のイベントの args は **`{token, nodeId, kind, ...payload}`**(payload は平らに展開)。`token` が今のページと違う
  イベント / `page.event` は無視する(古いページ)。RRL のイベントで `token` が**無い**ものは今のページへ通す(`rasq call --then` 用)。
- `shared.*` が断られた(`ok: false`、セッションが無い / 送りすぎ)ときは「送れませんでした」のトースト。

### 使う RRL ノード

`Scroll`(縦)> `Column` の中に: `Text`(`size` / `bold` / `color`)、`SectionHeader`(`title`)、`Row`、`Spacer`、`Button`(`label`、`on.tap`)、
`RadioListTile`(`title` / `subtitle` / `value` / `group`、`on.change` → `{value}`)、`Box`(`width` / `height` / `radius` / `color`。投票の棒と
お知らせの帯)、`Input`(`hint` / `value` / `maxLength`、`on.change` / `on.confirm` → `{value}`)、`Scroll`(横。定型文のボタン)、`ListTile`(`title` / `subtitle`。寄せ書き)。

## ビルドと確認

`rasq` CLI は `rasq-sdk-sample` から組む(`cd rust && cargo build --release -p rasq-cli`。契約 v0.18 が要る)。

```sh
rasq check                 # wasm32 ビルド → export 検査 → manifest 往復 → スキーマ / 登録簿 → cargo test
rasq build --raw --no-sign # dist/sample.board-0.1.0.rasqpkg と dist/custom_page_plugin.wasm
rasq inspect dist/sample.board-0.1.0.rasqpkg
```

native テスト(`cargo test`)は `set_host_hook` でホスト関数を差し替え、shared の一巡(`+1` → `shared.op`、`var` で描き直し、
投票の集計と取り消し、寄せ書きの append と入力欄のクリア、🎉 の `shared.call` と届いた `call` のトースト、`joined` / `left`、古い token の無視、
`closed` で端末だけに切り替え)と solo の一巡(`storage.set` の中身)を見ている。

ビルド済みの wasm をそのまま叩く:

```sh
rasq call custom-page-plugin page.open --args @open_shared.json \
  --reply 'shared.op={}' --reply 'shared.call={}' --then counter.incr --then celebrate
# page.open    → {root, title: "ルームのボード"}
# counter.incr → host shared.op {key: "counter", op: "incr", value: 1}
# celebrate    → host shared.call {fn: "celebrate", args: {}}

rasq call custom-page-plugin page.open \
  --args '{"token":"p1","surface":"page","context":{},"me":{"accountId":"a1","displayName":"花子"},"size":{"width":400,"height":800}}' \
  --reply 'storage.get={"value":null}' --then counter.incr --then celebrate
# page.open    → host storage.get {key: "board"}(ひとりモード)
# counter.incr → host storage.set {key: "board", value: {counter: 1, …}} → host page.render
# celebrate    → host toast {message: "🎉 おめでとう!"}
```

`open_shared.json` の `session` は `{id, scope: "room:r1", role, participants: [...], vars: {"counter": {value, rev, by}, "@a1.vote": …, "guestbook": …}}`。
`--reply` を付けないホスト関数は断られた扱いになるので、「送れませんでした」のトーストが出る(これも期待どおり)。

## 端末での試し方(rasq-client)

- 同梱: `rasq-client/assets/plugins/custom_page_plugin.wasm`(`dist/custom_page_plugin.wasm` と同じもの)。
- 開発中: `rasq dev`(または `rasq build --raw`)が出す `dist/` の絶対パスを「WASMプラグイン試験 (dev)」→
  `.wasm のパスからインポート` に入れ、権限 `render` / `shared.session` / `storage.safe` を許可する。
- AsTell のルームでは、参加者が「このプラグインを使いたい」と提案し、ホスト(オーナー / モデレーター)が承認するとルームの
  セッションができる。プロフィールに置くと、訪問者どうしで持ち主ごとの 1 つのボードを共有する。

## 掲載(審査)に出す

`rasq build` が出す `dist/sample.board-0.1.0.rasqpkg` がそのまま掲載できる形。権限の理由は `rasq.toml` の `[listing.safety]`。
掲載されると、ストアのカードの種別(`pluginType`)は **`custom`** になる。
