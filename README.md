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

## Freemium tier

| Tier | Price | Sharing behavior |
|--|--|--|
| **Free** | 0 | Generated LOL DSL + 3MF files opt-in shared with ALICE-LOL project (fuels LoRA fine-tune improvements for everyone) |
| **Paid** | (planned) | Generated LOL DSL + 3MF files stay local, nothing uploaded |

Free tier contributions grow the LoRA training set so the model gets better at
generating LOL DSL over time Paid tier is fully offline

## Tech stack

| Layer | Technology |
|--|--|
| GUI | Rust `eframe` + `egui` + `wgpu` (native desktop) |
| LLM inference | ALICE-LLM embedded (wgpu compute shaders + GGUF K-quant) |
| DSL parse / SDF / mesh | alice-lol / alice-sdf / alice-physics / alice-bamboo (path deps) |
| 3D preview | alice-view (WebGPU/WASM) with fallback |
| Optional P2P share | libp2p (mdns / gossipsub / kad) for free-tier upload to ALICE-LOL |
| Local DB | rusqlite (project history / license state) |

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
│   ├── app/       - Rust desktop GUI (egui + wgpu, main entry)
│   ├── core/      - LOL → mesh export pipeline, license state
│   ├── llm/       - ALICE-LLM sidecar + embedded backend integration
│   ├── network/   - libp2p P2P share (free-tier upload)
│   └── worker/    - Cloudflare Workers wasm32 backend (share endpoint,
│                    workspace 除外、`wrangler deploy` で運用)
├── datasets/      - LoRA training data (523+ samples, growing)
├── scripts/       - LoRA training / dataset generation
├── assets/        - static resources (NotoSansJP.ttf 等)
└── docs/          - design / release / share docs + ROADMAP.md
```

## Status

**Standalone desktop app** (v0.1.0 β 準備中) core パイプライン (text prompt →
embedded ALICE-LLM → LOL DSL → SDF → MakerWorld 対応 12-file zip 3MF) 完成、
cargo test workspace 207/207 pass

Milestone breakdown and remaining tasks to v0.1.0 β / v0.1.0 GA / v1.0.0
commercial release are in [`ROADMAP.md`](ROADMAP.md)

Recent changes:
- 2026-08-07: `legacy-saas/` 削除 (SaaS-era code retired)、UI phase state
  machine tests 追加、`crates/worker` の wasm32 CI check 追加
- 2026-08-06: Phase 5.7 完了 (`alice_bamboo::bambu_3mf::export_bambu_3mf`、
  Rust から MakerWorld 対応 3MF 直接生成)
- 2026-08-01: Stage 4 完了 (alice-lol → alice-bamboo 集約) + Stage 5
  (Freemium tier share/private opt-in) + Stage 3-C.11 (LOL_GBNF grammar
  constrained decoding)
- 2026-07-29: renamed `3dvbgaran` → `text-to-print` standalone pivot
- 2026-04-22: LoRA training pipeline (Paperspace A6000/A100) + 523 sample set
- 2026-04-18: Rust desktop app Phase 1-2 (egui + wgpu + libp2p)

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
