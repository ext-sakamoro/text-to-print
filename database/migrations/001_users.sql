create table if not exists public.users (
    id uuid primary key default gen_random_uuid(),
    email text unique not null,
    plan text not null default 'free',
    stripe_customer_id text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

alter table public.users enable row level security;

create policy "Users can read own data" on public.users
    for select using (auth.uid() = id);

create policy "Users can update own data" on public.users
    for update using (auth.uid() = id);
