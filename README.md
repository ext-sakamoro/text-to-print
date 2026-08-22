# text-to-print

Standalone desktop app that turns natural language into printable Bambu Lab 3MF
files with an embedded small LLM

Describe what you want to print, hit generate, get a slicer-ready file No
cloud, no login for the local pipeline

```
Text prompt
    │
    ▼
Embedded ALICE-LLM (Qwen 3.5-4B Q4_K_M / Bonsai 27B Q1_0)
    │  system prompt = LOL DSL Text-to-CAD instructions
    │  LoRA fine-tune (523+ samples, growing via free-tier contributions)
    ▼
LOL DSL source
    │
    ▼
ALICE-LOL parser → SdfNode tree
    │
    ├── 厚物: ALICE-SDF marching cubes → watertight mesh (standard)
    │
    └── 薄物 (< 5mm): ALICE-SDF dual_contouring → watertight mesh
                      (Hermite data で feature 保存、極薄 1.7mm でも non_manifold_edges=0
                       実測、MC の resolution 512 で 24,808 non-manifold 破綻を完全回避)
    │
    ▼
ALICE-Physics print-safety pipeline
  - thin-wall check
  - overhang report + heatmap
  - support volume mm³ 見積り
  - warp risk
  - beam stress + buckling (荷重指定時)
    │
    ▼
ALICE-Bamboo 3MF export (+ optional 4-color AMS split, overhang layer)
  or STEP export via alice-sdf (CAD import into Fusion 360 / FreeCAD)
  or direct G-code via alice-print (Bambu Studio 不要 mode)
    │
    ▼
.3mf file → open in Bambu Studio → print
.step file → import into Fusion 360 / FreeCAD for CAD tweak
.gcode file → send straight to printer (Bambu / Marlin / Klipper)
```

