# text-to-print Roadmap

target: **v1.0.0 商用出荷** (Paid tier + LoRA flywheel)

現状 (2026-08-07): core パイプライン完成 (text → LLM → LOL → SDF → mesh → MakerWorld 対応 .3mf、cargo test workspace 207/207 pass) release automation 完成 (.tar.gz / .zip / .msi / .deb / .AppImage + macOS codesign + notarize) 残タスクは docs / infra deploy / paid tier

## Milestone 全景

| version | 到達水準 | 対象 phase |
|--|--|--|
| **v0.1.0 β release** | 公開可能な最小 form、GitHub Releases 公開開始 | P0 のみ |
| **v0.1.0 GA** | production 品質、実 MakerWorld upload 検証済、backend infra 全 deploy 済 | P0 + P1 |
| **v1.0.0 商用** | Paid tier 課金稼働、LoRA flywheel 自動化、複数 printer / filament template 対応 | P0 + P1 + P2 |

---

## P0 — v0.1.0 β release blocker (数時間〜1 日想定)

公開に直接必要な docs / メタデータ更新 code 変更なし

### P0-1: `docs/images/` に screenshot / demo GIF 追加

- [ ] `docs/images/screenshot-generate.png` — Generate tab (prompt → LOL → 3MF preview)
- [ ] `docs/images/screenshot-settings.png` — Settings tab (LoRA share opt-out + queue status)
- [ ] `docs/images/demo.gif` — 10 秒 `sphere(20)` prompt → 3MF export デモ

capture 設定は `README.md` §Screenshot submission 参照 (1600×1200 / macOS Dark / max 5 MB / ~15 fps)

**受入基準**: README がリンク切れなく画像表示される

### P0-2: `README.md` §Export formats 表 更新

現状 (line 109) が Phase 5.4 以前の記述:

- [ ] `**3MF** | alice_bamboo::export_to_3mf` → `alice_bamboo::bambu_3mf::export_bambu_3mf (MakerWorld 対応 12-file zip 3MF、Phase 5.7)` に更新
- [ ] `**3MF (4-color)**` 行も現行 API 名を確認 (`alice_bamboo::color4::quantize_to_4color` は現行のまま)
- [ ] `**STL** / **FBX**` の producer が `alice_lol::print_export::*` になっているが、Stage 4 で `alice_bamboo::print_export::*` 経由に集約済 → 更新

**受入基準**: 実 code の `use` statement と README の表が一致

### P0-3: `README.md` §Status / §LoRA training 数値更新

- [ ] "Standalone pivot in progress (2026-07-29)" → "Standalone desktop app (v0.1.0 β)"
- [ ] "523 sample training set" → 実測 (`wc -l datasets/lol_train.jsonl` で確認)
- [ ] Recent changes section に Phase 5.4 (bambu_3mf 経路切替) と Phase 5.7 (Rust template embed 完了) を追記
- [ ] Repository layout 表に `crates/worker/` (CF Workers wasm32) を追加

### P0-4: `CLAUDE.md` stats 更新

- [ ] "テスト数 80" → "207" (実測 `cargo test --workspace`)
- [ ] クレート構成表に `crates/worker/` 追加 (現状 4 crate 表記、実 5 crate)
- [ ] `app 6 / core 25 / llm 12 / network 37 = 合計 80` → 実測値 `9 + 73 + 4 + 64 + 52 + 5 = 207`

### P0-5: v0.1.0 tag + GitHub Release 作成

P0-1〜P0-4 完了後:

- [ ] `Cargo.toml` の `workspace.package.version` を `0.1.0` に設定
- [ ] `git tag v0.1.0 && git push origin v0.1.0`
- [ ] `.github/workflows/release.yml` が build + sign + notarize + artifact upload を自動実行
- [ ] GitHub Releases page で `.tar.gz` / `.zip` / `.msi` / `.deb` / `.AppImage` 全 5 artifact 揃うか確認

**受入基準**: 一般 user が macOS / Windows / Linux のインストーラーを DL してアプリを起動できる

---

## P1 — v0.1.0 GA (production 品質、数日〜2 週想定)

LLM backend polish + backend infra deploy + 実機検証

### P1-1: `crates/llm/src/embedded_backend.rs` の server.rs per-layer orchestrator 移植

CLAUDE.md 「残候補 (次 session)」より、Qwen 3.5 hybrid DeltaNet + Attention の GPU native 対応 現状 Stage 3-C.15/3-C.16 で CPU auto fallback、GpuModel::load で明示 Err surface

