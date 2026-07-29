'use client';
import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { useProjects } from '@/lib/hooks/use-projects';
import { usePlan } from '@/lib/hooks/use-plan';

export default function ProjectsPage() {
  const { projects, loading, create, remove, togglePublic } = useProjects();
  const { plan, defaultPublic } = usePlan();
  const [newName, setNewName] = useState('');
  const [creating, setCreating] = useState(false);
  const router = useRouter();

  const canTogglePublic = plan === 'Pro' || plan === 'Enterprise';

  const handleCreate = async () => {
    if (!newName.trim()) return;
    setCreating(true);
    try {
      const p = await create(newName, defaultPublic);
      router.push(`/dashboard/console?project=${p.id}`);
    } catch {
      // error
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    if (!confirm('Delete this project?')) return;
    await remove(id);
  };

  const handleTogglePublic = async (e: React.MouseEvent, id: string, current: boolean) => {
    e.stopPropagation();
    await togglePublic(id, !current);
  };

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-2xl font-bold">Projects</h1>
      <div className="flex gap-2">
        <input
          type="text"
          placeholder="New project name"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
          className="px-3 py-2 border border-input rounded-md bg-background text-sm flex-1 max-w-xs"
        />
        <button
          onClick={handleCreate}
          disabled={creating || !newName.trim()}
          className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:opacity-90 disabled:opacity-50"
        >
          {creating ? 'Creating...' : 'Create'}
        </button>
      </div>
      {loading ? (
        <p className="text-sm text-muted-foreground">Loading...</p>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {projects.map((p) => (
            <button
              key={p.id}
              onClick={() => router.push(`/dashboard/console?project=${p.id}`)}
              className="border border-border rounded-lg p-4 text-left hover:bg-accent transition-colors group relative"
            >
              <div className="flex items-center gap-2">
                <h3 className="font-semibold">{p.name}</h3>
                <span
                  className={`text-xs px-1.5 py-0.5 rounded ${
                    p.is_public
                      ? 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300'
                      : 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400'
                  }`}
                >
                  {p.is_public ? 'Public' : 'Private'}
                </span>
              </div>
              <p className="text-xs text-muted-foreground mt-1">
                Updated {new Date(p.updated_at).toLocaleDateString()}
              </p>
              <div className="absolute top-2 right-2 flex gap-2 opacity-0 group-hover:opacity-100">
                {canTogglePublic && (
                  <span
                    onClick={(e) => handleTogglePublic(e, p.id, p.is_public)}
                    className="text-xs text-muted-foreground hover:text-primary cursor-pointer"
                  >
                    {p.is_public ? 'Make Private' : 'Make Public'}
                  </span>
                )}
                <span
                  onClick={(e) => handleDelete(e, p.id)}
                  className="text-xs text-muted-foreground hover:text-red-500 cursor-pointer"
                >
                  Delete
                </span>
              </div>
            </button>
          ))}
          {projects.length === 0 && (
            <p className="text-muted-foreground text-sm col-span-3">
              No projects yet.
            </p>
          )}
        </div>
      )}
    </div>
  );
}
