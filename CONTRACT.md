# RASQ スロットプラグイン契約 v0.3(plugin-sample)

`rasq-sdk-sample/PROTOCOL.md`(wasm ABI / RRL / FOW / ホスト関数)の上に、**アプリの各所へ
差し込む「スロット」**を定義する。プラグインは wasm32 cdylib(`rasq-plugin-kit` の
`rasq_plugin!` で export)で、ホスト(rasq-client D-167〜)は「WASMプラグイン試験 (dev)」の
**スロットに登録**で読み込み、宣言されたスロットの場所から `call` する。

原則:

- **データはすべて `call` の args で渡す**(ホスト関数で状態を問い合わせない)。
- プラグインからホストへの働きかけ(共有 / 置換 / コピー / 通知)は**ホスト関数**で、
  **fire-and-forget**(戻りは受理 `{}` だけ。表示や反映は非同期)。
- 画像などのバイナリは JSON 内の **base64**(`bytesB64` + `mimeType`)。
- UI はすべて **FOW**(`fow.open`、`fow.widget` 権限)か、スロットが受け取る RRL。
- 未対応の関数は `C01` を返す(ホストは該当 UI を隠す)。

## 1. マニフェスト — `call("plugin.manifest", {})`

```json
{ "schema": "rasq.plugin-manifest/1",
  "id": "sample.composer-media", "name": "メディア加工(サンプル)", "version": "0.1.0",
  "kind": "action", "description": "ガンマ / 回転 / 色相 / クロップ",
  "slots": ["composer.media"],
  "permissions": ["fow.widget", "storage.safe"] }
```

- `kind` は `schema/manifest.schema.json` の enum(`sns` / `action` / `view` / `trend` / `mediaViewer` / `postProcessor`)。
- `slots` = この契約のスロット id(下表)。複数可。
- `permissions` = ホスト関数の権限(`fow.widget` / `storage.safe` / `network` / `mute.words` …)。
  ホストはスロット登録時にユーザーへ表示し、許可した分だけ通す。

| slot | 差し込み先 | ホスト → プラグイン(`call`) | プラグイン → ホスト(ホスト関数) |
|---|---|---|---|
| `astell.action` | AsTell ルーム FOW 下段の **プラグインアクション** ボタン | `astell.open` / `astell.event` | `astell.share` / `sound.play` / `toast` |
| `composer.media` | コンポーザー添付タイルの **… → プラグインで加工** | `media.edit` | `composer.media.replace` / `toast` |
| `composer.text` | コンポーザー本文の変化(デバウンス 200ms) | `text.changed` | `composer.text.replace` / `toast` |
| `viewer.overlay` | メディア全画面ビューワー(画像の上に重ねる) | `viewer.open` / `viewer.close` | `viewer.overlay.set` / `clipboard.set` / `url.open` / `mute.add` / `toast` |
| `post.transform` | 全 TL の表示直前(投稿加工) | `post.transform` | — |
| `tl` | ホーム TL のカスタムタブ | `tl.timelines` / `tl.sort` | — |
| `trend.suggest` | 検索ホームのトレンド節(サジェスト行) | `trend.suggest` | — |
| `post.share` | 投稿の **共有 FOW** の「プラグイン」節(v0.3、宣言だけで自動登録) | `post.share` / `post.share.close` | `composer.open` / `clipboard.set` / `url.open` / `astell.share` / `toast` |

共通: `lifecycle {kind, surface?}` / FOW の `onClosed` は PROTOCOL どおり。

## 2. スロット別の詳細

### 2.1 `astell.action` — AsTell プラグイン

- `astell.open {token, room, me, peers, shares}` — ユーザーがプラグインアクションからこのプラグインを選んだ。
  `room` = `AstellRoom` ワイヤ(`id, title, ownerId, genres, …`)、`me` = `{accountId, peerId, handle, displayName}`、
  `peers` = `[{peerId, accountId, handle, displayName, micOn}]`(自分は含まない)、
  `shares` = 直近 10 件の共有(**古い → 新しい**、`[{id, kind, payload, displayName, handle, createdAt}]`)。
  `kind: "post"` の payload はアプリの投稿共有(D-166)と同じ `{url, sns, instance, postId, author(文字列), handle, avatarUrl?, body, thumbnailUrl?, media[], createdAt?}`。
  プラグインはここで `fow.open`(操作 UI)を出す。戻り `{}`。