- [ ] `ALICE-LLM/server/server.rs` の per-layer 実行 orchestrator を `text-to-print-llm::embedded_backend::worker_main_gpu` に移植 (~2000 LOC)
- [ ] DeltaNet layer / Attention layer の per-layer dispatch を Rust GPU 側で実装
- [ ] Qwen 3.5-4B Q4_K_M で GPU 推論成功を verify (Ollama sidecar と同等品質確認)

**受入基準**: Qwen 3.5-4B Q4_K_M model 選択 + Execution Mode = GPU で fallback なしに推論完走

### P1-2: OOM specific detection

- [ ] `worker_main_gpu` の GpuModel::load / GpuEngine 実行時 error を wgpu OOM vs 他 error で分類
- [ ] OOM 検知時のみ CPU fallback + user 通知、他 error は panic path 維持

**受入基準**: GPU VRAM 不足で CPU 落ちる時、log に「OOM 検知 → CPU fallback」と明示

### P1-3: `prompt.rs` の parse_lol retry (grammar 有効時)

- [ ] 現状 grammar 有効時は「事実上ゼロ想定」で retry 未実装
- [ ] LOL_GBNF constrained decoding でも parse 失敗するケース (grammar 抜け穴 / model 出力異常) の retry loop 追加
- [ ] max_retries = 3、失敗時は user に「LLM 出力が LOL DSL として無効」通知

**受入基準**: grammar 有効時でも parse エラーで silent 失敗しない

### P1-4: Crash reporter collector deploy (`crates/core/src/crash_report.rs` TODO)

Epic-Infra #36 の Cloudflare Workers Sentry-alternative collector 未 deploy

- [ ] `crates/worker/` (wasm32-only、CF Workers 用) に crash_report_handler 追加 (`share_handler.rs` と同構造、rate_limit + validate 込み)
- [ ] `wrangler.toml` に route `POST /crash-report` 定義
- [ ] `wrangler deploy` で本番 endpoint 稼働開始
- [ ] `crates/core/src/crash_report.rs` の upload path を実装 (currently TODO 化)
- [ ] opt-in user の crash が collector に届くか e2e 確認

**受入基準**: 実 crash を意図的に発生 → 数分以内に collector 側 D1/KV に record 到達

### P1-5: Share upload endpoint (`crates/worker/src/share_handler.rs`) の CF Workers deploy

- [ ] 現状 `share_handler.rs` 175 LOC + `validate.rs` 261 LOC + `rate_limit.rs` 71 LOC は wasm32 target で build 可能 (workspace 除外設定済)
- [ ] `wrangler deploy` を Cloudflare 本番 account に対して実行
- [ ] R2 bucket / D1 database 準備 (schema は `crates/worker/` 内 or `docs/SHARE.md` 参照)
- [ ] `crates/network/src/share.rs` の `enqueue` → CF Workers endpoint への実 HTTP POST を verify

**受入基準**: Free tier user が opt-in share ON でアプリから 3MF 生成 → CF Workers に POST 成功 → R2 に保存確認

### P1-6: Bonsai27B GGUF 公開 or 代替決定

- [ ] `Project-ALICE/Bonsai-27B-Q1_0-GGUF` (placeholder repo) の実 model 公開判断
- [ ] 公開する場合: HF repo 作成 + Q1_0 GGUF upload + `crates/llm/src/downloader.rs` の `ModelChoice::Bonsai27B` `default_hf_ref` 差替
- [ ] 公開しない場合: UI から Bonsai27B option 撤去 or "coming soon" grey out

**受入基準**: Settings > Model dropdown で Bonsai27B 選択時に自動 DL or 削除の明確な UX

### P1-7: 実 MakerWorld upload 検証

- [ ] text-to-print で生成した .3mf を Bambu Studio で開く → メッシュ表示 + slicer 設定 (H2D + PETG) 反映確認
- [ ] MakerWorld アップロード → プレビュー / thumbnail / project settings 全て表示されるか
- [ ] リジェクトされる場合は `alice_bamboo::bambu_3mf` の生成物と Bambu Studio 保存 .3mf の diff で不足 metadata を特定 → 修正

**受入基準**: MakerWorld アップロード成功 (approved / preview 表示 / DL 可能)

### P1-8: `Cargo.toml` version 0.1.0 GA tag

P1-1〜P1-7 完了後:

- [ ] `git tag v0.1.0-ga`
- [ ] release notes に P1 完了事項を明記

