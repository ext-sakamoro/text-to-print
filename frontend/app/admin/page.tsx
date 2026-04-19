'use client';
import { useState, useEffect } from 'react';
import { useAdmin } from '@/lib/hooks/use-admin';

interface Stats {
  uptime_secs: number;
  llm_endpoint: string;
  llm_online: boolean;
  total_users: number;
  total_generations: number;
  today_generations: number;
  active_rate_limiters: number;
}

export default function AdminDashboard() {
  const { getStats } = useAdmin();
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    (async () => {
      try {
        setStats(await getStats());
      } catch { /* */ }
      setLoading(false);
    })();
  }, [getStats]);

  const formatUptime = (secs: number) => {
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${d}d ${h}h ${m}m`;
  };

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-2xl font-bold">Dashboard</h1>
      {loading ? (
        <p className="text-sm text-muted-foreground">Loading...</p>
      ) : !stats ? (
        <p className="text-sm text-red-500">Failed to load stats</p>
      ) : (
        <>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <Card label="Uptime" value={formatUptime(stats.uptime_secs)} />
            <Card label="Total Users" value={String(stats.total_users)} />
            <Card label="Total Generations" value={stats.total_generations.toLocaleString()} />
            <Card label="Today" value={String(stats.today_generations)} />
          </div>

          <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
            <Card
              label="LLM Server"
              value={stats.llm_online ? 'Online' : 'Offline'}
              className={stats.llm_online ? 'border-green-500' : 'border-red-500'}
            />
            <Card label="LLM Endpoint" value={stats.llm_endpoint} />
            <Card label="Active Sessions" value={String(stats.active_rate_limiters)} />
          </div>
        </>
      )}
    </div>
  );
}

function Card({ label, value, className }: { label: string; value: string; className?: string }) {
  return (
    <div className={`border rounded-lg p-4 ${className || 'border-border'}`}>
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="text-lg font-bold mt-1 truncate">{value}</p>
    </div>
  );
}