- `astell.event {token, type, ...}` — FOW を開いている間の通知。`type` = `share`(`share`)/ `transcript`(`transcript`)/
  `peer-joined` / `peer-left`(`peer`)/ `closed`。
- `astell.share {kind: "text"|"link"|"media"|"post", payload}` — ルームへ共有(ホストが WS の `share` にする)。
  `media` は `{name, mimeType, bytesB64}` でも `{url, mimeType, title}` でも可 — `bytesB64` はホストがアセットにアップロードして URL にする。
  効果映像 = 生成した PNG/GIF を `media` で流す。
- `sound.play {kind: "click"|"alert"}` — 端末のシステム音(効果音)。ホストは `SystemSound` を鳴らす。
- 投稿タイルデータの受け渡し(投稿の … → 「AsTell で共有」経由)は `astell.event {type:"share", share:{kind:"post", payload}}` で届く。

### 2.2 `composer.media` — メディア加工(v0.2: 専用の全画面エディター)

ホストは添付の「プラグインで加工」で**専用の全画面エディター**を開く: 中央に画像(画面の 70%。下付き FOW が
開くと小さくなる)、左に第 1 階層の丸ボタン列(選んだジャンルのアクション)、右に縦のフローティング列
(上端の ↓ でプラグイン / 編集ジャンルを選ぶ。選ぶと左の列が入れ替わる)、ヘッダーに キャンセル / 適用。

- `media.edit {token, index, name, mimeType, width?, height?, bytesB64}` — 添付 1 枚。戻り:
  - **エディター形**: `{genres: [{id, label, icon?, actions: [{id, label, icon?}]}]}`。ホストがジャンル/アクションの列を描く。
    `icon` はホストのアイコン名(`rrlIconFor` と同じ。`tune` / `rotate_left` / `crop` …)。
  - すぐ返す形(UI 無し): `{bytesB64, mimeType, name?}` → その場で差し替え。
  - 旧形(`{}` を返して自分で `fow.open`)も動く(FOW はエディターの下付きになる)。
- `media.action {token, genre, action}` — 第 1 階層のボタン。即時の操作(回転など)ならその場で処理して
  `composer.media.preview` を呼ぶ。パラメーターが要るなら `fow.open`(第 2 階層 = **エディターの下付き FOW**、
  スライダー等)。スライダーの `change` ごとにプレビューを更新してよい(長辺 ≤ 360 の小さい画像で)。戻り `{}`。
- `composer.media.preview {token, bytesB64, mimeType}`(ホスト関数)— 中央のプレビューを差し替える(何度でも)。
- `media.commit {token}` — 適用。**戻りに本サイズの結果** `{bytesB64, mimeType, name?}` を返す
  (ホストが添付を差し替えてエディターを閉じる)。`composer.media.replace` でもよい。
- `media.cancel {token}` — キャンセル(FOW を閉じ、状態を捨てる)。戻り `{}`。
- 対象の `mimeType` は `image/png` / `image/jpeg`(他は `C01` でよい)。`name` は差し替え後のファイル名(拡張子を変えてよい)。
- `SliderListTile.p.value` を `setProp` で書き換えるとスライダーのつまみも動く(ホストのタイルは props 駆動)。

### 2.3 `composer.text` — テキスト加工 / 補完

- `text.changed {token, text, selectionStart, selectionEnd, trigger?}` — 本文が変わった(デバウンス)。
  `trigger` = カーソル直前のトークン(`#タグ` / `@handle` / `:emoji`)があればホストが**文字列**で渡す
  (範囲は `[selectionStart − trigger.length, selectionStart)`、UTF-16 code unit)。空白/改行/全角空白で区切る。
  プラグインは条件(例: `#` で始まるトークン)を見て `fow.open`(候補一覧)し、そうでなければ `fow.close`。戻り `{}`。