**受入基準**: v0.1.0 GA の全 P1 タスク受入基準 pass

---

## P2 — v1.0.0 商用 (Paid tier + LoRA flywheel、数週〜数ヶ月想定)

### P2-1: Paid tier 実装

現状: tier enum + gate 表示のみ、実 payment layer は `legacy-saas/` に archive

- [ ] Payment provider 選定 (Stripe / Paddle / LemonSqueezy 等) — user 判断案件
- [ ] License key 生成 / 検証 pipeline (現状 `gen-license-key` binary + `text-to-print-core::license` module 部分実装済、要拡張)
- [ ] Paid tier 選択時の payment flow (Web checkout → license key issued → app 内 activate)
- [ ] License 有効性の定期 verify (offline grace period 設定)
- [ ] Free / Paid 切替時の migration (opt-in share の cleanup)

**受入基準**: Paid tier user が payment → license → 完全 offline mode で使える

### P2-2: LoRA re-training flywheel 自動化

現状: `scripts/train_lora.py` あり、Free tier share endpoint (P1-5 で deploy) から集めたデータの自動再学習は未実装

- [ ] CF Workers の share endpoint (R2) から週次 batch で Paperspace A6000/A100 に data pull
- [ ] `scripts/train_lora.py` を CI cron から実行 (fine-tune → checkpoint → HF repo push)
- [ ] 新 LoRA を GGUF に merge → app 側で auto-update (updater.rs 既存)
- [ ] user 側は Model 選択で "Qwen 3.5-4B (community LoRA v1.2)" 等の version 表示

**受入基準**: Free tier で 100 件 3MF 生成 → 翌週の LoRA 版で 質改善 (blind test で判定)

### P2-3: 追加 printer / filament template 拡張

現状: `alice-bamboo::bambu_3mf` 埋込 template は H2D + PETG プロファイル 1 種のみ

- [ ] A1 mini + PLA
- [ ] X1 Carbon + PLA / PETG / ABS
- [ ] P1S + PLA / PETG
- [ ] `crates/app/src/ui/settings.rs` に printer / filament ドロップダウン追加
- [ ] `alice_bamboo::bambu_3mf::export_bambu_3mf` に printer/filament 引数追加 → 対応 template dispatch

**受入基準**: user が printer + filament を選ぶと、その組合せの正しい project_settings.config が埋込まれた .3mf 出力

### P2-4: Windows Authenticode 署名

Windows Defender SmartScreen 回避 (`docs/RELEASE.md` §Windows Authenticode 署名 参照)

- [ ] DigiCert / SSL.com / GlobalSign から EV Code Signing Certificate 取得 (数万〜十数万円/年)
- [ ] `WINDOWS_CERT_B64` + `WINDOWS_CERT_PASSWORD` GitHub Secret 追加
- [ ] `.github/workflows/release.yml` の .msi build 直後に signtool step 追加

**受入基準**: 一般 Windows user が .msi インストール時に SmartScreen 警告出ない

### P2-5: マーケティング + 配布 channel

- [ ] Landing page (extoria.co.jp or 専用 domain)
- [ ] App icon デザイン (`.icns` / `.ico` / `.png` 各 size)
- [ ] Product Hunt / Hacker News 投稿タイミング検討
- [ ] Bambu Lab MakerWorld / Reddit r/BambuLab / r/3Dprinting 露出

### P2-6: v1.0.0 商用 tag

P2-1〜P2-5 完了後:

- [ ] `git tag v1.0.0` + Paid tier open 開始

**受入基準**: 最初の Paid tier 課金成功 + Free tier LoRA flywheel 1 サイクル完走

---

## 判断が要る design 案件

以下は user 判断待ちで、この Roadmap の scope 内で決定してから着手する:

1. **Paid tier の payment provider**: Stripe (実装 mature、手数料 3.6%) vs Paddle (Merchant of Record、税務対応込) vs LemonSqueezy vs 独自
2. **Bonsai27B GGUF 公開判断**: HF public か、Paid tier 専用か、そもそも撤去か
3. **CF Workers 依存の範囲**: share endpoint + crash collector を CF に置く前提で確定するか、AWS / Vercel 等の alternative 検討
4. **LoRA training 主体**: text-to-print 内 or ALICE-LLM 側 or 別 repo で分離
5. **legacy-saas/ の扱い**: Paid tier で復活させるか、完全削除するか

---

## 変更履歴

| date | change |
|--|--|
| 2026-08-07 | 初版作成 (Phase 5.7 完了時点、cargo test 207/207 pass) |
