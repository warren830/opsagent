/**
 * Localhost-oriented test utilities for the comprehensive Playwright suite.
 *
 * The Ops backend sets `token` and `refresh_token` as HttpOnly cookies on
 * successful login (see backend/src/handlers/auth.rs). We replicate that by:
 *   1. POSTing to /api/auth/login via fetch to obtain the token and cookies.
 *   2. Injecting the auth cookie into the Playwright BrowserContext so every
 *      new page is already authenticated.
 *   3. Also writing `localStorage.setItem('auth-token', token)` via
 *      addInitScript — belt-and-suspenders, and honours the task spec.
 *
 * Do NOT import from ../helpers.ts — that file points at the CloudFront
 * environment and must not be modified.
 */
import type { Browser, BrowserContext, ConsoleMessage, Page } from '@playwright/test';

// ─── Constants ────────────────────────────────────────────────────────────

export const LOCAL_BASE = 'http://localhost:9999';
export const BACKEND_BASE = 'http://localhost:3080';
export const ADMIN_CREDS = { username: 'admin', password: 'admin123' };

// ─── Low-level: login via backend API ─────────────────────────────────────

export interface LoginResult {
  token: string;
  /** Raw Set-Cookie headers from the login response */
  setCookies: string[];
}

/**
 * Call POST /api/auth/login directly against the backend.
 * Returns the access token and raw Set-Cookie headers so the caller can
 * install them on a BrowserContext.
 */
export async function loginViaBackend(
  username = ADMIN_CREDS.username,
  password = ADMIN_CREDS.password,
): Promise<LoginResult> {
  const resp = await fetch(`${BACKEND_BASE}/api/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password }),
  });
  if (!resp.ok) {
    throw new Error(`Login failed: ${resp.status} ${await resp.text()}`);
  }
  // Node fetch exposes Set-Cookie via headers.getSetCookie() (Node 20+)
  // but falls back to raw 'set-cookie' header for older versions.
  const getSetCookie = (resp.headers as any).getSetCookie?.bind(resp.headers);
  const setCookies: string[] = getSetCookie
    ? getSetCookie()
    : (resp.headers.get('set-cookie') ? [resp.headers.get('set-cookie') as string] : []);
  const body = (await resp.json()) as { token: string };
  return { token: body.token, setCookies };
}

/**
 * Parse a single `Set-Cookie` header line into the shape Playwright expects.
 * Only extracts the fields we need (name, value, path, expires).
 */
function parseSetCookie(header: string, domain: string) {
  const [pair, ...attrs] = header.split(';').map((s) => s.trim());
  const eq = pair.indexOf('=');
  const name = pair.slice(0, eq);
  const value = pair.slice(eq + 1);
  let path = '/';
  let maxAge: number | null = null;
  for (const attr of attrs) {
    const [k, v] = attr.split('=');
    const key = k.toLowerCase();
    if (key === 'path' && v) path = v;
    if (key === 'max-age' && v) maxAge = parseInt(v, 10);
  }
  const cookie: any = { name, value, domain, path, httpOnly: true, secure: false, sameSite: 'Lax' };
  if (maxAge !== null && !isNaN(maxAge)) {
    cookie.expires = Math.floor(Date.now() / 1000) + maxAge;
  }
  return cookie;
}

/**
 * Public spec'd API: `loginViaAPI(context)` → token string, with the context
 * fully authenticated for any page opened afterward.
 */
export async function loginViaAPI(context: BrowserContext): Promise<string> {
  const { token, setCookies } = await loginViaBackend();

  // Install backend-issued cookies on the context so XHR and navigation
  // include them. Rewrite to domain=localhost and secure=false since the
  // local dev server uses plain HTTP.
  const cookies = setCookies.map((line) => parseSetCookie(line, 'localhost'));
  if (cookies.length > 0) {
    await context.addCookies(cookies);
  }

  // Also inject localStorage.auth-token as the spec requests, so that every
  // new page auto-authenticates without a network round-trip. The app itself
  // reads the cookie, not localStorage, but having the token available in
  // window storage is harmless and useful for feature checks.
  await context.addInitScript((t: string) => {
    try {
      window.localStorage.setItem('auth-token', t);
    } catch {
      /* ignore */
    }
  }, token);

  return token;
}

/**
 * Create a fresh BrowserContext that is already authenticated and return
 * `{ context, page }`.
 */
export async function setupAuthed(browser: Browser): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    ignoreHTTPSErrors: true,
    baseURL: LOCAL_BASE,
  });
  await loginViaAPI(context);
  const page = await context.newPage();
  return { context, page };
}

/**
 * Wait for Nuxt SSR + hydration to settle.
 *   - networkidle: all in-flight requests finished
 *   - extra 500 ms buffer: Nuxt client-side hydration + reactive watchers
 *     often fire microtasks after networkidle.
 */
export async function waitForHydration(page: Page): Promise<void> {
  try {
    await page.waitForLoadState('networkidle', { timeout: 15_000 });
  } catch {
    // Some pages keep long-poll / SSE connections open and never idle.
    // Fall back to DOMContentLoaded — hydration has fired by then.
    await page.waitForLoadState('domcontentloaded');
  }
  await page.waitForTimeout(500);
}

/**
 * Attach listeners to capture console errors and uncaught exceptions.
 * The returned object keeps growing until the page is closed.
 *
 * Known benign noise is filtered out so tests don't fail on dev-server
 * warnings (e.g. Vite HMR, hydration hints that are already tracked).
 */
export interface ConsoleCapture {
  errors: string[];
}

const BENIGN_ERROR_PATTERNS: RegExp[] = [
  /Failed to load resource.*favicon/i,
  /\[vite\]/i,
  /DevTools failed to load/i,
  // Nuxt sometimes logs this during hot reload; not a product bug.
  /Hydration completed but contains mismatches/i,
];

export function captureConsole(page: Page): ConsoleCapture {
  const errors: string[] = [];
  page.on('console', (msg: ConsoleMessage) => {
    if (msg.type() !== 'error') return;
    const text = msg.text();
    if (BENIGN_ERROR_PATTERNS.some((re) => re.test(text))) return;
    errors.push(`[console.error] ${text}`);
  });
  page.on('pageerror', (err: Error) => {
    const text = err.message || String(err);
    if (BENIGN_ERROR_PATTERNS.some((re) => re.test(text))) return;
    errors.push(`[pageerror] ${text}`);
  });
  return { errors };
}

// ─── Convenience helpers used across specs ────────────────────────────────

/**
 * Return `true` if the current URL path starts with `/login`.
 * Useful for asserting redirects.
 */
export function isOnLogin(page: Page): boolean {
  const u = new URL(page.url());
  return u.pathname.startsWith('/login');
}

/**
 * Clear all cookies + storage to fully log out a context.
 */
export async function clearAuth(context: BrowserContext): Promise<void> {
  await context.clearCookies();
  try {
    const page = context.pages()[0];
    if (page) {
      await page.evaluate(() => {
        try { window.localStorage.clear(); } catch { /* noop */ }
        try { window.sessionStorage.clear(); } catch { /* noop */ }
      });
    }
  } catch { /* noop */ }
}

/**
 * Slugify a route path for use in screenshot filenames.
 * "/"  →  "root"
 * "/auth/cognito/callback" → "auth-cognito-callback"
 */
export function routeSlug(route: string): string {
  if (route === '/') return 'root';
  return route.replace(/^\//, '').replace(/\//g, '-');
}