- `composer.text.replace {token, start, end, text}` — 本文の `[start, end)` を `text` に置き換える(UTF-16 code unit オフセット、Dart の `String` と同じ)。
- ホストは FOW をコンポーザーの子(`parentId` 無しでも同じカラム)として開くので入力は隠れない。

### 2.4 `viewer.overlay` — 全画面ビューワーの重ね

- `viewer.open {token, url, mimeType, width, height, bytesB64?}` — 全画面表示が開いた。`bytesB64` は画像本体
  (ホストが取れた場合のみ。大きい画像はホストが長辺 1024 に縮小してから渡し、`width/height` はその縮小後の画素数)。
  戻り `{root?: RRL}` — 画像の上に重ねる RRL(座標は `width/height` の画素系。ホストが表示倍率に合わせて拡縮)。
  後から差し替えるときは `viewer.overlay.set {token, root}`。
- `viewer.close {token}` — 閉じた(FOW も閉じること)。
- `viewer.open` の失敗(壊れた入力 = `D01`)はホストが黙って無視する(重ねない)。「何も無い」は `{}`。
- ホストは重ねる `Stack` を画像の画素数(`width/height`)のサイズで置き、表示倍率に合わせて拡縮する。
- 重ねる RRL は `Stack` の子に **`Box`**(ホスト拡張ノード、§3)を `left/top/width/height` で置く。`on: {tap}` でプラグインへ戻る。
- リンク FOW からは `clipboard.set {text}` / `url.open {url}` / `mute.add {text}`(権限 `mute.words`。ホストは確認してからミュート語に追加)。

### 2.5 `post.transform` — 投稿加工(PROTOCOL §4.4 と同じ)

- `post.transform {posts: [Post ワイヤ]}` → `{posts: {postId: {body?, cw?}}}`。載っていない投稿は無加工。

### 2.6 `tl` — タイムライン = **アルゴリズムノード**(v0.2)

TL プラグインはタブではなく、簡略カスタマイズ UI / ノードグラフの**アルゴリズム部品(ソートノード)**として並ぶ。
パラメーターは「ノードを設定」FOW にプラグインの RRL 設定ページが差し込まれる。

- `tl.timelines {}` → `{timelines: [{id, label, base}]}`(`label` = 部品名「〇〇でソート」の元。`base` は互換用)。
- `tl.params {id}` → `{root: RRL, defaults: {key: value}}` — 設定 DSL のサブセット(`SectionHeader` / `RadioListTile` /
  `SwitchListTile` / `SliderListTile` / `ValueListTile` / `Text`)。値は Bind `$.settings.<key>`、書き込みは
  `on: {"change": "settings.set"}`(ホストが部品のパラメーターに保存する。ノード key = パラメーター名)。無ければ `C01`。
- `tl.sort {id, posts, params}` → `{order: [postId…]}` — `params` は保存されたパラメーター(無ければ `defaults`)。

### 2.7 `trend.suggest` — トレンドのサジェスト

- `trend.suggest {recent: [string], query?: string, limit?: number}` → `{suggestions: [{text, reason?}]}`。
  `recent` = 検索ホームが今持っているトレンド語(タグ)。サーバー無しでも動くよう、ローカルの規則(共起 / 時間帯 / 固定辞書)で返す。

### 2.8 `post.share` — 投稿共有(v0.3: 共有 FOW からインラインで編集 & 共有)

投稿のアクションバー右端(共有)で**共有 FOW** が開く: ヘッダー左にアカウント選択(非公開投稿は受け取った
アカウントだけ)、本体に 内容をコピー / リンクをコピー / 他のアカウントで共有(選んだアカウントのコンポーザーに
リンクを初期入力)、**プラグイン**節、DM へ共有、ブラウザで開く。`post.share` を `slots` に宣言したプラグインは
**権限(マニフェスト)から自動で**プラグイン節に 1 行ずつ並ぶ(`name` / `description`)。

