# text-to-print Roadmap

target: **v1.0.0 商用出荷** (Paid tier + LoRA flywheel)

現状 (2026-08-11): core パイプライン完成 (text → LLM → LOL → SDF → mesh → MakerWorld 対応 .3mf、cargo test workspace **232**/232 pass、SKADIS panel canonical 化 + Template アーキテクチャ Phase T1 完了 2026-08-09) release automation 完成 (.tar.gz / .zip / .msi / .deb / .AppImage + macOS codesign + notarize) 残タスクは docs / infra deploy / paid tier

## Release スケジュール (β 公開 vs 本番公開)

| 段階 | version | 意味 | 対象 user | 依存 phase | 目安時期 |
|--|--|--|--|--|--|
| **β 公開** | v0.1.0 β | early adopter 向け先行公開 (GitHub Releases 公開開始、user 側 install 可、ただし production 品質 pre-verify なし、実 MakerWorld / 実 Stripe 未通過) | 個人 / ALICE community / 3D print 早期 user | P0 のみ | 数時間〜1 日 |
| **本番公開 (GA)** | v0.1.0 GA | production 品質 (実機検証済、backend infra 全 deploy 済、公開情報として推奨可) | 一般 3D print user、Bambu owner | P0 + P1 | 数日〜2 週 |
| **商用公開** | v1.0.0 | Paid tier 課金稼働、LoRA flywheel 自動化、複数 printer / filament template | 商用 user + Enterprise | P0 + P1 + P2 | 数週〜数ヶ月 |

**β vs 本番の判断基準**:
- **β 公開時点で許容**: production 品質未達 / 実 MakerWorld upload 未検証 / Stripe live 未切替 / crash reporter / share endpoint 未 deploy / Bonsai27B 公開判断保留 / 実機 test は個人環境のみ
- **本番公開 (GA) 時点で必達**: 全 P1 タスク受入基準 pass + 実 MakerWorld upload 成功 + backend endpoint (share / crash) CF Workers deploy 済 + GA release notes 公開
- **商用公開 (v1.0.0) 時点で必達**: 実 Stripe live 課金 1 件成功 + LoRA flywheel 1 サイクル完走 + Windows Authenticode 署名 + landing page + マーケティング配布 channel 確保

---

### 2026-08-09 完了項目

- ✅ **SKADIS panel canonical 化 (Phase T1.1)** — ALICE-LOL `skadis_panel_sdf` の 3 段 fix: (1) Y 板厚 17mm bug (RoundedBox 6 面 inflate 罠、`~/.claude/projects/-Users-ys/memory/feedback_alice_sdf_rounded_box_six_face_inflate.md`) を `Intersection { RoundedBox, Box3d Y-cutter }` で解消、Y=5mm 正確、X/Z corner fillet 保持 (2) Stadium peg 穴 (Box3d rectangle → 中央 Box + Y 軸 Cylinder × 2 半円 ends の Union、SKADIS_SPEC.md §1 準拠 5×15mm round 2.5) (3) connector 穴 44 個 + mount 穴 6 個追加 (production `models/wall-organizer/skadis-300x300/generate.py::get_conn_positions` / `_mount_positions` を Rust に port、Python 板 origin=左下 → Rust 板 origin=中央 座標変換) 実測 148 hole 全 visible (peg 98 + conn 44 + mount 6)、mesh gen 5292ms/238700 tri/overhang 2.1%/PLA 安全性 OK、Bambu production 3MF `skadis_panel_300x300.3mf` と shape 一致 alice-lol lib test 226 pass (skadis 12 tests all pass)

- ✅ **pipeline aspect_ratio ベース DC/MC 判定 (Phase T1.2)** — `crates/core/src/pipeline.rs` の `use_dc = thickness_y < 5.0` (Y 軸単独) を `should_use_dual_contouring(dims)` helper (`aspect_ratio > 5.0 || min_dim <= 5.0`) に refactor SKADIS panel Y=17mm bug で MC 経路に落ちて Ø5mm peg 穴が MC 解像度不足 (X/Z cell 3.25mm) で消失した bug を根本予防 Bamboo canonical (`~/ALICE-Bamboo/pattern_scores.json` の `"route": "DC"/"MC"`) と実装 route 一致確認 新 test 4 個追加 (should_use_dc_for_flat_panel_shapes / thin_coins / mc_for_bulky / boundary at min_dim 5mm) 詳細: memory `feedback_alice_sdf_dc_mc_route_aspect_ratio.md`

