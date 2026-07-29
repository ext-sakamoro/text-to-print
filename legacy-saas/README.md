# legacy-saas — pre-2026-07-29 SaaS deployment

This directory preserves the SaaS layer that ran text-to-print (旧 3dvbgaran)
as a subscription web service on Mac mini + Cloudflare Tunnel from 2026-04
through 2026-07 It is archived, not maintained

## What's here

| Path | Content |
|--|--|
| `services/api-gateway/` | Rust axum gateway (JWT auth, rate limiting, plan gate, admin API, frontend proxy) |
| `services/core-engine/` | Rust axum inference worker (Ollama LLM → LOL DSL → 3MF/FBX export) |
| `frontend/` | Next.js 15 web dashboard (Supabase Auth, Stripe Billing, ALICE-View WASM preview) |
| `database/` | Schema + migrations |
| `migrations/` | (empty when archived) |
| `supabase/` | Supabase config, RLS policies |
| `docker/` | Dockerfiles for api-gateway, core-engine, frontend |
| `k8s/` | (empty when archived) Kubernetes deployment manifests |
| `railway.toml` | Railway PaaS deploy config |
| `SAAS_LAUNCH_PLAN.md` | SaaS launch strategy document |

## Why archived

2026-07-29 pivot to **standalone desktop with embedded ALICE-LLM**

- ALICE-LLM inference engine matured (v1.2.1+) — can run Qwen 3.5-4B Q4_K_M or
  Bonsai 27B Q1_0 locally on modest hardware including 8 GB Jetson Orin
- SaaS operating cost (Cloudflare Tunnel free but Supabase / Stripe / monitoring
  had setup burden) vs standalone (zero infra) tradeoff no longer favors SaaS
- Freemium model shifts: instead of subscription billing, free users
  opt-in-contribute their generated LOL DSL to grow the LoRA training set,
  paid users get privacy
- Cross-device sync / multi-user gallery / web access are dropped as
  requirements P2P share (via libp2p, `crates/network/`) remains for optional
  free-tier upload

## Can this be revived?

Yes The SaaS layer wires cleanly onto the standalone core Should demand for
a hosted version return, `legacy-saas/services/core-engine/` already invokes
the same `alice-lol::print_export::lol_to_3mf` pipeline the standalone app
uses, so a hosted SaaS could resume with only minor rewiring

## Depending on this code

Don't Anything under `legacy-saas/` may lag behind the main workspace and
should not be used from the standalone build path
