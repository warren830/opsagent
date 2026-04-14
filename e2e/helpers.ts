import { Page, Browser, BrowserContext } from '@playwright/test';

export const BASE = 'https://dg00c54mwvycp.cloudfront.net';
export const ADMIN_CREDS = { username: 'admin', password: 'admin123' };
export const RUN_ID = String(Date.now()).slice(-6);

// ============================================================
// API Client — direct HTTP calls to backend API
// ============================================================
export class ApiClient {
  constructor(
    private baseUrl: string = BASE,
    public token: string = '',
  ) {}

  private headers(): Record<string, string> {
    const h: Record<string, string> = { 'Content-Type': 'application/json' };
    if (this.token) h['Authorization'] = `Bearer ${this.token}`;
    return h;
  }

  async login(username: string, password: string): Promise<string> {
    const resp = await fetch(`${this.baseUrl}/api/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
    });
    if (!resp.ok) throw new Error(`Login failed: ${resp.status} ${await resp.text()}`);
    const data = await resp.json();
    this.token = data.token;
    return data.token;
  }

  async get<T = any>(path: string): Promise<{ status: number; data: T }> {
    const resp = await fetch(`${this.baseUrl}${path}`, { headers: this.headers() });
    const data = resp.ok ? await resp.json() : null;
    return { status: resp.status, data };
  }

  async post<T = any>(path: string, body?: unknown): Promise<{ status: number; data: T }> {
    const resp = await fetch(`${this.baseUrl}${path}`, {
      method: 'POST',
      headers: this.headers(),
      body: body ? JSON.stringify(body) : undefined,
    });
    const data = resp.ok ? await resp.json().catch(() => null) : null;
    return { status: resp.status, data };
  }

  async put<T = any>(path: string, body?: unknown): Promise<{ status: number; data: T }> {
    const resp = await fetch(`${this.baseUrl}${path}`, {
      method: 'PUT',
      headers: this.headers(),
      body: body ? JSON.stringify(body) : undefined,
    });
    const data = resp.ok ? await resp.json().catch(() => null) : null;
    return { status: resp.status, data };
  }

  async del(path: string): Promise<{ status: number }> {
    const resp = await fetch(`${this.baseUrl}${path}`, {
      method: 'DELETE',
      headers: this.headers(),
    });
    return { status: resp.status };
  }
}

// ============================================================
// Browser helpers — login via UI, SPA navigation
// ============================================================
export async function loginViaUI(
  browser: Browser,
  username: string,
  password: string,
): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    ignoreHTTPSErrors: true,
  });
  const page = await context.newPage();

  await page.goto(`${BASE}/login`);
  await page.waitForLoadState('networkidle');
  await page.locator('input').first().fill(username);
  await page.locator('input[type="password"]').fill(password);
  await page.locator('button[type="submit"]').click();
  await page.waitForURL((url) => !url.pathname.includes('/login'), { timeout: 15000 });
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(2000);

  return { context, page };
}

export async function spaNav(page: Page, textPattern: RegExp, timeout = 3000): Promise<boolean> {
  const link = page.locator('a').filter({ hasText: textPattern }).first();
  if (await link.isVisible({ timeout }).catch(() => false)) {
    await link.click();
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(1500);
    return true;
  }
  return false;
}

// ============================================================
// Shared test state — populated by Phase 1, used by all phases
// ============================================================
export interface TestEnv {
  // Tokens
  superAdminToken: string;
  alphaAdminToken: string;
  alphaReadonlyToken: string;
  betaAdminToken: string;

  // IDs
  tenantAlphaId: string;
  tenantBetaId: string;
  userAlphaAdminId: string;
  userAlphaReadonlyId: string;
  userBetaAdminId: string;
  accountAlphaId: string;
  accountBetaId: string;

  // Credentials
  alphaAdminCreds: { username: string; password: string };
  alphaReadonlyCreds: { username: string; password: string };
  betaAdminCreds: { username: string; password: string };

  // Created resource IDs for cleanup
  glossaryAlphaId?: string;
  glossaryBetaId?: string;
  clusterAlphaId?: string;
}

export const env: TestEnv = {
  superAdminToken: '',
  alphaAdminToken: '',
  alphaReadonlyToken: '',
  betaAdminToken: '',
  tenantAlphaId: '',
  tenantBetaId: '',
  userAlphaAdminId: '',
  userAlphaReadonlyId: '',
  userBetaAdminId: '',
  accountAlphaId: '',
  accountBetaId: '',
  alphaAdminCreds: { username: `e2e-alpha-admin-${RUN_ID}`, password: 'Test1234!' },
  alphaReadonlyCreds: { username: `e2e-alpha-ro-${RUN_ID}`, password: 'Test1234!' },
  betaAdminCreds: { username: `e2e-beta-admin-${RUN_ID}`, password: 'Test1234!' },
};
