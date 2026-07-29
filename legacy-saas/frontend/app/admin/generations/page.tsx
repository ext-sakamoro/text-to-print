'use client';
import { useState, useEffect, useCallback } from 'react';
import { useAdmin } from '@/lib/hooks/use-admin';

interface Generation {
  id: string;
  user_id: string;
  prompt: string;
  lol_source: string;
  triangle_count: number;
  quality: string;
  status: string;
  error: string | null;
  created_at: string;
}

export default function AdminGenerationsPage() {
  const { getGenerations } = useAdmin();
  const [gens, setGens] = useState<Generation[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [offset, setOffset] = useState(0);
  const limit = 50;

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setGens(await getGenerations({ limit, offset, search: search || undefined }));
    } catch { /* */ }
    setLoading(false);
  }, [getGenerations, offset, search]);

  useEffect(() => { load(); }, [load]);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    setOffset(0);
    load();
  };

  return (
    <div className="p-6 space-y-4">
      <h1 className="text-2xl font-bold">Generation Logs</h1>

      <form onSubmit={handleSearch} className="flex gap-2">
        <input
          type="text"
          placeholder="Search prompts..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="px-3 py-1.5 border rounded-md bg-background text-sm flex-1 max-w-sm"
        />
        <button type="submit" className="px-3 py-1.5 bg-primary text-primary-foreground rounded-md text-sm">
          Search
        </button>
      </form>

      {loading ? (
        <p className="text-sm text-muted-foreground">Loading...</p>
      ) : (
        <>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b text-left text-muted-foreground">
                  <th className="p-2">Time</th>
                  <th className="p-2">User</th>
                  <th className="p-2">Prompt</th>
                  <th className="p-2">Quality</th>
                  <th className="p-2">Triangles</th>
                  <th className="p-2">Status</th>
                  <th className="p-2">LOL</th>
                </tr>
              </thead>
              <tbody>
                {gens.map((g) => (
                  <tr key={g.id} className={`border-b ${g.status === 'error' ? 'bg-red-50 dark:bg-red-950' : ''}`}>
                    <td className="p-2 text-xs text-muted-foreground whitespace-nowrap">
                      {new Date(g.created_at).toLocaleString()}
                    </td>
                    <td className="p-2 font-mono text-xs truncate max-w-[100px]">{g.user_id.slice(0, 8)}</td>
                    <td className="p-2 truncate max-w-[200px]">{g.prompt}</td>
                    <td className="p-2 capitalize">{g.quality}</td>
                    <td className="p-2">{g.triangle_count > 0 ? g.triangle_count.toLocaleString() : '-'}</td>
                    <td className="p-2">
                      <span className={`text-xs px-1.5 py-0.5 rounded ${
                        g.status === 'completed' ? 'bg-green-100 text-green-700' : 'bg-red-100 text-red-700'
                      }`}>
                        {g.status}
                      </span>
                    </td>
                    <td className="p-2">
                      {g.lol_source && (
                        <details>
                          <summary className="text-xs cursor-pointer text-primary">View</summary>
                          <pre className="mt-1 text-xs bg-muted p-2 rounded max-h-32 overflow-auto whitespace-pre-wrap max-w-xs">
                            {g.lol_source}
                          </pre>
                        </details>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="flex gap-2">
            <button
              onClick={() => setOffset(Math.max(0, offset - limit))}
              disabled={offset === 0}
              className="px-3 py-1 text-sm border rounded disabled:opacity-50"
            >
              Prev
            </button>
            <span className="text-sm text-muted-foreground py-1">
              {offset + 1} - {offset + gens.length}
            </span>
            <button
              onClick={() => setOffset(offset + limit)}
              disabled={gens.length < limit}
              className="px-3 py-1 text-sm border rounded disabled:opacity-50"
            >
              Next
            </button>
          </div>
        </>
      )}
    </div>
  );
}
