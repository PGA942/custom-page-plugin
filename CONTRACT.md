# RASQ スロットプラグイン契約 — 正典は rasq-sdk-sample

この文書は **`rasq-sdk-sample/docs/slots.md`** に移った(v0.8)。以前はここに同じ内容が複製されていたが、
13 リポジトリでずれるのをやめ、SDK 側を正典にした。

- 契約の本文(§1 マニフェスト / §2.1〜2.10 スロット別 / §3 RRL のホスト拡張 / §4 ホスト関数 / §5 ホスト側の読み込み):
  https://gitlab.com/orgate-question/rasq-sdk-sample/-/blob/main/docs/slots.md — **節番号は同じ**(README の「§2.x」はそのまま読める)
- 機械可読な登録簿(スロットごとのティア / call / ホスト関数 / 権限 / ライフサイクル): `rasq-sdk-sample/schema/slots.json`
- マニフェスト: `rasq-sdk-sample/schema/manifest.schema.json`(v0.3)。ActionRef / ActionKey: `schema/action-ref.schema.json`
- 検証: `python3 schema/validate.py`(`examples/manifests/` にこのサンプルのマニフェスト例がある)