- ✅ **Preview resolution 128→96 (Bamboo canonical 準拠)** — Bamboo canonical (`~/ALICE-Bamboo/examples/compute_pattern_scores.rs` 全 13 pattern 統一値 96) と揃える sample 数 128³=2.1M → 96³=885K = 2.4× 削減、mesh gen 大幅高速化 (SKADIS panel 25 分見込→5.3s = 285× speedup)

- ✅ **TEMPLATE_CATEGORIES ALICE-Bamboo canonical 刷新** — `crates/app/src/ui/prompt.rs` の 16 items (実用品/DIY/ゲーム 15 items が自作 LOL DSL、anti-pattern E: examples を無視して templates を自作) を削除 → 9 items 2 カテゴリ (`~/ALICE-Bamboo/models/` 由来、runtime_parser Phase 5.1 高階 primitive 経由): 「実績品 Both 認証 (Sim 88 + UserFieldTest)」= コイン (100円) / SKADIS panel 300×300 / SKADIS フック S / SKADIS クリップ / SKADIS ゴムバンド 5 items、「実績品 UserFieldTest 認証 (実荷重合格)」= SKADIS フック J/L / SKADIS コンテナ / SKADIS シェルフ 4 items (`alice_lol::stdlib::pattern::registry::ALL` の 13 canonical pattern と 1:1、未対応 4 items = shelf_divider / wall_hook / gridfinity_bin / drawer_organizer は別 session で ALICE-LOL runtime_parser に primitive 追加後に取り込み)

- ✅ **UI 経過時間表示** — `crates/app/src/state.rs::PhaseProgress` に `generation_start: Option<Instant>` + `elapsed()` method 追加 進捗 bar が 0% のまま LLM phase 待機中でも user が経過時間を確認可能 (旧: 「stuck か working か区別つかない」問題)

