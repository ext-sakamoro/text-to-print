import { NextResponse } from 'next/server';
import type { NextRequest } from 'next/server';
export function middleware(request: NextRequest) {
  // Supabase v2 cookie: sb-<project-ref>-auth-token
  const cookies = request.cookies.getAll();
  const isAuth = cookies.some((c) => c.name.startsWith('sb-') && c.name.endsWith('-auth-token'));
  if (request.nextUrl.pathname.startsWith('/dashboard') && !isAuth) return NextResponse.redirect(new URL('/auth/login', request.url));
  if ((request.nextUrl.pathname === '/auth/login' || request.nextUrl.pathname === '/auth/register') && isAuth) return NextResponse.redirect(new URL('/dashboard', request.url));
  return NextResponse.next();
}
export const config = { matcher: ['/dashboard/:path*', '/auth/login', '/auth/register'] };
