# 3dvbgaran

Text-to-3D SaaS. Describe what you want, get a printable .3mf or .fbx file.

## Architecture

```
           Internet
              |
   Cloudflare Tunnel (free)
              |
       +----- Mac mini -----+
       |                     |
       |  API Gateway :8080  |
       |  (Rust/axum)        |
       |  - JWT / API key    |
       |  - Rate limiting    |
       |  - alice-lol        |
       |    LOL -> SDF       |
       |    -> .3mf / .fbx   |
       |  - DL plan gate     |
       |  - Admin API        |
       |  - Frontend proxy   |
       |                     |
       |  Next.js :3000      |
       |  - Supabase Auth    |
       |  - Stripe Billing   |
       |  - ALICE-View (WASM)|
       |  - three.js fallback|
       |  - Admin panel      |
       |                     |
       |  LLM Server :8000   |
       +---------------------+
              |
       Supabase (external)
       - PostgreSQL + Auth
       - Row Level Security
```

All computation runs on a single Mac mini. Cloudflare Tunnel provides the public URL. No cloud GPU or VPS required.

## Plans

| Plan | Price | Daily Gen | Download | File Sharing | Rate/h |
|------|-------|-----------|----------|-------------|--------|
| Free | 0 | 5 | No | - | 100 |
| General | 1,500/mo | 30 | .3mf .fbx | Public (forced) | 1,000 |
| Pro | 5,000/mo | 100 | .3mf .fbx | Private (toggleable) | 10,000 |
| Enterprise | Contact | Unlimited | .3mf .fbx | Private, no branding | 100,000 |

- **Free**: Browser preview only. No download. Server-side enforced.
- **General**: Downloads enabled. Projects are always public (DB trigger enforced) -> Gallery exposure -> LOL learning data.
- **Pro**: Downloads enabled. Private by default, toggleable.
- **Enterprise**: Commercial license. No ALICE branding. SLA.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| API | Rust / axum + alice-lol + alice-sdf |
| Frontend | Next.js 15 / React 19 |
| 3D Preview | ALICE-View (WebGPU/WASM) + three.js fallback (switchable) |
| DB / Auth | Supabase PostgreSQL + RLS |
| Payments | Stripe (4-tier subscription) |
| Hosting | Mac mini + Cloudflare Tunnel |
| 3D Pipeline | LOL DSL -> SDF -> .3mf / .fbx (Rust, on-demand) |

## Pages

### User-facing

| Path | Auth | Description |
|------|------|-------------|
| `/` | No | Redirect to login |
| `/gallery` | No | Public project gallery |
| `/auth/login` | No | Login / signup |
| `/dashboard` | Yes | Main generation interface |
| `/dashboard/console` | Yes | Per-project editor |
| `/dashboard/projects` | Yes | Project list (public/private badges) |
| `/dashboard/billing` | Yes | 4-tier plan selection |
| `/dashboard/history` | Yes | Generation history |
| `/dashboard/settings` | Yes | Profile, API key |

### Admin (`/admin` — role=admin only)

| Path | Description |
|------|-------------|
| `/admin` | Dashboard (uptime, LLM status, user/generation counts) |
| `/admin/users` | User list, plan change, role change, ban/unban |
| `/admin/generations` | Generation logs (search, pagination, LOL source view) |
| `/admin/gallery` | Public project moderation (hide/unhide) |
| `/admin/revenue` | MRR, subscriber counts by plan |

## Setup

### 1. Build

```bash
# API Gateway
cd services/api-gateway
cargo build --release

# Core Engine (standalone mode)
cd services/core-engine
cargo build --release

# Frontend
cd frontend
npm install && npm run build

# ALICE-View WASM (optional, for WebGPU preview)
cd ~/ALICE-View
wasm-pack build --target web --features wasm -- --no-default-features --features wasm,lol
cp pkg/alice_view_wasm* ~/3dvbgaran/frontend/public/wasm/
```

### 2. Environment Variables

```bash
export SUPABASE_URL="https://xxx.supabase.co"
export SUPABASE_SERVICE_ROLE_KEY="eyJ..."
export JWT_SECRET="your-secret-32chars-minimum"
export LLM_ENDPOINT="http://localhost:8000"
export OUTPUT_DIR="/tmp/3dvbgaran"
export FRONTEND_URL="http://127.0.0.1:3000"
export STRIPE_SECRET_KEY="sk_..."
export STRIPE_WEBHOOK_SECRET="whsec_..."

# Frontend (build-time, baked into Next.js)
export NEXT_PUBLIC_SUPABASE_URL="https://xxx.supabase.co"
export NEXT_PUBLIC_SUPABASE_ANON_KEY="eyJ..."
export NEXT_PUBLIC_WORKER_URL="https://your-domain.com"
export NEXT_PUBLIC_STRIPE_PUBLISHABLE_KEY="pk_..."
```

### 3. Database Migrations

Run in Supabase SQL Editor, in order:

