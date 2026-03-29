import Link from 'next/link';

export default function NotFound() {
  return (
    <div className="min-h-screen flex items-center justify-center px-4">
      <div className="text-center space-y-4">
        <h1 className="text-6xl font-bold text-muted-foreground">404</h1>
        <p className="text-lg text-muted-foreground">Page not found</p>
        <Link
          href="/auth/login"
          className="inline-block px-6 py-3 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:opacity-90"
        >
          Go to Login
        </Link>
      </div>
    </div>
  );
}
