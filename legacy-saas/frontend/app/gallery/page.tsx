'use client';
import { useState, useEffect, lazy, Suspense } from 'react';
import { createClient } from '@/lib/supabase/client';

const ModelPreview = lazy(() => import('../dashboard/components/ModelPreview'));

interface PublicProject {
  id: string;
  name: string;
  updated_at: string;
  owner: { full_name: string | null }[] | null;
}

interface PublicGeneration {
  id: string;
  prompt: string;
  lol_source: string;
  triangle_count: number;
  quality: string;
  created_at: string;
}

export default function GalleryPage() {
  const [projects, setProjects] = useState<PublicProject[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [generations, setGenerations] = useState<PublicGeneration[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    (async () => {
      try {
        const supabase = createClient();
        const { data } = await supabase
          .from('projects')
          .select('id, name, updated_at, owner:profiles!owner_id(full_name)')
          .eq('is_public', true)
          .order('updated_at', { ascending: false })
          .limit(50);
        if (data) setProjects(data as PublicProject[]);
      } catch {
        // Supabase not configured
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  useEffect(() => {
    if (!selected) {
      setGenerations([]);
      return;
    }
    (async () => {
      try {
        const supabase = createClient();
        const { data } = await supabase
          .from('generations')
          .select('id, prompt, lol_source, triangle_count, quality, created_at')
          .eq('status', 'completed')
          .order('created_at', { ascending: false })
          .limit(20);
        if (data) setGenerations(data);
      } catch {
        // error
      }
    })();
  }, [selected]);

  return (
    <div className="p-6 max-w-5xl mx-auto space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Gallery</h1>
          <p className="text-sm text-muted-foreground mt-1">
            Public projects shared by the community
          </p>
        </div>
        <a
          href="/auth/login"
          className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:opacity-90"
        >
          Sign in
        </a>
      </div>

      {loading ? (
        <p className="text-sm text-muted-foreground">Loading...</p>
      ) : projects.length === 0 ? (
        <p className="text-sm text-muted-foreground">No public projects yet.</p>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {projects.map((p) => (
            <button
              key={p.id}
              onClick={() => setSelected(selected === p.id ? null : p.id)}
              className={`border rounded-lg p-4 text-left hover:bg-accent transition-colors ${
                selected === p.id ? 'border-primary ring-2 ring-primary/20' : 'border-border'
              }`}
            >
              <h3 className="font-semibold">{p.name}</h3>
              <p className="text-xs text-muted-foreground mt-1">
                {p.owner?.[0]?.full_name || 'Anonymous'} - {new Date(p.updated_at).toLocaleDateString()}
              </p>
            </button>
          ))}
        </div>
      )}

      {selected && generations.length > 0 && (
        <div className="space-y-4">
          <h2 className="text-lg font-semibold">Generations</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {generations.map((g) => (
              <div key={g.id} className="border border-border rounded-lg p-4 space-y-3">
                <p className="text-sm font-medium truncate">{g.prompt}</p>
                <div className="text-xs text-muted-foreground flex gap-3">
                  <span className="capitalize">{g.quality}</span>
                  {g.triangle_count > 0 && (
                    <span>{g.triangle_count.toLocaleString()} triangles</span>
                  )}
                  <span>{new Date(g.created_at).toLocaleString()}</span>
                </div>
                {g.lol_source && (
                  <details>
                    <summary className="text-xs cursor-pointer text-muted-foreground">
                      LOL Source
                    </summary>
                    <pre className="mt-1 text-xs bg-muted p-2 rounded overflow-x-auto max-h-32">
                      {g.lol_source}
                    </pre>
                  </details>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="text-xs text-muted-foreground border-t pt-4">
        Powered by text-to-print Engine
      </div>
    </div>
  );
}