- ✅ **CI green 化 (両 repo)** — ALICE-LOL fmt fail (`skadis_sdf.rs` L448/L457 trailing comment 位置ずれ、rustfmt が inline コメント直後の // 単独行を続き位置にインデントする挙動) を空行で分離して解消 text-to-print doc fail (rustdoc が `[[wikilink]]` 記法を intra-doc link と解釈して unresolved link error) を「memory `X.md` 参照」text 形式に修正 両 CI success 確認 (ALICE-LOL 1m24s / text-to-print 5m45s)

- ✅ **memory 3 file 追加** (`~/.claude/projects/-Users-ys/memory/`): `feedback_alice_sdf_rounded_box_six_face_inflate.md` (RoundedBox 6 面 inflate 罠) + `feedback_alice_sdf_dc_mc_route_aspect_ratio.md` (MC/DC route 判定 rule) + `success_skadis_panel_canonical_alignment_2026_08_09.md` (production Python 対応 mirror pattern 10 段 canonical フロー、gridfinity/wall_hook/drawer/shelf 4 items で再発予定)

### 2026-08-08 完了項目

- ✅ **3D preview を WGSL raymarching → in-process mesh renderer に置換** (`crates/app/src/sdf/pipeline.rs` + `renderer.rs` + `ui/viewer.rs` 大幅書き換え、~500 行 change) 生成 pipeline が MC/DC で作った `alice_sdf::mesh::Mesh` を `Arc<Mesh>` として `AppState::viewer_mesh` に流し、viewer は wgpu vertex/index buffer に upload して Phong lit で描画 viewer と Bambu Studio が **同一 mesh を表示** するので生成結果の確認が信頼できるようになった Z-up camera + drag orbit / scroll zoom / auto-frame reset 実装済 (以前の Y-up raymarching は camera / world axis 差異で viewer と 3MF が別 shape に見える混乱源だった) 依存 crate `lol_to_wgsl` + `raymarching.wgsl` (505 行) は削除、`MeshStats.preview_mesh: Option<Arc<Mesh>>` field 追加でパイプライン → viewer のデータフロー統一 depth attachment は egui render pass 制約で無し (front/back sort の軽微 artifact あり、offscreen render は将来 improvement)
- ✅ **end-to-end 「入力 → 生成 → ファイル出力」完走** — `system_prompt.md` 全面書き換え (Z-up 慣習明示 + `rotate(90, 0, 0, cylinder(...))` for vertical hole + smartphone stand wedge example) `fix_prompt::LolParseError` variant + directive で parse 失敗時 retry を発動可能に (旧 `SafetyViolationKind::from_message` classifier で "LOL parse error" 未マッチ → 空 suffix → break の gap 修正) `max_retries` 1→2 (3 attempts 許容、worst case ~7 min) HTTP timeout 180→300s (system_prompt 拡張分の inference time 増加を吸収) `export_mesh` の Err を `.ok()` で silent 破棄していた bug 修正 (parse 失敗時に UI に「mesh export failed」を surface) 実測: `スマホスタンド、幅80mm、奥行60mm、高さ40mm、傾斜角65度、ケーブル穴 直径10mm` prompt で Bambu Studio 表示可能な wedge shape の 3MF が確実に出るところまで動作確認 (base + tilted back plate + vertical cable hole)

### 2026-08-07 完了項目 (本日実施済)

- ✅ **legacy-saas/ 完全削除** (2.6 GB uncompressed、tracked file 83 個) 復活時は fresh 実装方針
- ✅ **UI phase state machine test 追加** (`crates/app/src/state.rs::tests`、+7 test: `phase_all_covers_five_phases_in_order` / `phase_label_stable_for_ui` / `phase_progress_default_is_empty` / `phase_progress_marks_completed_phase` / `phase_progress_full_pipeline_flow` / `phase_progress_reset_clears_all_state` / `phase_progress_idempotent_reset`)
- ✅ **CI `check-wasm-worker` job 追加** (`crates/worker` の wasm32 build を CI 側で fail fast 検知)
- ✅ **`crates/worker` の pre-existing wasm32 build エラー 2 件を修正** (上記 CI 追加で顕在化した副次成果)
  - `Cargo.toml`: `worker = "0.5"` → `worker = { version = "0.5", features = ["d1"] }` (D1Database / Env::d1 は feature gate)
  - `share_handler.rs`: `Date::new(&DateInit::Millis(x))` → `Date::new(DateInit::Millis(x))` (worker 0.5 で by-value API)
  - `cargo check --target wasm32-unknown-unknown` green 確認 P1-5 (share endpoint CF Workers deploy) の実 blocker が 1 つ解消
- ✅ **`.unwrap()` audit 完了** — 全 224 unwrap のうち production は license.rs L91 の 1 件のみ (invariant guard 済、`.expect("length guarded by preceding check")` に refactor 済) 他 223 件は全て `#[cfg(test)]` 内、refactor 不要

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

### P1-9: ALICE-Physics `structural_solver` 応力可視化統合 (差別化価値高)

現状 `alice-physics` は 71k LOC あるが `safety_validate` (warp/thermal) のみ使用 (~10%)、`structural_solver` (応力 heatmap) / `cfd_solver` (fluid) / print_pipeline_solver 内部 module (thin_wall / beam_stress / buckling / warp_risk 詳細) は未活用

**Milestone**:
- [ ] `alice-physics::structural_solver` API 調査 (mesh + material → stress field)
- [ ] preview 上に **応力 heatmap overlay** (WGSL fragment で頂点 stress を color mapping)
- [ ] safety_report 拡張: 「この部分は薄すぎて破断リスク」等の位置指定 warning
- [ ] competitor 差別化: Bambu Studio / Cura は pre-print 応力解析持たない、text-to-print 独自 value

**受入基準**: 「傾斜壁 2mm厚 + 底面 hole」等の弱点 shape で応力集中箇所が preview で可視化される

### P1-10: `alice-print` G-code 直接生成 UI 露出

現状 `ExportFormat::Gcode` library で対応済、UI picker で表示なし (3MF / FBX / STL / STEP のみ)

**Milestone**:
- [ ] Settings or Export section に G-code 直接生成 option 追加
- [ ] `alice_print::slice_sdf` 呼びで G-code output (Bambu Lab preset)
- [ ] 上級 user 向け UX: Bambu Studio bypass、SD 直挿し / OctoPrint 系連携可

**受入基準**: 「G-code (Bambu H2D)」export で `.gcode` file が生成される、Bambu H2D で印刷成功

---

## P2 — v1.0.0 商用 (Paid tier + LoRA flywheel、数週〜数ヶ月想定)

### P2-1: Paid tier 実装 (Stripe 統合)

**設計決定** (2026-08-07 確定):
- Payment provider: **Stripe** (test → live 切替 env 変数 1 個)
- Backend: **CF Workers** (`crates/worker` 拡張、libp2p-independent)
- Delivery: **Email** (Resend API 経由、license key 添付)
- Product: **Free ¥0** (LoRA share) / **Pro ¥3,000/mo + ¥30,000/yr** / **Enterprise 問合せ (Stripe 外)**
- License: **Ed25519 署名** (worker で issue、既存 `crates/core/src/license.rs` verify と wire format 互換)
- Grace: subscription_end + **3 day grace** → `TierState::Grace` → Free rollback

#### Phase S1 ✅ (2026-08-07 完了、Backend scaffold)

- ✅ `crates/worker/Cargo.toml` に Stripe / Ed25519 / uuid / chrono / hmac / base64 依存追加
- ✅ `migrations/0002_stripe.sql` (subscribers + licenses + webhook_events 3 table)
- ✅ `crates/worker/src/license_issue.rs` (Ed25519 signer、wire format `text_to_print_core::license::LicenseKey` 互換、8 unit test pass)
- ✅ `crates/worker/src/stripe_webhook.rs` (`POST /stripe/webhook`、HMAC-SHA256 signature verify、`checkout.session.completed` / `customer.subscription.{updated,deleted}` dispatch、idempotency via `webhook_events` table、7 unit test pass)
- ✅ `crates/worker/src/checkout.rs` (`POST /stripe/checkout-session`、Stripe REST API 経由 `mode=subscription` セッション生成、3 unit test pass)
- ✅ `crates/worker/src/email.rs` (Resend API 経由 license email 送信、RESEND_API_KEY 未設定時は log-only fallback、2 unit test pass)
- ✅ `wrangler.toml` に secrets 定義 (STRIPE_SECRET_KEY / STRIPE_WEBHOOK_SECRET / LICENSE_SIGNING_KEY_HEX / PRICE_ID_PRO_{MONTHLY,YEARLY} / RESEND_API_KEY)
- ✅ `docs/STRIPE_SETUP.md` 新規 (Stripe CLI / D1 / Ed25519 生成 / secrets 登録 / local dev / e2e 手順)
- ✅ **cargo test --lib on crates/worker: 31 pass / 0 fail** (既存 15 + Stripe 追加 20)
- ✅ **cargo check --target wasm32-unknown-unknown: green** (CI `check-wasm-worker` job で自動検証)
- ✅ **cargo clippy --target wasm32-unknown-unknown -- -D warnings: 0 warnings**

#### Phase S2 ✅ (2026-08-07 完了、App UI + verify wire)

- ✅ **worker checkout API refactor**: `price_id` → `plan` (`pro_monthly`/`pro_yearly`)、client 側で Stripe price ID 知る必要なし backend 側 `resolve_plan()` で env vars から lookup
- ✅ **既存 license infra 発見・再利用**: `crates/app/src/ui/settings.rs` に `LICENSE_PUBLIC_KEY` 32-byte 実 key + `apply_license` UI + `SettingsState` 既に実装済 `crates/core/src/db.rs` の profiles table に `license_key TEXT` column + `update_profile_tier(id, tier, license_key)` method も既に存在 Phase S2 で新規追加不要 (既存資産の追加拡張のみ)
- ✅ **`crates/app/src/ui/settings.rs` UI 大幅拡張** (+299 行、8 test 追加):
  - License / Subscription collapsible 再構成 (Tier badge with color / Free 時のみ Upgrade section 表示)
  - Email 入力 + "Buy Monthly ¥3,000/月" / "Buy Yearly ¥30,000/年 (-17%)" ボタン → `spawn_checkout` (tokio runtime 上で `reqwest::Client::post` → `open::that` で browser open、UI thread ブロックしない)
  - "Enterprise 問合わせ" ボタン (mailto:enterprise@alicelaw.net)
  - "ライセンスをクリア (Free に戻す)" ボタン (Paid 時のみ表示、db.update_profile_tier で Free + 空 license_key に更新)
  - `apply_license` に有効期限表示追加 (payload.expires_at → "有効期限 YYYY-MM-DD")
  - `is_plausible_email` client-side validation (worker と同 rule)
- ✅ **wire format contract test**: `CheckoutRequestBody` (client) と `worker::checkout::CheckoutRequest` の JSON 形式一致を app 側 unit test で verify
- ✅ **docs/STRIPE_SETUP.md 更新**: Step 7-1 curl 例を `price_id` → `plan` に更新、`TTP_CHECKOUT_ENDPOINT` env var 案内追記
- ✅ **cargo test --workspace: 213 → 221 pass** (+8 settings tests)
- ✅ **cargo test --lib on crates/worker: 31 → 32 pass** (+1 plan naming freeze test)
- ✅ **cargo clippy --workspace -- -D warnings: 0 warnings**
- ✅ **cargo check --target wasm32-unknown-unknown -p text-to-print-worker: green**

#### Phase S3 (次々 session、~2h scope)

- [ ] Live mode 切替 (`STRIPE_SECRET_KEY` / `STRIPE_WEBHOOK_SECRET` / `RESEND_API_KEY` を live 版に差替)
- [ ] Production `wrangler deploy` + custom domain (text-to-print.alicelaw.net) route 設定
- [ ] Landing page (`checkout/success`, `checkout/cancel`) を Cloudflare Pages or extoria.co.jp に配置
- [ ] Stripe Dashboard で live webhook endpoint 登録 → whsec_ を live secret に登録
- [ ] 実 test card ではなく実 credit card で 1 件 subscribe → email 受信 → app verify e2e 確認

**受入基準**: 実 Stripe live 課金で subscribe → Email 受信 → app で verify → Tier::Pro 有効化 → 課金停止で 3 day grace → Free rollback までの loop が完走

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

P2-1〜P2-5 + P2-7 完了後:

- [ ] `git tag v1.0.0` + Paid tier open 開始

**受入基準**: 最初の Paid tier 課金成功 + Free tier LoRA flywheel 1 サイクル完走

### P2-8: Template アーキテクチャ再設計 (LLM bypass + ALICE-Bamboo 直叩き)

**背景**: 現状 templates は日本語 prompt を LLM に注入する scaffolding 実装 template click しても LLM 推論 (2-8 分 + retry) を毎回待たされ、非決定性 (temperature > 0) で毎回違う LOL が生成される 「template = 既知完成品」の user 期待とギャップあり ALICE-LOL / ALICE-Bamboo / ALICE-Print の proven pipeline を直叩きすべきなのに現状は LLM の下流ツール扱いになっている

**正しい architecture** (2026-08-08 議論確定):
- **templates** = LOL DSL library (LLM bypass、alice-bamboo pipeline 直叩き、~1 秒で mesh)
- **自然言語 prompt** = 「novel な形状を text で探索」時のみ LLM 経路 (2-8 分、非決定性)
- 両経路とも同じ alice-bamboo::lol_to_sdf → mesh → export_bambu_3mf に集約

#### Phase T1 ✅ (2026-08-08〜2026-08-09 完了): 静的 LOL テンプレート化 (D-1) + ALICE-Bamboo canonical 化

- ✅ **Phase T1.0** (2026-08-08): 現状 14 template (実用品 / DIY / ゲーム・装飾 の 3 カテゴリ) の Japanese prompt を実 LOL DSL に置換
- ✅ **Phase T1.1** (2026-08-09): TEMPLATE_CATEGORIES を ALICE-Bamboo/models 由来 9 items 2 カテゴリに刷新 (自作 15 items 削除、anti-pattern E 解消、`registry::ALL` 13 canonical pattern と 1:1)
- ✅ **Phase T1.2** (2026-08-09): pipeline `should_use_dual_contouring(dims)` helper で aspect_ratio ベース DC/MC 判定 + Preview resolution 128→96 (Bamboo canonical 準拠)
- ✅ **Phase T1.3** (2026-08-09): ALICE-LOL `skadis_panel_sdf` 3 段 canonical 化 (Y板厚 5mm + Stadium peg + connector/mount 148 hole 全再現)
- ✅ template click → 直接 LOL 生成経路 (`alice_bamboo::lol_to_sdf` → mesh → 3MF) を通す (LLM 完全 bypass)
- ✅ 生成時間 2-8 分 → **数百 ms〜数秒に短縮** (SKADIS panel 大型 template で 5.3s、コイン等の小型で ~1s)、決定性 100%
- ✅ SKADIS panel 300×300 で shape 意図通りに Bambu Studio 表示可能なことを実 UI 目視確認、他 8 items は user 検証待ち

**受入基準**: template click → 数秒以内に mesh preview + 3MF file 生成、同じ template を N 回 click しても常に identical shape (SKADIS panel は 148 hole 全 canonical 再現、production 3MF と一致)

#### Phase T2 (次 session、~2h scope): Placeholder + slider UI (D-3)

- [ ] LOL DSL に `{width}` `{height}` 等 named placeholder を許可 (`stdlib::format!` 相当の変数展開)
- [ ] template metadata に param 定義 (name, min, max, default, unit)
- [ ] template click 時に slider / number input を surface (例: 「コースター 直径 [90] mm、厚さ [4] mm」)
- [ ] slider 変更で LOL bake → 即座に mesh preview 更新 (debounce ~100ms)

**受入基準**: user が template 選択 + slider で寸法調整 → 実時間で preview 更新、3MF export で slider 値通りの mesh 出力

#### Phase T3 (次 session、~1h scope): ALICE-Bamboo/examples の import

- [ ] `~/ALICE-Bamboo/examples/skadis_sdf.rs` / `shopping_cart_coin.rs` / `wall-organizer` 系の hand-crafted LOL を text-to-print の template library に import
- [ ] category を追加 (e.g., "SKADIS system", "実用品 (印刷実績あり)")
- [ ] license note (originator: alice-bamboo/examples) を UI に添付

**受入基準**: SKADIS panel / coin 等の proven LOL を text-to-print から 1 click で出力

#### Phase T4 (次 session、~1h scope): History → template promote

- [ ] History tab で過去生成 LOL を右クリック → 「マイテンプレートに追加」button
- [ ] user 定義 template を db に永続 (profiles.custom_templates JSON column)
- [ ] template category に「マイテンプレート」を追加、hardcoded 14 と並列表示
- [ ] template 名編集 + 削除 UI

**受入基準**: user 生成物 → template 化 → 別 session で再利用可能

### P2-7: datasets 拡大 + LOL primitive 網羅 audit script (別 session 実行想定)

現状 523 sample を数千 sample に拡大 + LOL DSL 全 primitive カバレッジを systematic に verify

- [ ] `scripts/audit_lol_coverage.py` 新規: ALICE-LOL/LLM_REFERENCE.md から primitive 一覧を parse → `datasets/lol_train.jsonl` の各 sample から primitive 使用を抽出 → gap 表出力
- [ ] gap 表を元に合成 sample 生成 (edge case: nested CSG / repeat_finite / thin plate / dual contouring 経路 / hardsurface 24 primitive / SKADIS system 7 accessory)
- [ ] fine-tune 再実行 (Paperspace A6000/A100) + blind eval で品質改善確認

**受入基準**: LOL DSL primitive カバレッジ 100% (全 primitive が最低 3 sample で登場) + sample 総数 2000+ + blind eval で MakerWorld 対応 3MF 生成成功率 90%+

**別 session 実行注意**: 本タスクは Paperspace 課金 + 数時間の training + eval 手作業を含むので、時間 / 予算を確保してから着手する P2-1 (Stripe) と同じく separate execution 想定

---

## 判断が要る design 案件

以下は user 判断待ちで、この Roadmap の scope 内で決定してから着手する:

1. **Paid tier の payment provider**: Stripe (実装 mature、手数料 3.6%) vs Paddle (Merchant of Record、税務対応込) vs LemonSqueezy vs 独自 (P2-1 で実装、判断は別 session で実行想定)
2. **Bonsai27B GGUF 公開判断**: HF public か、Paid tier 専用か、そもそも撤去か
3. **CF Workers 依存の範囲**: share endpoint + crash collector を CF に置く前提で確定するか、AWS / Vercel 等の alternative 検討
4. **LoRA training 主体**: text-to-print 内 or ALICE-LLM 側 or 別 repo で分離
5. ~~**legacy-saas/ の扱い**~~ ✅ 2026-08-07 完全削除 (復活時は fresh 実装)

---

## 変更履歴

| date | change |
|--|--|
| 2026-08-07 | 初版作成 (Phase 5.7 完了時点、cargo test 207/207 pass) |
| 2026-08-07 | legacy-saas/ 削除 + UI phase state machine test 追加 + CI wasm32 job 追加 + license.rs prod unwrap 1 件 refactor 完了 + P2-7 (datasets audit script) 追加 |
| 2026-08-07 | **P2-1 Phase S1 完了** — CF Workers 側 Stripe scaffold (webhook + license issue + checkout + email) 実装、worker crate test 15 → 31 pass、docs/STRIPE_SETUP.md 新規 Phase S2 (app UI + verify wire) / S3 (production deploy) は次 session |
| 2026-08-07 | **P2-1 Phase S2 完了** — worker checkout API を `plan` param に refactor + settings.rs License / Subscription UI 大幅拡張 (Upgrade to Pro / Enterprise mailto / Clear license / expiry 表示) 既存 `LICENSE_PUBLIC_KEY` + `apply_license` + DB `license_key` column を発見して再利用 (新規追加不要) test 244 → 253 pass |
| 2026-08-07 | **CI 修正** — 4 job 全 green 化:  `cargo audit` は `.cargo/audit.toml` per-entry rationale 付き ignore list + Cargo.lock update で 12 vuln 解消 / `fmt` 独立 job 廃止して `clippy-test-doc` に merge (workspace path deps 解決 fail 回避) / `ALICE_ECO_TOKEN` GitHub secret 登録 (private ALICE-Bamboo checkout 通過) / Phase S2 追加 code の rustfmt 自動整形 |
| 2026-08-07 | **end-to-end pipeline 完走まで到達** — LLM system_prompt を Z-up 慣習 + wedge example に刷新、`fix_prompt::LolParseError` variant で parse retry loop 完成、`max_retries` 1→2、HTTP timeout 180→300s、export silent Err bug 修正 実測 `スマホスタンド…` prompt で Bambu Studio 対応 3MF 完走 test 228 pass |
| 2026-08-08 | **3D preview を mesh renderer に置換** — WGSL raymarching (505 行 shader + 全 pipeline) → in-process wgpu mesh renderer + Phong lit + Z-up camera viewer と Bambu が同一 mesh を表示するので生成結果確認が信頼可能 `MeshStats.preview_mesh` field で pipeline → viewer データフロー統一 gallery タブの P2P SDF preview は一時 stub 化 (別 session で mesh 経路に refactor 予定) |
| 2026-08-08 | **P2-8 追加** — Template アーキテクチャ再設計 議論 (現状 Japanese prompt + LLM 経路の非決定性 / 遅さ / 失敗リスクの問題共有) LLM は「novel な形状の探索」だけに使い、templates は alice-bamboo pipeline 直叩き (~1 秒、決定性 100%) にすべきという設計判断確定 Phase T1 (LOL DSL 化、本 session) / T2 (placeholder + slider UI) / T3 (ALICE-Bamboo/examples import) / T4 (history → template promote) の 4 phase に分割 |
| 2026-08-09 | **Phase T1.1-1.3 完了** — SKADIS panel canonical 化 3 段 fix (Y板厚 17mm→5mm bug + Stadium peg + connector/mount 148 hole 全再現) + pipeline aspect_ratio ベース DC/MC 判定 helper + Preview resolution 128→96 (SKADIS panel mesh gen 25 分見込→5.3s = 285x speedup) + TEMPLATE_CATEGORIES を ALICE-Bamboo/models 由来 9 items 2 カテゴリに刷新 + UI 経過時間表示 + memory 3 file (RoundedBox 罠 / DC-MC route / SKADIS canonical) 追加 alice-lol lib test 226 pass / text-to-print workspace 232 pass |
| 2026-08-10 | **CI green 化** — ALICE-LOL fmt fail (trailing comment 位置ずれ) + text-to-print rustdoc fail (wikilink → intra-doc link 誤解釈) 両方修正、両 CI success (ALICE-LOL 1m24s / text-to-print 5m45s) |
| 2026-08-11 | **README / ROADMAP 更新** — Release スケジュール section (β 公開 / 本番公開 GA / 商用公開 v1.0.0 の 3 段区切り + 判断基準明示) 新設 + 2026-08-09 完了項目 6 個追加 + P2-8 Phase T1 チェックボックス更新 (14→9 items canonical) |
| 2026-09-07 | **ALICE-* 活用度精査 + P1 拡張** — text-to-print × ALICE-* crate utilization audit (core value chain 100% 活用済、`alice-view` dead dep 撤去 `cf6c08a`) + P1-9 (`alice-physics::structural_solver` 応力可視化統合、差別化価値高) + P1-10 (`alice-print` G-code 直接生成 UI 露出、上級 user 向け) を ROADMAP に landing worker crate (2.6k LOC wasm32) の ALICE-Auth/Billing/API wrapping は wasm32 制約で意義薄いと判断 (native 実装が最適) |
