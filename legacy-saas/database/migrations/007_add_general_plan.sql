-- Add 'General' plan to profiles check constraint
alter table public.profiles drop constraint if exists profiles_plan_check;
alter table public.profiles add constraint profiles_plan_check
  check (plan in ('Free', 'General', 'Pro', 'Enterprise'));

-- Add General plan config
insert into public.plan_configs (plan_name, max_projects, max_api_calls_per_hour)
  values ('General', 20, 1000)
  on conflict (plan_name) do nothing;

-- Update existing plan configs to match new tier
update public.plan_configs set max_projects = 5, max_api_calls_per_hour = 100 where plan_name = 'Free';
update public.plan_configs set max_projects = 100, max_api_calls_per_hour = 10000 where plan_name = 'Pro';

-- Add can_download column to plan_configs
alter table public.plan_configs add column if not exists can_download boolean default false;
update public.plan_configs set can_download = false where plan_name = 'Free';
update public.plan_configs set can_download = true where plan_name in ('General', 'Pro', 'Enterprise');

-- Add default_public column (General plan projects default to public)
alter table public.plan_configs add column if not exists default_public boolean default false;
update public.plan_configs set default_public = false where plan_name in ('Free', 'Pro', 'Enterprise');
update public.plan_configs set default_public = true where plan_name = 'General';