```sql
-- 001-006: Initial schema (already applied)

-- 007: Add General plan + download gating + public defaults
alter table public.profiles drop constraint if exists profiles_plan_check;
alter table public.profiles add constraint profiles_plan_check
  check (plan in ('Free', 'General', 'Pro', 'Enterprise'));

insert into public.plan_configs (plan_name, max_projects, max_api_calls_per_hour)
  values ('General', 20, 1000)
  on conflict (plan_name) do nothing;

update public.plan_configs set max_projects = 5, max_api_calls_per_hour = 100
  where plan_name = 'Free';
update public.plan_configs set max_projects = 100, max_api_calls_per_hour = 10000
  where plan_name = 'Pro';

alter table public.plan_configs add column if not exists can_download boolean default false;
update public.plan_configs set can_download = false where plan_name = 'Free';
update public.plan_configs set can_download = true
  where plan_name in ('General', 'Pro', 'Enterprise');

alter table public.plan_configs add column if not exists default_public boolean default false;
update public.plan_configs set default_public = false
  where plan_name in ('Free', 'Pro', 'Enterprise');
update public.plan_configs set default_public = true where plan_name = 'General';

-- 008: General plan public enforcement (DB trigger)
create or replace function public.enforce_general_public()
  returns trigger language plpgsql security definer set search_path = '' as $$
begin
  if exists (
    select 1 from public.profiles
    where id = new.owner_id and plan = 'General'
  ) then
    new.is_public := true;
  end if;
  return new;
end;
$$;

drop trigger if exists enforce_general_public_trigger on public.projects;
create trigger enforce_general_public_trigger
  before insert or update on public.projects
  for each row execute function public.enforce_general_public();

-- 009: Admin role + moderation
alter table public.profiles add column if not exists role text default 'user'
  check (role in ('user', 'admin'));
alter table public.profiles add column if not exists banned boolean default false;
alter table public.projects add column if not exists hidden boolean default false;

-- Admin RLS policies
create policy "Admin can read all profiles" on public.profiles
  for select using (
    exists (select 1 from public.profiles p where p.id = auth.uid() and p.role = 'admin')
  );
create policy "Admin can update all profiles" on public.profiles
  for update using (
    exists (select 1 from public.profiles p where p.id = auth.uid() and p.role = 'admin')
  );
create policy "Admin can read all projects" on public.projects
  for select using (
    exists (select 1 from public.profiles p where p.id = auth.uid() and p.role = 'admin')
  );
create policy "Admin can update all projects" on public.projects
  for update using (
    exists (select 1 from public.profiles p where p.id = auth.uid() and p.role = 'admin')
  );

-- Update gallery visibility (hide hidden projects)
drop policy if exists "Users can view own projects" on public.projects;
create policy "Users can view own projects" on public.projects
  for select using (
    auth.uid() = owner_id
    or (is_public = true and hidden = false)
    or exists (select 1 from public.profiles p where p.id = auth.uid() and p.role = 'admin')
  );

-- Set initial admin
update public.profiles set role = 'admin' where email = 'sakamoro@alicelaw.net';
```

### 4. Run

```bash
# Terminal 1: Frontend
cd frontend && PORT=3000 npm start

# Terminal 2: API Gateway
cd services/api-gateway && cargo run --release

# Terminal 3: LLM server (your setup)

# Terminal 4: Cloudflare Tunnel
cloudflared tunnel run 3dvbgaran
```

## Directory Structure

```
3dvbgaran/
  services/
    api-gateway/          Rust/axum unified server
                          - auth, rate limit, admin API
                          - alice-lol pipeline (.3mf + .fbx)
                          - DL plan gate, frontend proxy
    core-engine/          Rust/axum inference worker (standalone)
  frontend/
    app/
      dashboard/          User app
        components/       ModelPreview, AliceViewPreview, PreviewSwitcher
      admin/              Admin panel (dashboard, users, generations, gallery, revenue)
      gallery/            Public project viewer
      auth/               Login / signup
      api/stripe/         Checkout + webhook
    lib/
      hooks/              use-generation, use-usage, use-plan, use-projects, use-admin
      stripe/             Stripe server helpers
      supabase/           Supabase client/server helpers
  database/migrations/    SQL migrations (001-009)
  docker/                 Dockerfile + entrypoint (Railway legacy)
```

## Remaining Tasks

### Required for launch

| # | Task | Type | Details |
|---|------|------|---------|
| 1 | **Stripe Product/Price** | Manual | Create `price_general` (1,500/mo) and `price_pro` (5,000/mo) in Stripe Dashboard, update billing page |
| 2 | **DB migrations** | Manual | Run 007-009 SQL in Supabase SQL Editor (see above) |
| 3 | **Cloudflare Tunnel** | Infra | `cloudflared tunnel create 3dvbgaran` + DNS record |
| 4 | **Mac mini startup** | Infra | launchd plist for API Gateway + Next.js + LLM auto-start |
| 5 | **LLM server** | Infra | Replace ollama with llama.cpp server (llama-server) for native perf |
| 6 | **Cargo.toml path** | Deploy | Change alice-lol absolute path to Mac mini path |

### Optional improvements

