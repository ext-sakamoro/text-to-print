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
    ▼
ALICE-SDF marching cubes → watertight mesh
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
    │
    ▼
.3mf file → open in Bambu Studio → print
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

## Repository layout

```
text-to-print/
├── crates/
│   ├── app/       - Rust desktop GUI (egui + wgpu, main entry)
│   ├── core/      - LOL → mesh export pipeline, license state
│   ├── llm/       - ALICE-LLM sidecar integration + system prompt
│   └── network/   - libp2p P2P share (free-tier upload)
├── datasets/      - LoRA training data (523+ samples, growing)
├── scripts/       - LoRA training / dataset generation
├── assets/        - static resources
├── docs/          - design documents
└── legacy-saas/   - SaaS-era code archived 2026-07-29 (Cloudflare Tunnel +
                    Supabase + Stripe + Next.js frontend + Rust API gateway)
                    kept for historical reference and possible commercial revival
```

## Status

**Standalone pivot in progress** (2026-07-29) Previously deployed as a SaaS on
Mac mini + Cloudflare Tunnel + Supabase + Stripe Now refactoring toward a
single-binary desktop app with embedded ALICE-LLM

Recent changes:
- 2026-07-29: renamed `3dvbgaran` → `text-to-print` SaaS layer moved to
  `legacy-saas/` Standalone-first roadmap
- 2026-04-22: LoRA training pipeline (Paperspace A6000/A100) + 523 sample
  training set
- 2026-04-18: Rust desktop app Phase 1-2 (egui + wgpu + libp2p)

## Build

```bash
# Standalone desktop app
cargo build --release --package text-to-print

# LoRA license utility
cargo build --release --package text-to-print-core --bin gen-license-key
```

Requires:
- Rust 1.75+ (rust-toolchain.toml pinned)
- ALICE ecosystem sibling checkouts at `../ALICE-SDF`, `../ALICE-LOL`,
  `../ALICE-View`, `../ALICE-Physics`, `../ALICE-Bamboo`

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

## Legacy SaaS layer

The pre-2026-07-29 SaaS deployment (Cloudflare Tunnel + Supabase Auth +
Stripe Billing + Next.js frontend + Rust API gateway + inference worker +
Kubernetes/Docker deploy configs) is preserved under [`legacy-saas/`](legacy-saas/)
See [`legacy-saas/README.md`](legacy-saas/README.md) for the pivot rationale
