-- Add role column to profiles
alter table public.profiles add column if not exists role text default 'user'
  check (role in ('user', 'admin'));

-- Add banned flag
alter table public.profiles add column if not exists banned boolean default false;

-- Add hidden flag to projects (for moderation)
alter table public.projects add column if not exists hidden boolean default false;

-- Admin can read all profiles
create policy "Admin can read all profiles" on public.profiles
  for select using (
    exists (select 1 from public.profiles p where p.id = auth.uid() and p.role = 'admin')
  );

-- Admin can update all profiles
create policy "Admin can update all profiles" on public.profiles
  for update using (
    exists (select 1 from public.profiles p where p.id = auth.uid() and p.role = 'admin')
  );

-- Admin can read all projects
create policy "Admin can read all projects" on public.projects
  for select using (
    exists (select 1 from public.profiles p where p.id = auth.uid() and p.role = 'admin')
  );

-- Admin can update all projects (for moderation)
create policy "Admin can update all projects" on public.projects
  for update using (
    exists (select 1 from public.profiles p where p.id = auth.uid() and p.role = 'admin')
  );

-- Hide hidden projects from gallery (update existing policy)
drop policy if exists "Users can view own projects" on public.projects;
create policy "Users can view own projects" on public.projects
  for select using (
    auth.uid() = owner_id
    or (is_public = true and hidden = false)
    or exists (select 1 from public.profiles p where p.id = auth.uid() and p.role = 'admin')
  );