| # | Task | Type | Details |
|---|------|------|---------|
| 7 | **ALICE-View WASM build** | Build | `wasm-pack build` + copy to `frontend/public/wasm/` |
| 8 | **Banned user enforcement** | Backend | Check `banned` flag in auth middleware, reject requests |
| 9 | **Admin generation stats** | Frontend | Daily/weekly chart on admin dashboard |
| 10 | **Stripe webhook plan sync** | Backend | Handle plan downgrades (Pro -> General -> Free) |
| 11 | **Email notifications** | Feature | Welcome email, plan change, generation limit warning |
| 12 | **Landing page** | Frontend | Public marketing page at `/` instead of redirect |
| 13 | **OGP / favicon** | Frontend | Social media preview, browser icon |
| 14 | **E2E tests** | Test | Update smoke tests for new admin/gallery pages |

## Benchmark

### Environment

| Item | Spec |
|------|------|
| Machine | Mac Mini |
| Chip | Apple M2 Pro |
| Memory | 32 GB |
| LLM | ollama (Metal GPU, VRAM 25GB) |

### Model Comparison (LOL DSL generation)

Test prompts: Cube / Phone Stand / Vase

| Model | Size | Cube | Phone Stand | Vase | Avg Latency | Verdict |
|---|---|---|---|---|---|---|
| Qwen2.5 1.5B | 986MB | OK | Bad syntax | Invalid attrs | ~1s | Unusable |
| Qwen2.5 3B | 1.9GB | OK | Understands structure | Bad syntax | ~1.2s | Minimum |
| **Qwen2.5 7B** | **4.7GB** | **OK** | **Understands structure** | **subtract thin wall** | **~2.6s** | **Recommended** |
| Qwen3.5 9B | 6.6GB | OK | Thinking overflow | Thinking overflow | ~45s | Unsuitable |

**Conclusion**: Qwen2.5 7B is the best balance of accuracy and speed. Qwen3.5 9B wastes time in thinking mode and overflows max_tokens on complex prompts.

### Qwen2.5 7B Load Test (ollama)

Same prompt (short LOL DSL), N concurrent requests.

| N | wall | min | p50 | p95 | max | avg | throughput |
|---|---|---|---|---|---|---|---|
| 1 | 0.84s | 0.81s | 0.81s | 0.81s | 0.81s | 0.81s | 1.2 req/s |
| 5 | 3.39s | 0.73s | 2.04s | 3.36s | 3.36s | 2.04s | 1.5 req/s |
| 10 | 6.82s | 0.84s | 4.13s | 6.78s | 6.78s | 3.81s | 1.5 req/s |
| 20 | 14.48s | 0.73s | 7.79s | 14.42s | 14.42s | 7.47s | 1.4 req/s |
| 50 | 38.11s | 1.03s | 19.74s | 36.53s | 37.99s | 19.40s | 1.3 req/s |

### Qwen2.5 7B Load Test (llama.cpp server, parallel=8)

KV cache 448MB (1.7% of 25GB VRAM).

| N | wall | min | p50 | p95 | max | avg | throughput |
|---|---|---|---|---|---|---|---|
| 1 | 0.78s | 0.74s | 0.74s | 0.74s | 0.74s | 0.74s | 1.3 req/s |
| 5 | 2.32s | 2.14s | 2.22s | 2.28s | 2.28s | 2.22s | 2.2 req/s |
| 8 | 3.19s | 3.03s | 3.14s | 3.15s | 3.15s | 3.09s | **2.5 req/s** |
| 10 | 3.82s | 2.47s | 2.76s | 3.78s | 3.78s | 2.90s | **2.6 req/s** |
| 20 | 8.37s | 2.87s | 6.52s | 8.32s | 8.32s | 5.46s | 2.4 req/s |
| 50 | 21.29s | 2.87s | 13.23s | 20.01s | 21.18s | 11.88s | **2.3 req/s** |

### Comparison

| N | ollama | llama.cpp p=8 | improvement |
|---|---|---|---|
| 1 | 0.84s / 1.2 req/s | 0.78s / 1.3 req/s | +8% |
| 5 | 3.39s / 1.5 req/s | 2.32s / 2.2 req/s | **+47%** |
| 10 | 6.82s / 1.5 req/s | 3.82s / 2.6 req/s | **+73%** |
| 50 | 38.11s / 1.3 req/s | 21.29s / 2.3 req/s | **+77%** |

### Concurrent User Estimate

| Experience | ollama | llama.cpp p=8 |
|---|---|---|
| Comfortable (< 1s) | 1 | **1-2** |
| Good (< 3s) | 3-5 | **8-10** |
| Acceptable (< 5s) | 5-8 | **10-15** |
| Limit (< 10s) | 10-15 | **20-30** |

### Bottleneck: M2 Pro GPU 19-core is the ceiling at ~2.5 req/s. Beyond this requires hardware scaling.

### LLM Server Command

```bash
llama-server \
  --model ~/.3dvbgaran/models/qwen2.5-7b-instruct-q4_k_m.gguf \
  --port 8000 \
  --cont-batching \
  --parallel 8 \
  --ctx-size 8192 \
  --n-gpu-layers 99 \
  --flash-attn on
```

## License

MIT
