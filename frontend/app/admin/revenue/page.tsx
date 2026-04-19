'use client';
import { useState, useEffect } from 'react';
import { useAdmin } from '@/lib/hooks/use-admin';

interface Revenue {
  subscribers: {
    general: number;
    pro: number;
    enterprise: number;
  };
  mrr_jpy: number;
  note: string;
}

export default function AdminRevenuePage() {
  const { getRevenue } = useAdmin();
  const [data, setData] = useState<Revenue | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    (async () => {
      try { setData(await getRevenue()); } catch { /* */ }
      setLoading(false);
    })();
  }, [getRevenue]);

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-2xl font-bold">Revenue</h1>

      {loading ? (
        <p className="text-sm text-muted-foreground">Loading...</p>
      ) : !data ? (
        <p className="text-sm text-red-500">Failed to load</p>
      ) : (
        <>
          <div className="border rounded-lg p-6 max-w-md">
            <p className="text-sm text-muted-foreground">Monthly Recurring Revenue</p>
            <p className="text-4xl font-bold mt-2">
              <span className="text-2xl font-normal">&#165;</span>
              {data.mrr_jpy.toLocaleString()}
              <span className="text-sm font-normal text-muted-foreground"> / mo</span>
            </p>
            <p className="text-xs text-muted-foreground mt-2">{data.note}</p>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-4 max-w-2xl">
            <PlanCard plan="General" price={1500} count={data.subscribers.general} />
            <PlanCard plan="Pro" price={5000} count={data.subscribers.pro} />
            <PlanCard plan="Enterprise" price={null} count={data.subscribers.enterprise} />
          </div>

          <div className="border rounded-lg p-4 max-w-md">
            <p className="text-sm font-medium">Breakdown</p>
            <table className="w-full text-sm mt-2">
              <tbody>
                <tr className="border-b">
                  <td className="py-1">General ({data.subscribers.general})</td>
                  <td className="py-1 text-right">&#165;{(data.subscribers.general * 1500).toLocaleString()}</td>
                </tr>
                <tr className="border-b">
                  <td className="py-1">Pro ({data.subscribers.pro})</td>
                  <td className="py-1 text-right">&#165;{(data.subscribers.pro * 5000).toLocaleString()}</td>
                </tr>
                <tr className="border-b">
                  <td className="py-1">Enterprise ({data.subscribers.enterprise})</td>
                  <td className="py-1 text-right text-muted-foreground">Custom</td>
                </tr>
                <tr className="font-bold">
                  <td className="py-1">Total MRR</td>
                  <td className="py-1 text-right">&#165;{data.mrr_jpy.toLocaleString()}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
}

function PlanCard({ plan, price, count }: { plan: string; price: number | null; count: number }) {
  return (
    <div className="border rounded-lg p-4">
      <p className="text-sm text-muted-foreground">{plan}</p>
      <p className="text-2xl font-bold mt-1">{count}</p>
      <p className="text-xs text-muted-foreground mt-1">
        {price ? `@ &#165;${price.toLocaleString()}/mo` : 'Custom pricing'}
      </p>
    </div>
  );
}
