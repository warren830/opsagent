import { test, expect, Page, BrowserContext } from '@playwright/test';

const BASE = 'https://dg00c54mwvycp.cloudfront.net';
const CREDS = { username: 'admin', password: 'admin123' };

test.describe('Auth Debug', () => {
  let page: Page;
  let context: BrowserContext;

  test.beforeAll(async ({ browser }) => {
    context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      ignoreHTTPSErrors: true,
    });
    page = await context.newPage();
  });

  test.afterAll(async () => {
    await context.close();
  });

  test('Debug: trace full auth flow', async () => {
    // 1. Capture the login API response
    console.log('=== STEP 1: Login API call ===');

    await page.goto(`${BASE}/login`);
    await page.waitForLoadState('networkidle');

    // Intercept login API response
    const loginResponsePromise = page.waitForResponse((r) => r.url().includes('/api/auth/login'));

    await page.locator('input').first().fill(CREDS.username);
    await page.locator('input[type="password"]').fill(CREDS.password);
    await page.locator('button[type="submit"]').click();

    const loginResponse = await loginResponsePromise;
    console.log(`Login response status: ${loginResponse.status()}`);

    const loginHeaders = loginResponse.headers();
    console.log('Login response headers:');
    for (const [key, value] of Object.entries(loginHeaders)) {
      if (/cookie|auth|token|set-cookie|access|cors/i.test(key)) {
        console.log(`  ${key}: ${value}`);
      }
    }

    const loginBody = await loginResponse.json().catch(() => null);
    if (loginBody) {
      console.log('Login response body keys:', Object.keys(loginBody));
      if (loginBody.token) {
        console.log('Token received (first 20 chars):', loginBody.token.substring(0, 20) + '...');
      }
    }

    // Wait for redirect
    await page.waitForURL((url) => !url.pathname.includes('/login'), { timeout: 15000 });
    console.log(`After login URL: ${page.url()}`);

    // 2. Check cookies
    console.log('\n=== STEP 2: Cookies after login ===');
    const cookies = await context.cookies();
    console.log(`Total cookies: ${cookies.length}`);
    for (const cookie of cookies) {
      console.log(`  Cookie: name=${cookie.name}, domain=${cookie.domain}, path=${cookie.path}, httpOnly=${cookie.httpOnly}, secure=${cookie.secure}, sameSite=${cookie.sameSite}`);
    }

    // 3. Check localStorage
    console.log('\n=== STEP 3: localStorage after login ===');
    const localStorageData = await page.evaluate(() => {
      const data: Record<string, string> = {};
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key) {
          data[key] = localStorage.getItem(key)?.substring(0, 100) || '';
        }
      }
      return data;
    });
    console.log('localStorage keys:', Object.keys(localStorageData));
    for (const [key, value] of Object.entries(localStorageData)) {
      console.log(`  ${key}: ${value}`);
    }

    // 4. Check sessionStorage
    console.log('\n=== STEP 4: sessionStorage after login ===');
    const sessionStorageData = await page.evaluate(() => {
      const data: Record<string, string> = {};
      for (let i = 0; i < sessionStorage.length; i++) {
        const key = sessionStorage.key(i);
        if (key) {
          data[key] = sessionStorage.getItem(key)?.substring(0, 100) || '';
        }
      }
      return data;
    });
    console.log('sessionStorage keys:', Object.keys(sessionStorageData));
    for (const [key, value] of Object.entries(sessionStorageData)) {
      console.log(`  ${key}: ${value}`);
    }

    // 5. Take screenshot of the current (authenticated) page
    await page.waitForTimeout(2000);
    await page.screenshot({ path: 'screenshots/debug-01-after-login.png', fullPage: true });

    // 6. Now try SPA navigation (click a link instead of goto)
    console.log('\n=== STEP 5: SPA navigation (click link) ===');
    const clusterLink = page.locator('a[href="/clusters"]').first();
    if (await clusterLink.isVisible({ timeout: 5000 }).catch(() => false)) {
      await clusterLink.click();
      await page.waitForLoadState('networkidle');
      await page.waitForTimeout(2000);
      console.log(`After SPA nav to clusters: ${page.url()}`);
      await page.screenshot({ path: 'screenshots/debug-02-spa-nav-clusters.png', fullPage: true });
    } else {
      console.log('Cluster link not visible in sidebar');
    }

    // 7. Hard navigation (page.goto) — this is what breaks
    console.log('\n=== STEP 6: Hard navigation (page.goto) ===');
    await page.goto(`${BASE}/clusters`);
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(2000);
    console.log(`After hard nav to clusters: ${page.url()}`);
    await page.screenshot({ path: 'screenshots/debug-03-hard-nav-clusters.png', fullPage: true });

    // Check cookies after hard nav
    const cookiesAfter = await context.cookies();
    console.log(`Cookies after hard nav: ${cookiesAfter.length}`);
    for (const cookie of cookiesAfter) {
      console.log(`  Cookie: name=${cookie.name}, domain=${cookie.domain}, path=${cookie.path}`);
    }

    // Check localStorage after hard nav
    const lsAfter = await page.evaluate(() => {
      const data: Record<string, string> = {};
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key) {
          data[key] = localStorage.getItem(key)?.substring(0, 100) || '';
        }
      }
      return data;
    });
    console.log('localStorage after hard nav:', Object.keys(lsAfter));

    // 8. Try setting token manually and navigating
    console.log('\n=== STEP 7: Manual API call with token ===');
    if (loginBody?.token) {
      const meResponse = await page.evaluate(async (token) => {
        const res = await fetch('/api/auth/me', {
          headers: { 'Authorization': `Bearer ${token}` },
          credentials: 'include',
        });
        return { status: res.status, body: await res.json().catch(() => null) };
      }, loginBody.token);
      console.log(`/api/auth/me response: ${meResponse.status}`);
      console.log(`/api/auth/me body:`, meResponse.body);
    }
  });

  test('Debug: test SPA-only navigation (no page.goto)', async () => {
    // Fresh login
    await page.goto(`${BASE}/login`);
    await page.waitForLoadState('networkidle');
    await page.locator('input').first().fill(CREDS.username);
    await page.locator('input[type="password"]').fill(CREDS.password);
    await page.locator('button[type="submit"]').click();
    await page.waitForURL((url) => !url.pathname.includes('/login'), { timeout: 15000 });
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(3000);

    console.log(`\n=== SPA-ONLY NAVIGATION TEST ===`);
    console.log(`Starting URL: ${page.url()}`);
    await page.screenshot({ path: 'screenshots/debug-10-spa-start.png', fullPage: true });

    // Navigate by clicking sidebar links only (SPA navigation)
    const navTests = [
      { text: /tenant/i, name: 'Tenants' },
      { text: /user/i, name: 'Users' },
      { text: /account|cloud/i, name: 'Accounts' },
      { text: /cluster/i, name: 'Clusters' },
      { text: /deploy/i, name: 'Deployments' },
      { text: /issue/i, name: 'Issues' },
      { text: /skill/i, name: 'Skills' },
      { text: /setting/i, name: 'Settings' },
      { text: /topology|service/i, name: 'Topology' },
      { text: /telemetry/i, name: 'Telemetry' },
      { text: /glossary/i, name: 'Glossary' },
      { text: /knowledge/i, name: 'Knowledge' },
      { text: /provider|model/i, name: 'Providers' },
      { text: /channel/i, name: 'Channels' },
    ];

    for (const nav of navTests) {
      const link = page.locator('a').filter({ hasText: nav.text }).first();
      if (await link.isVisible({ timeout: 2000 }).catch(() => false)) {
        await link.click();
        await page.waitForLoadState('networkidle');
        await page.waitForTimeout(1500);
        const url = page.url();
        const onLogin = url.includes('/login');
        console.log(`  ${onLogin ? '✗' : '✓'} ${nav.name}: ${url}`);

        const screenshotName = `debug-11-spa-${nav.name.toLowerCase()}.png`;
        await page.screenshot({ path: `screenshots/${screenshotName}`, fullPage: true });

        if (onLogin) {
          console.log('  ⚠ Session lost during SPA navigation!');
          break;
        }
      } else {
        console.log(`  - ${nav.name}: link not found`);
      }
    }
  });
});