- `post.share {token, post, account, link}` — ユーザーがこのプラグインの行を選んだ。
  `post` = §2.1 と同じ投稿 payload(`{url, sns, instance, postId, author, handle, avatarUrl?, body, thumbnailUrl?, media[], createdAt?}`)、
  `account` = ヘッダーで選んだアカウント `{sns, instance, handle}`、`link` = 投稿の Web URL。
  プラグインはここで `fow.open`(**共有 FOW の子 = インライン**。共有 FOW が閉じれば連動して閉じる)を出し、
  文面の編集(テンプレート / 引用の付け方 …)と共有先のボタンを描く。戻り `{}`。
- `post.share.close {token}` — 共有 FOW が閉じた(プラグインは FOW を閉じて状態を捨てる)。戻り `{}`。
- `composer.open {token, text, mediaB64?, mimeType?, name?}`(ホスト関数)— **選んだアカウントのコンポーザー**を
  `text` を初期入力して開く(画像を添えるときは `mediaB64` + `mimeType`)。ホストは共有 FOW とプラグイン FOW を閉じてから開く。
- ほかに `clipboard.set` / `url.open` / `astell.share`(参加中の AsTell があれば)/ `toast`。

## 3. RRL のホスト拡張(rasq-client D-167 以降。古いホストは無視/⚠ボックス)

- `Image.p.src` に **`data:image/png;base64,…`** を受ける。`p.width` / `p.height` で大きさ指定。
- `Stack` の子で `p.left` / `p.top`(+ `width` / `height`)があるものは絶対配置(`Positioned`)。
- 新ノード **`Box`**: `p: {width, height, color, borderColor, borderWidth, radius}`、子を持てる。`on: {tap}` 可。
- `Text.p` に `size` / `bold` / `color`。色は `#RRGGBB` か `#RRGGBBAA`(8 桁は末尾が不透明度)。`Icon.p.color` も同じ。
- L1 ノード(`Text` / `Box` / `Image` …)の `on: {tap, longPress}` はホストが GestureDetector で拾う。

## 4. ホスト関数の追加(すべて fire-and-forget、戻り `{}`)

| fn | args | 権限 |
|---|---|---|
| `toast` | `{message}` | — |
| `sound.play` | `{kind: click\|alert}` | — |
| `clipboard.set` | `{text}` | — |
| `url.open` | `{url}`(http/https のみ) | — |
| `mute.add` | `{text}` | `mute.words`(ユーザー確認つき) |
| `composer.open` | `{token, text, mediaB64?, mimeType?, name?}`(§2.8) | — |
| `astell.share` | `{kind, payload}` | スロット `astell.action` |
| `composer.media.replace` | `{token, bytesB64, mimeType, name?}` | スロット `composer.media` |
| `composer.media.preview` | `{token, bytesB64, mimeType}` | スロット `composer.media` |
| `composer.text.replace` | `{token, start, end, text}` | スロット `composer.text` |
| `viewer.overlay.set` | `{token, root}` | スロット `viewer.overlay` |

`token` はホストが `call` で渡した値をそのまま返す(古い呼び出しに対する反映を捨てるため)。

## 5. ホスト側(rasq-client)の読み込み

- 「WASMプラグイン試験 (dev)」→ `.wasm` を読み込み → **スロットに登録**。登録はパスを設定に残し、次回起動でも読み直す。
- 登録されたプラグインはワーカー isolate で常駐し、各スロットの場所(AsTell の下段ボタン / 添付タイルの … / コンポーザー / ビューワー / TL / トレンド)から `call` される。
- 権限外のホスト関数は `E01`。スロットに宣言の無い `call` は来ない。
- **FOW の着き方(v0.2)**: プラグインの `fow.open` は、呼び出し元の面の**子**として開く(コンポーザー → MFM ピッカーと同じ
  インライン FOW、AsTell ルーム FOW → その下、メディアエディター → 下付き)。親が閉じれば連動して閉じる。
