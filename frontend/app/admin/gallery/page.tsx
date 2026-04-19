'use client';
import { useState, useEffect } from 'react';
import { useAdmin } from '@/lib/hooks/use-admin';

interface Project {
  id: string;
  name: string;
  owner_id: string;
  is_public: boolean;
  hidden: boolean;
  created_at: string;
  updated_at: string;
}

export default function AdminGalleryPage() {
  const { getProjects, updateProject } = useAdmin();
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<'all' | 'public' | 'hidden'>('public');

  useEffect(() => {
    (async () => {
      try { setProjects(await getProjects()); } catch { /* */ }
      setLoading(false);
    })();
  }, [getProjects]);

  const toggleHidden = async (id: string, hidden: boolean) => {
    await updateProject(id, { hidden });
    setProjects((prev) => prev.map((p) => (p.id === id ? { ...p, hidden } : p)));
  };

  const filtered = projects.filter((p) => {
    if (filter === 'public') return p.is_public && !p.hidden;
    if (filter === 'hidden') return p.hidden;
    return true;
  });

  return (
    <div className="p-6 space-y-4">
      <h1 className="text-2xl font-bold">Gallery Moderation</h1>

      <div className="flex gap-2">
        {(['all', 'public', 'hidden'] as const).map((f) => (
          <button
            key={f}
            onClick={() => setFilter(f)}
            className={`px-3 py-1 text-sm rounded-md capitalize ${
              filter === f ? 'bg-primary text-primary-foreground' : 'bg-muted'
            }`}
          >
            {f} ({projects.filter((p) => {
              if (f === 'public') return p.is_public && !p.hidden;
              if (f === 'hidden') return p.hidden;
              return true;
            }).length})
          </button>
        ))}
      </div>

      {loading ? (
        <p className="text-sm text-muted-foreground">Loading...</p>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filtered.map((p) => (
            <div
              key={p.id}
              className={`border rounded-lg p-4 space-y-2 ${
                p.hidden ? 'border-red-300 bg-red-50 dark:bg-red-950' : 'border-border'
              }`}
            >
              <div className="flex items-center justify-between">
                <h3 className="font-semibold truncate">{p.name}</h3>
                <div className="flex gap-1">
                  {p.is_public && (
                    <span className="text-xs px-1.5 py-0.5 rounded bg-green-100 text-green-700">Public</span>
                  )}
                  {p.hidden && (
                    <span className="text-xs px-1.5 py-0.5 rounded bg-red-100 text-red-700">Hidden</span>
                  )}
                </div>
              </div>
              <p className="text-xs text-muted-foreground font-mono">{p.owner_id.slice(0, 8)}...</p>
              <p className="text-xs text-muted-foreground">
                Updated {new Date(p.updated_at).toLocaleDateString()}
              </p>
              <button
                onClick={() => toggleHidden(p.id, !p.hidden)}
                className={`text-xs px-3 py-1 rounded ${
                  p.hidden
                    ? 'bg-green-100 text-green-700 hover:bg-green-200'
                    : 'bg-red-100 text-red-700 hover:bg-red-200'
                }`}
              >
                {p.hidden ? 'Unhide' : 'Hide from Gallery'}
              </button>
            </div>
          ))}
          {filtered.length === 0 && (
            <p className="text-sm text-muted-foreground col-span-3">No projects.</p>
          )}
        </div>
      )}
    </div>
  );
}
