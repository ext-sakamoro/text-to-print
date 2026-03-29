import type { Metadata } from 'next';
import { Inter } from 'next/font/google';
import '@/styles/globals.css';
const inter = Inter({ subsets: ['latin'] });
export const metadata: Metadata = {
  title: '3dvbgaran',
  description: '3dvbgaran — Text-to-3D SaaS powered by SDF',
  icons: { icon: '/favicon.svg' },
  openGraph: {
    title: '3dvbgaran',
    description: 'Describe what you want to 3D print. AI generates a mathematically precise .3mf file.',
    type: 'website',
    url: 'https://3dvbgaran-production.up.railway.app',
  },
};
export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (<html lang="en" suppressHydrationWarning><body className={inter.className}>{children}</body></html>);
}
