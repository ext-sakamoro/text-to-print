'use client';
import { useUsage } from '@/lib/hooks/use-usage';

const plans = [
  {
    name: 'Free',
    price: '0',
    priceLabel: '無料',
    features: [
      '5 generations / day',
      'Browser preview only',
      'No download',
    ],
    priceId: '',
    highlight: false,
  },
  {
    name: 'General',
    price: '1,500',
    priceLabel: '1,500 / mo',
    features: [
      '30 generations / day',
      'Download .3mf / .fbx',
      'Files shared publicly',
      'LOL DSL direct input',
    ],
    priceId: 'price_general',
    highlight: false,
  },
  {
    name: 'Pro',
    price: '5,000',
    priceLabel: '5,000 / mo',
    features: [
      '100 generations / day',
      'Download .3mf / .fbx',
      'Files are private',
      'LOL DSL direct input',
      'Priority support',
    ],
    priceId: 'price_pro',
    highlight: true,
  },
  {
    name: 'Enterprise',
    price: '',
    priceLabel: 'Contact us',
    features: [
      'Unlimited generations',
      'Self-hosted worker',
      'Custom printer profiles',
      'API access',
      'SLA guarantee',
      'No branding',
    ],
    priceId: '',
    highlight: false,
  },
];

const CAN_DOWNLOAD: Record<string, boolean> = {
  Free: false,
  General: true,
  Pro: true,
  Enterprise: true,
};

export { CAN_DOWNLOAD };

export default function BillingPage() {
  const { usage, loading } = useUsage();

  const handleUpgrade = async (plan: (typeof plans)[0]) => {
    if (plan.name === 'Enterprise') {
      window.open(
        'mailto:sakamoro@alicelaw.net?subject=text-to-print%20Enterprise',
        '_blank',
      );
      return;
    }
    if (!plan.priceId) return;
    try {
      const r = await fetch('/api/stripe/checkout', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ priceId: plan.priceId, plan: plan.name }),
      });
      const { url } = await r.json();
      if (url) window.location.href = url;
    } catch {
      // Stripe not configured
    }
  };

  const limitLabel =
    usage.limit === -1 ? 'Unlimited' : `${usage.limit}`;
  const usagePercent =
    usage.limit === -1
      ? 0
      : Math.min(100, (usage.today / usage.limit) * 100);

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-2xl font-bold">Billing</h1>

      <div className="border rounded-lg p-4 max-w-md">
        <p className="text-sm text-muted-foreground">Today&apos;s usage</p>
        {loading ? (
          <p className="text-sm text-muted-foreground mt-1">Loading...</p>
        ) : (
          <>
            <p className="text-2xl font-bold">
              {usage.today} / {limitLabel}{' '}
              <span className="text-sm font-normal text-muted-foreground">
                generations
              </span>
            </p>
            <div className="mt-2 h-2 bg-muted rounded-full overflow-hidden">
              <div
                className="h-full bg-primary rounded-full transition-all"
                style={{ width: `${usagePercent}%` }}
              />
            </div>
            <p className="text-xs text-muted-foreground mt-2">
              Plan: {usage.plan}
              {!CAN_DOWNLOAD[usage.plan] && (
                <span className="ml-2 text-yellow-600">(Download disabled)</span>
              )}
            </p>
          </>
        )}
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 max-w-5xl">
        {plans.map((p) => (
          <div
            key={p.name}
            className={`border rounded-lg p-6 space-y-4 ${
              usage.plan === p.name
                ? 'border-primary ring-2 ring-primary/20'
                : p.highlight
                  ? 'border-primary/50'
                  : 'border-border'
            }`}
          >
            {p.highlight && (
              <span className="text-xs font-medium bg-primary text-primary-foreground px-2 py-0.5 rounded-full">
                Popular
              </span>
            )}
            <h3 className="text-lg font-semibold">{p.name}</h3>
            <p className="text-2xl font-bold">
              {p.price ? (
                <>
                  <span className="text-base font-normal">&#165;</span>
                  {p.price}
                  <span className="text-sm font-normal text-muted-foreground"> / mo</span>
                </>
              ) : (
                <span className="text-lg">{p.priceLabel}</span>
              )}
            </p>
            <ul className="space-y-2">
              {p.features.map((f) => (
                <li
                  key={f}
                  className="text-sm text-muted-foreground flex items-center gap-2"
                >
                  <span className="text-primary">&#10003;</span>
                  {f}
                </li>
              ))}
            </ul>
            {usage.plan === p.name ? (
              <p className="text-sm text-primary font-medium text-center">
                Current plan
              </p>
            ) : (
              <button
                onClick={() => handleUpgrade(p)}
                className="w-full px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:opacity-90"
              >
                {p.name === 'Enterprise' ? 'Contact Sales' : 'Upgrade'}
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
