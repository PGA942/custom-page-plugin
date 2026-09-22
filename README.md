# custom-page-plugin(サンプルなし — スロット `page` は計画中)

AsTell や一部 SNS の独自機能(Misskey Play など)を移植するための **ページ型プラグイン**(`kind: view`)の置き場。
ページ型はホスト側のルーティング(`custom:<pluginId>` のページ)と一緒に決める必要があるため、
他のスロットが実機で回ってから最後に着手する。**このリポジトリにはまだ実装が無い。**

## いまの状態(SDK 側)

契約と登録簿の正典は [`rasq-sdk-sample`](https://gitlab.com/orgate-question/rasq-sdk-sample)。`page` は
**`status: planned`**(面の id だけ予約されている状態)で、[`schema/slots.json`](https://gitlab.com/orgate-question/rasq-sdk-sample/-/blob/main/schema/slots.json)
に次のところまで書かれている:

| 項目 | 予約されている内容 |
|---|---|
| tier / kinds | `page` / `view` |
| 差し込み先 | `screen.fullscreen`、ルート `custom:<pluginId>` |
| 制約 | app-owned の back / escape を奪えない / BottomNavigation・safe area はホスト / ホストのルーティングと一緒に決める |
| call | `page.open` / `page.close` |
| ホスト関数 | `fow.open` / `fow.close` / `storage.get` / `storage.set` / `storage.delete` / `toast` / `url.open` |
| 権限 | 必須 `render`、任意 `storage.safe` / `network` |
| ライフサイクル | `stateScope: screen` / `offscreenPolicy: suspend` |
| フォールバック | `loading` / `timeout` / `crashed` / `unsupported` |

マニフェストの例は [`examples/manifests/sample.custom-page.json`](https://gitlab.com/orgate-question/rasq-sdk-sample/-/blob/main/examples/manifests/sample.custom-page.json)
(`id: sample.custom-page` / `kind: view` / `slots: ["page"]` / `permissions: ["render", "storage.safe"]`)。

`page.open` / `page.close` の引数と戻りはまだ決まっていない(`docs/slots.md` §2 に節が無い)。
**`rasq new page <name>` もまだできない** — `rasq registry templates` のとおり雛形(`templates/page/`)が
SDK に無いため(`--allow-planned` を付けても `error: slot page の雛形はまだありません` で止まる)。
実装を始めるのは、① `docs/slots.md` に `page` の節が入り、② `templates/page/` ができてから。

ほかの 12 リポジトリのように `rasq` ワークスペースへ作り直す作業も、実装ができてからになる
(手順は `rasq-sdk-sample/docs/cli.md` §2)。

## この先の入口

- 契約(正典): [`docs/slots.md`](https://gitlab.com/orgate-question/rasq-sdk-sample/-/blob/main/docs/slots.md) — このリポジトリの [`CONTRACT.md`](CONTRACT.md) はそこへのポインタ
- 機械可読な登録簿: `rasq-sdk-sample/schema/slots.json`(`page` の項)
- wasm ABI / RRL / FOW / ホスト関数: `rasq-sdk-sample/PROTOCOL.md`
- CLI: `rasq-sdk-sample/docs/cli.md`(`rasq init` / `new` / `check` / `call` / `build` / `inspect`)
