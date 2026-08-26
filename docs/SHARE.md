# LoRA share consent — 何をアップロードするか

text-to-print は Free tier で生成された LOL DSL + 品質シグナルを ALICE-LOL プロジェクトの LoRA 学習セットに opt-in で送信します このドキュメントは **何が送られ、何が送られないか** を透明化するために作成されました

## サマリー

- Free tier は default で **share ON**、Settings > LoRA share でいつでも off にできます
- Cloudflare Workers relay (`/api/share`) は稼働中 (Sprint X.1 で custom domain `text-to-print.alicelaw.net` に配線済)
- Gallery Phase 2 (2026-08-26) で生成完了ごとの公開確認 dialog が入りました default は **公開確認 dialog**、Settings > LoRA share > 「毎回自動公開」toggle で dialog を bypass 可能
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

opt-in ON の生成完了時、以下 file が local に書き出されます:

```
{data_dir}/share_dry_run/{uuid}.json    <- SharePayload v1 JSON (500B-2KB/entry、local audit 永続)
{data_dir}/share_queue/{uuid}.json      <- Cloudflare upload 待ちキュー (drain 後削除)
```

- Settings > LoRA share に「アップロード待ち: 実キュー N 件 / dry-run N 件」として件数が表示されます
- 起動時に `retry_queued_uploads` が `share_queue/` を Cloudflare relay に送信、成功したものは削除されます
- `share_dry_run/` は local audit 用に残ります (opt-out 切替後も残る、手動で削除可)

## Gallery Phase 2 — 公開確認 dialog (2026-08-26 追加)

Free tier + LoRA share ON + `gallery_auto_share` OFF の 3 条件で、生成完了時に modal が出ます

```
┌───────────────────────────────────────────┐
│ Gallery に公開しますか?                    │
│                                            │
│ この生成物 (LOL DSL + prompt + 品質信号)   │
│ を Gallery に公開しますか?                 │
│ 公開すると他の user から fork / 参考に     │
│ される可能性があります                     │
│ 公開しなくても local audit ログには残ります│
│                                            │
│ [今回だけ公開] [公開しない]                │
│ [毎回自動公開 (以降 dialog 出ない)]        │
└───────────────────────────────────────────┘
```

- **今回だけ公開**: dry-run + `share_queue/` enqueue、次回の generation で再度 dialog
- **公開しない**: dry-run のみ (local audit)、`share_queue/` には積まない
- **毎回自動公開**: DB `profiles.gallery_auto_share = 1` に永続化、以降 dialog bypass Settings で off に戻すまで有効

Paid tier では tier check で dialog に到達しません (常時 upload なし)

## Gallery Phase 3 — Cloudflare relay endpoint (2026-08-26 追加)

Gallery タブから見える list / publish / delete は Cloudflare Workers 経由で D1 database `text-to-print-gallery` にアクセスします

| Endpoint | 用途 | 認証 |
|--|--|--|
| `GET  /api/gallery/list?limit=N&offset=N` | 最新順 pagination (default 100 max 500) | 認証不要 (public read) |
| `POST /api/gallery/publish` | Gallery に post 追加 (rate limit + validate + insert) | ed25519 sig verify + DID match |
| `DELETE /api/gallery/{id}` | 自 post 削除 (soft delete = `deleted_at` set) | ed25519 sig verify + author_did match |

### 送信フィールド (Gallery publish wire schema)

| フィールド | 内容 | 上限 |
|--|--|--|
| `id` | UUID v7 (client 生成) | 36 char |
| `author_did` | `did:key:<64hex>` (ed25519 pub key) | 72 char |
| `author_nickname` | Settings で入力した表示名 (Optional) | 32 char |
| `lol_source` | LOL DSL 原文 | **100 KB** (rate limit 起動負荷抑制) |
| `created_at` | ISO-8601 UTC | 20-40 char |
| `signature` | ed25519 sig hex (canonical msg 上署名) | 128 char |

### 署名 canonical msg (replay-protection 込み)

publish と delete で prefix を分離して、publish sig を delete に流用できないよう設計:

```
publish: {id}|{author_did}|{lol_source}|{created_at}
delete:  delete|{id}|{author_did}
```

### Rate limit

`/api/share` と同 counter (SHARES_DB `rate_limit_counters`) を prefix 分離で相乗り:

- `gallery_uuid:<id>` — 1 id あたり `PER_UUID_HOURLY` (default 10) / 1h
- `gallery_ip:<sha256(ip+salt)>` — 1 IP あたり `PER_IP_HOURLY` (default 100) / 1h

### 送信されないもの (Gallery でも同じ除外方針)

- prompt (Gallery publish は LOL DSL のみ、prompt は SharePayload 側)
- quality signals (retry_count / time_to_file_ms / safety_violations 等)
- 実 IP address (`SHA256(ip + IP_HASH_SALT)` hex のみ counter に保存)

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
2. 「LoRA share」セクションを展開
3. 「LoRA 学習データ提供に協力する」checkbox を off
4. 変更は即 DB に永続化されます (再起動不要)

opt-out 後は SharePayload / Gallery publish どちらも生成されず、dry-run / share_queue にも追加されません

## nickname 設定 (Gallery 表示名)

Gallery タブで DID hex の代わりに表示される名前を Settings で入力可能:

1. app 起動 → Settings タブ
2. 「プロフィール (Gallery 表示名)」セクションを展開
3. TextEdit に名前入力 (最大 32 char、空欄なら DID short-form を表示)
4. focus を外すと即 DB `profiles.nickname` に永続化

変更しても過去に公開した post には反映されません (最新の nickname は次回公開時から反映)

## Gallery 自 post 削除

Gallery タブで自 post のみ🗑削除 button が表示され (author_did が local Identity と一致するときのみ):

1. Gallery タブで自 post を選択
2. 「🗑 削除 (自分の post)」button クリック
3. ed25519 sig で delete-scoped canonical msg 署名 → Cloudflare relay に DELETE 送信
4. 成功なら D1 で soft delete (`deleted_at` set)、UI 側は auto refresh で消える

## 関連 issue / 実装

- **Epic #3** Stage 5: Ko-fi Supporter tier + LoRA share opt-out UI
- **#18** T5.2: opt-out toggle 実装 (完了)
- **#42** SharePayload schema conformance test (完了)
- **#44** GAP-3: SharePayload upload path (完了、Cloudflare Workers `/api/share` 稼働中)
- **#35** [Epic-Infra] Cloudflare Workers upload backend (完了、Sprint X.1 で deploy 済)
- **Gallery Phase 1-3** (2026-08-26 完了、commit `c97a09b` / `36e9e0f` / `e9aa721` / `beaed29`): nickname + confirm dialog + Cloudflare relay endpoint + fork/delete UI
