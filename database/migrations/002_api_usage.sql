create table if not exists public.api_usage (
    id uuid primary key default gen_random_uuid(),
    user_id uuid not null references public.users(id),
    endpoint text not null,
    request_count int not null default 0,
    month text not null,
    created_at timestamptz not null default now()
);

alter table public.api_usage enable row level security;

create policy "Users can read own usage" on public.api_usage
    for select using (auth.uid() = user_id);

create unique index idx_api_usage_user_month on public.api_usage(user_id, endpoint, month);
