'use client';
import { useCallback } from 'react';
import { createClient } from '@/lib/supabase/client';

async function getToken(): Promise<string> {
  const supabase = createClient();
  const { data } = await supabase.auth.getSession();
  return data.session?.access_token || '';
}

async function adminFetch(path: string, opts?: RequestInit) {
  const token = await getToken();
  const workerUrl = process.env.NEXT_PUBLIC_WORKER_URL || '';
  const resp = await fetch(`${workerUrl}${path}`, {
    ...opts,
    headers: {
      'Authorization': `Bearer ${token}`,
      'Content-Type': 'application/json',
      ...opts?.headers,
    },
  });
  return resp;
}

export function useAdmin() {
  const getStats = useCallback(async () => {
    const r = await adminFetch('/api/v1/admin/stats');
    return r.json();
  }, []);

  const getUsers = useCallback(async () => {
    const r = await adminFetch('/api/v1/admin/users');
    return r.json();
  }, []);

  const updateUser = useCallback(async (id: string, data: Record<string, unknown>) => {
    const r = await adminFetch(`/api/v1/admin/users/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    });
    return r.json();
  }, []);

  const getGenerations = useCallback(async (opts?: { limit?: number; offset?: number; search?: string }) => {
    const params = new URLSearchParams();
    if (opts?.limit) params.set('limit', String(opts.limit));
    if (opts?.offset) params.set('offset', String(opts.offset));
    if (opts?.search) params.set('search', opts.search);
    const r = await adminFetch(`/api/v1/admin/generations?${params}`);
    return r.json();
  }, []);

  const getProjects = useCallback(async () => {
    const r = await adminFetch('/api/v1/admin/projects');
    return r.json();
  }, []);

  const updateProject = useCallback(async (id: string, data: Record<string, unknown>) => {
    const r = await adminFetch(`/api/v1/admin/projects/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    });
    return r.json();
  }, []);

  const getRevenue = useCallback(async () => {
    const r = await adminFetch('/api/v1/admin/revenue');
    return r.json();
  }, []);

  return { getStats, getUsers, updateUser, getGenerations, getProjects, updateProject, getRevenue };
}