All computation runs on a single machine No cloud GPU required LLM inference
uses [ALICE-LLM](https://github.com/ext-sakamoro/ALICE-LLM) with hybrid
CPU/GPU DeltaNet+Attention and wgpu compute shaders

## Screenshots

<!--
  Screenshots are stored under `docs/images/` and referenced from this
  section For submission guidelines see below
-->

| | |
|--|--|
| ![Generate tab — 20mm sphere prompt](docs/images/screenshot-generate.png) | ![Settings tab — LoRA share opt-out toggle + dry-run queue count](docs/images/screenshot-settings.png) |
| Generate tab (prompt → LOL → 3MF preview) | Settings tab (LoRA share opt-out + queue status) |

![Demo — text prompt to 3MF in ~10 seconds](docs/images/demo.gif)

### Screenshot submission

Screenshots and the demo GIF are placeholders until submitted from a live
build Use the following capture settings for consistency:

- Window size **1600 × 1200** (retina 2x is fine — image gets downscaled)
- macOS system appearance: **Dark** (matches the app default)
- Include the top tab bar and status bar
- PNG for stills, GIF for the demo (max 5 MB, ~15 fps, ~10 s loop)

Submission workflow:

1. Capture PNG stills of the Generate / Settings tabs at the sizes above
2. Record a 10-second demo GIF of `sphere(20)` prompt → generation → 3MF
   export (use e.g. `xcap` on Linux, macOS built-in screen recording +
   `ffmpeg` for GIF encoding, or `LICEcap` cross-platform)
3. Save into `docs/images/` with the exact names referenced above
4. Open a PR: `feat(docs): README screenshots + demo GIF`

## Pricing

個人向け tool として最小構成 全て Stripe subscription で管理 (Phase S1/S2 実装済、
S3 で Live 切替 → 課金開始)

| Tier | Price | Sharing behavior |
|--|--|--|
| **Free** | ¥0 | 生成した LOL DSL + 3MF を opt-in で ALICE-LOL プロジェクトに共有 (LoRA 学習データに寄与、全 user が恩恵) |
| **Pro Monthly** | ¥3,000/月 | 完全 offline (LoRA 共有 OFF 強制)、無制限生成 |
| **Pro Yearly** | ¥30,000/年 (-17%) | 同 Monthly、年払い割引 |
| **Enterprise** | 要問合わせ | 複数 user / 商用 / カスタム機能 対応 mailto:enterprise@alicelaw.net |

Free contribution が LoRA training set を育て、model の生成品質が全 user に還元される
flywheel Paid tier は privacy 保証 (完全 offline、生成物は local のみ)

License は Ed25519 署名で client 側 offline verify、subscription 解約 3 日後の grace
period 経過で Free tier に自動 rollback Backend は Cloudflare Workers 無料枠
(`crates/worker`) + Resend 経由 email 配信 全 Rust

## Tech stack

| Layer | Technology |
|--|--|
| GUI | Rust `eframe` + `egui` + `wgpu` (native desktop) |
| LLM inference | ALICE-LLM embedded (wgpu compute shaders + GGUF K-quant + LOL_GBNF grammar constrained decoding) |
| DSL parse / SDF / mesh | alice-lol / alice-sdf / alice-physics / alice-bamboo (path deps) |
| 3D preview | in-process wgpu **mesh** viewer (same `alice_sdf::mesh::Mesh` the exporter writes to 3MF — viewer / Bambu 見た目は一致) |
| Optional P2P share | libp2p (mdns / gossipsub / kad) for free-tier upload to ALICE-LOL |
| Local DB | rusqlite (project history / license state / tier / model choice) |
| Payment | Stripe subscription (Test mode scaffold complete、Phase S3 で Live 切替) |
| Backend | Cloudflare Workers wasm32 (`crates/worker`、share endpoint + Stripe webhook + license issuance + preset library `/api/presets` KV-backed) |
| License | Ed25519 (`ed25519-dalek`、`text_to_print_core::license` client-side offline verify) |
| Email | Resend API (license delivery、backend 未設定時は log-only fallback) |
| Auto-update | GitHub Releases + `self-update` (crates/app/src/updater.rs) |

## Export formats

| Format | Path | Producer | Use |
|--|--|--|--|
| **3MF (MakerWorld 対応)** | `alice_bamboo::bambu_3mf::export_bambu_3mf` (template embed、12-file zip、Phase 5.7) | Bambu Lab AMS / MakerWorld 直接 upload | Standard FDM print |
| **STL** | `alice_bamboo::print_export::lol_to_stl` (Stage 4 集約後) | Any slicer | Legacy pipelines |
| **FBX** | `alice_bamboo::print_export::lol_to_fbx` (Stage 4 集約後) | 3D animation / game engines | Non-print exchange |
| **STEP** | `alice_sdf::io::step::export_step` | Fusion 360 / FreeCAD / SolidWorks | CAD editing round-trip |
| **G-code** | `alice_print::slice_sdf` (Bambu preset, Marlin flavor) | Direct-to-printer | Skip Bambu Studio |
| **3MF (4-color)** | `alice_bamboo::color4::quantize_to_4color` | Bambu Lab AMS 4-filament | Multi-color print |

## Repository layout

```
text-to-print/
├── crates/
│   ├── app/       - Rust desktop GUI (egui + wgpu、main entry、Settings に
│   │                Upgrade / License 入力 UI 実装済)
│   ├── core/      - LOL → mesh export pipeline、Ed25519 license
│   │                verify、tier 管理、rusqlite persist
│   ├── llm/       - Sidecar + Embedded backend (Qwen3.5-4B / Gemma2-27B /
│   │                Bonsai27B) + LOL_GBNF grammar constrained decoding
│   ├── network/   - libp2p P2P share (free-tier upload の client 側)
│   └── worker/    - Cloudflare Workers wasm32 backend (share endpoint +
│                    Stripe webhook + license issuance + Resend email +
│                    preset library `/api/presets` (KV-backed、Sprint X.1)、
│                    workspace 除外、`wrangler deploy` で運用)
├── datasets/      - LoRA training data (523+ samples, growing)
├── scripts/       - LoRA training / dataset generation
├── assets/        - static resources (NotoSansJP.ttf 等)
├── docs/          - design / release / share / STRIPE_SETUP / images
├── ROADMAP.md     - v0.1.0 β / GA / v1.0.0 商用 の milestone breakdown
└── .cargo/        - audit.toml (cargo audit ignore list、per-entry rationale 付き)
```

## Status

**Standalone desktop app** (v0.1.0 β 準備中) core パイプライン (text prompt →
embedded ALICE-LLM → LOL DSL → SDF → MakerWorld 対応 12-file zip 3MF) 完成、
Stripe subscription 統合 backend + app UI 完成 (Test mode)

- `cargo test --workspace`: **232 pass / 0 fail / 2 ignored** (2026-08-09 pipeline aspect_ratio 4 tests 追加)
- `cargo test --lib on crates/worker`: **32 pass / 0 fail**
- `cargo clippy --workspace --all-targets -- -D warnings`: **0 own warnings**
- `cargo check --target wasm32-unknown-unknown -p text-to-print-worker`: **green**
- **CI**: ALICE-LOL / text-to-print 両 repo GitHub Actions **success** (2026-08-10 doc/fmt fix 完了)

Milestone breakdown and remaining tasks to v0.1.0 β / v0.1.0 GA / v1.0.0
commercial release are in [`ROADMAP.md`](ROADMAP.md)

Recent changes:
- 2026-08-22: **Sprint X.1 Cloudflare preset library 完了** — worker `/api/presets` KV-backed preset library deploy 完了 (Custom Domain `text-to-print.alicelaw.net`)、app 起動時に background で fetch → ETag/304 cache 経路で 10 preset 同期、UI に "presets: Bundled / Cache / Cloud" 3 tier ラベル表示、DB `presets_cache` (SQLite single row) + `PresetsSnapshot` + `spawn_presets_sync` mpsc → UI 反映 β user が新 preset 追加を起動時に auto propagate 受信可能に 副次で worker crate 0.5→0.8 upgrade (wasm-bindgen schema mismatch fix)、log 増強 (begin/304 info/error Debug/elapsed_ms、silent failure 診断不能事案の反省) 副次実測 β verify で macOS + Tailscale MagicDNS が Cloudflare Custom Domain の A record を silent drop する経路罠を発見、Tailscale 撤去で恒久解決
- 2026-08-09: **SKADIS panel canonical 化 + Template アーキテクチャ Phase T1** — ALICE-LOL `skadis_panel_sdf` の 3 段 fix (Y板厚 17mm → 5mm、Stadium peg 穴 5×15、connector 穴 44 + mount 穴 6、合計 148 hole 全再現、Bambu production 3MF `~/ALICE-Bamboo/models/wall-organizer/skadis-300x300/skadis_panel_300x300.3mf` と shape 一致) + text-to-print pipeline `should_use_dual_contouring(dims)` helper で `aspect_ratio > 5.0 || min_dim <= 5.0` ベースの DC/MC route 判定 (旧 `thickness_y < 5.0` の Y 軸単独判定で SKADIS panel が MC 経路に落ちて peg 穴消失した bug の根本予防) + Preview resolution 128→96 (Bamboo canonical 準拠、sample 数 2.1M→885K = 2.4x 削減、SKADIS panel 実測 25 分見込 → 5.3s に短縮、285x 高速化) + TEMPLATE_CATEGORIES を ALICE-Bamboo/models 由来 9 items 2 カテゴリに刷新 (自作 15 items 削除、anti-pattern E 解消、`alice_lol::stdlib::pattern::registry::ALL` の 13 canonical pattern と 1:1) + UI 経過時間表示 (state.rs `elapsed()` method) 追加
- 2026-08-08: **3D preview を mesh renderer に置換** (WGSL raymarching 廃止) 生成 pipeline が MC/DC で作った同一 `Mesh` を wgpu vertex/index buffer に upload して Phong lit で描画 viewer と Bambu Studio が同じ形状を表示するため生成結果の確認が信頼できるようになった (旧 raymarching だと Y-up world / camera artifact で違って見える混乱があった) `crates/app/src/sdf/` は名前は残るが中身は mesh pipeline
- 2026-08-07: **end-to-end 完走まで到達** — LLM system_prompt を Z-up 慣習 + 適切な `rotate(90, 0, 0, cylinder(...))` を教える wedge example に刷新、`fix_prompt::LolParseError` variant + directive で parse 失敗時の retry loop を接続 (以前は空 suffix で silent break)、`max_retries` 1→2 + HTTP timeout 180→300s + export `.ok()` silent 破棄 bug 修正 現行 Qwen 3B (grammar OFF) で「スマホスタンド、幅80mm…」prompt から Bambu Studio 表示可能な wedge shape の 3MF が確実に出るところまで動作確認
- 2026-08-07: **P2-1 Phase S1 + S2 完了** — Stripe subscription 統合 (CF Workers
  backend scaffold + app UI Upgrade section / Enter License Key / Grace period
  表示 / Enterprise mailto 導線) Test mode で完結、Live 切替は Phase S3
- 2026-08-07: CI 修正 (`cargo audit` rationale 付き ignore list `.cargo/audit.toml`、
  `fmt` job を `clippy-test-doc` に merge、`ALICE_ECO_TOKEN` secret 登録手順明記)
- 2026-08-07: `legacy-saas/` 完全削除 (2.6 GB uncompressed、tracked 91 file)、
  復活時は fresh 実装方針
- 2026-08-07: UI phase state machine test 追加、`crates/worker` の wasm32 CI check 追加
- 2026-08-06: Phase 5.7 完了 (`alice_bamboo::bambu_3mf::export_bambu_3mf`、Rust
  から MakerWorld 対応 12-file zip 3MF 直接生成、template embed)
- 2026-08-01: Stage 4 完了 (alice-lol → alice-bamboo 集約) + Stage 5 (Freemium
  share/private opt-in) + Stage 3-C.11 (LOL_GBNF grammar constrained decoding)
- 2026-07-29: renamed `3dvbgaran` → `text-to-print` standalone pivot
- 2026-04-22: LoRA training pipeline (Paperspace A6000/A100) + 523 sample set
- 2026-04-18: Rust desktop app Phase 1-2 (egui + wgpu + libp2p)

## Install (β 期間、無署名)

v0.1.0 β 期間中、macOS 版 (`.tar.gz`) と Windows 版 (`.msi` / `.zip`) は
**無署名** で配布されている OS がインストール時に警告を出す場合の回避手順:

- **macOS**: Finder で `.tar.gz` を展開 → 出た `text-to-print` を右クリック → **開く** → 「開発元を確認できません」ダイアログの 「開く」 ボタン (初回のみ、以降は通常起動)
- **Windows**: SmartScreen が「認識されないアプリ」warning を出したら「詳細情報」→「実行」 (`.msi` 直接 install も可、Authenticode 未署名警告あり)
- **Linux**: `.deb` (Debian/Ubuntu) or `.AppImage` (portable、`chmod +x` してから実行)

Apple Developer Program 加入 + Windows Authenticode cert 導入は Phase S3 (Live 課金化) 以降に実施予定 β 期間は「install できる」を優先、警告 UX は割り切り

## Build

```bash
# Standalone desktop app
cargo build --release --package text-to-print

# Bundled LLM sidecar (`alice-llm-server` binary shipped alongside the app)
scripts/build_sidecar.sh

# LoRA license utility
cargo build --release --package text-to-print-core --bin gen-license-key
```

Requires:
- Rust 1.75+ (rust-toolchain.toml pinned)
- ALICE ecosystem sibling checkouts at `../ALICE-SDF`, `../ALICE-LOL`,
  `../ALICE-View`, `../ALICE-Physics`, `../ALICE-Bamboo`, `../ALICE-LLM`

The release installers (`.msi` follow-up pending, `.deb`, `.AppImage`,
`.tar.gz`, `.zip`) bundle `alice-llm-server` next to the desktop binary
`crates/llm/src/sidecar.rs::resolve_bin_path` looks in the exe directory
first, then falls back to `PATH`, so end users have nothing to install
separately

## Run

```bash
cargo run --release --package text-to-print
```

## LoRA training

523+ sample dataset for LOL DSL generation Fine-tune scripts run on
Paperspace A6000/A100

```bash
scripts/train_lora.py --config configs/lora_qwen3_5_4b.yaml
```

## License

- Code: MIT
- Assets and training data: see individual `LICENSE` files under `datasets/`

## Related repos

- [ALICE-LLM](https://github.com/ext-sakamoro/ALICE-LLM) — pure Rust LLM
  inference engine (embedded here)
- [ALICE-LLM-Studio](https://github.com/ext-sakamoro/ALICE-LLM-Studio) —
  general LLM chat desktop app (Tauri v2), sister project
- [ALICE-LOL](https://github.com/ext-sakamoro/ALICE-LOL) — LOL DSL parser and
  Text-to-CAD system prompt library
- [ALICE-Bamboo](https://github.com/ext-sakamoro/ALICE-Bamboo) — 3D print
  pipeline (LOL → SDF → Physics → 3MF export)
- [ALICE-Physics](https://github.com/ext-sakamoro/ALICE-Physics) —
  deterministic 128-bit fixed-point physics engine (print-safety verification)

## History

- 2026-07-29 standalone pivot: The project was previously a SaaS deployment
  (Cloudflare Tunnel + Supabase Auth + Stripe Billing + Next.js frontend +
  Rust API gateway) The SaaS layer has been retired in favor of the
  single-binary desktop app documented above
- 2026-08-07: `legacy-saas/` directory removed from the repository If a
  future Paid tier requires server-side payment / license issuance, it will
  be built fresh rather than reviving the retired SaaS code
