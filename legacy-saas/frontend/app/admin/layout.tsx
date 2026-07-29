'use client';
import { useEffect, useState } from 'react';
import { useRouter, usePathname } from 'next/navigation';
import { createClient } from '@/lib/supabase/client';

const NAV = [
  { href: '/admin', label: 'Dashboard' },
  { href: '/admin/users', label: 'Users' },
  { href: '/admin/generations', label: 'Generations' },
  { href: '/admin/gallery', label: 'Gallery' },
  { href: '/admin/revenue', label: 'Revenue' },
];

export default function AdminLayout({ children }: { children: React.ReactNode }) {
  const [authorized, setAuthorized] = useState<boolean | null>(null);
  const router = useRouter();
  const pathname = usePathname();

  useEffect(() => {
    (async () => {
      const supabase = createClient();
      const { data: { user } } = await supabase.auth.getUser();
      if (!user) { router.push('/auth/login'); return; }

      const { data: profile } = await supabase
        .from('profiles')
        .select('role')
        .eq('id', user.id)
        .single();

      if (profile?.role !== 'admin') {
        router.push('/dashboard');
        return;
      }
      setAuthorized(true);
    })();
  }, [router]);

  if (authorized === null) {
    return <div className="p-6 text-sm text-muted-foreground">Checking access...</div>;
  }

  return (
    <div className="flex h-screen">
      <aside className="w-48 border-r border-border bg-muted/30 p-4 space-y-1">
        <h2 className="text-sm font-bold mb-4">Admin</h2>
        {NAV.map((n) => (
          <a
            key={n.href}
            href={n.href}
            className={`block px-3 py-1.5 text-sm rounded-md ${
              pathname === n.href
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:bg-accent'
            }`}
          >
            {n.label}
          </a>
        ))}
      </aside>
      <main className="flex-1 overflow-auto">{children}</main>
    </div>
  );
}
