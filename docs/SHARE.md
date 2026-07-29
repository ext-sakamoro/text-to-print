# LoRA share consent — 何をアップロードするか

text-to-print は Free tier で生成された LOL DSL + 品質シグナルを ALICE-LOL プロジェクトの LoRA 学習セットに opt-in で送信します このドキュメントは **何が送られ、何が送られないか** を透明化するために作成されました

## サマリー

- Free tier は default で **share ON** ですが、Settings > LoRA share でいつでも off にできます
- 現状 Cloudflare Workers backend (#35) は未実装なので、opt-in してもファイルは local dry-run キュー (`{data_dir}/share_dry_run/{uuid}.json`) に保存されるだけで **どこにも upload されません**
- Paid tier (planned) は完全 offline で opt-in 設定自体が存在しません

## 送信されるフィールド (`SharePayload` v1 schema)

`docs/schema/v1/alice_manifest.schema.json` に authoritative schema が定義されています

| フィールド | 内容 | 送信理由 |
|--|--|--|
| `uuid` | 生成 UUID v7 | dedup / provenance |
| `prompt` | user 入力プロンプト (原文) | LoRA 学習ラベル |
| `prompt_lang` | BCP-47 言語タグ (`ja` / `en`) | multi-lingual 学習分離 |
| `llm_model` | `qwen3.5-4b-q4km` etc. | model family 別品質分析 |
| `lol_source` | 生成された LOL DSL 原文 | 学習ターゲット |
| `lol_sha256` | LOL DSL の SHA-256 | dedup 検出 |
| `mesh_sha256` | 出力 mesh (3MF/STL/FBX) の SHA-256 | dedup + provenance chain |
| `quality.success` | 生成成功したか | RLHF-lite label |
| `quality.retry_count` | Stage 8 retry loop の再試行回数 | model quality metric |
| `quality.time_to_file_ms` | 生成開始から file 出力までの ms | latency 分析 |
| `quality.safety_violations` | `alice-physics` の warp/thermal/beam 違反 messages | model の safety awareness 学習 |
| `quality.export_format` | `3mf` / `stl` / `fbx` / `step` / `gcode` | format 別品質分析 |
| `quality.user_kept` | user が結果を保存したか (今後実装) | user preference label |
| `quality.user_edited` | user が LOL を編集したか (今後実装) | correction signal |

## 送信されないもの (明示的除外)

以下は SharePayload に **含まれません** (schema level で禁止):

- Apple / Google / Microsoft アカウント ID
- Machine ID / MAC アドレス / hostname
- ライセンスキー / API key / DID 秘密鍵
- File system パス (data_dir / home dir / etc)
- crash report (opt-out で on にする別 flag `.crash_reports_enabled` で管理、default off)
- P2P share pending publish キュー
- DB manifest_json 内の app_version 以外の environment fields (安全側)

## data_dir 上の実 file

opt-in ON の生成完了時、以下 file が実 backend #35 実装まで local に留まります:

```
{data_dir}/share_dry_run/{uuid}.json    <- SharePayload v1 JSON (500B-2KB/entry)
```

- Settings > LoRA share に "アップロード待ち (dry-run キュー): N 件" として件数が表示されます
- backend deploy 後 (#35 完了) にこれらは順次 upload されます
- opt-out に切り替えても既存 dry-run キューは残ります (手動で削除可)

## Paid tier との違い

- **Free tier**: default share ON、SharePayload dry-run queue に保存 (backend deploy 後 upload)、opt-out で完全遮断
- **Paid tier (planned)**: opt-out 選択 UI 自体が存在せず、何もアップロードされない

## LoRA 学習セットへの貢献

Free tier user の SharePayload は ALICE-LOL project の LoRA fine-tune 学習セット (現状 523+ サンプル) に加わり、以下の quality metrics で fine-tune 対象にされます:

- 生成成功率
- retry_count → 少ないほど high quality
- safety_violations → 少ないほど high quality
- user_kept / user_edited → user preference reward

flywheel: Free user が生成すればするほど、LoRA が改善され、全 user (Free + Paid) が恩恵を受けます

## opt-out 手順

1. app 起動 → Settings タブ
2. "LoRA share" セクションを展開
3. "LoRA 学習データ提供に協力する" checkbox を off
4. 変更は即 DB に永続化されます (再起動不要)

opt-out 後は SharePayload は生成されず、dry-run キューにも追加されません

## 関連 issue / 実装

- **Epic #3** Stage 5: Ko-fi Supporter tier + LoRA share opt-out UI
- **#18** T5.2: opt-out toggle 実装 (完了)
- **#42** SharePayload schema conformance test (完了)
- **#44** GAP-3: SharePayload upload path (backend #35 待ち)
- **#35** [Epic-Infra] Cloudflare Workers upload backend (未実装)
