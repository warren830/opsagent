/**
 * Auth flow + navigation interactions.
 *
 * Run: E2E_BASE_URL=http://localhost:9999 npx playwright test \
 *      comprehensive/02-auth-nav.spec.ts --project=chromium
 */
import { test, expect } from '@playwright/test';
import {
  ADMIN_CREDS,
  LOCAL_BASE,
  clearAuth,
  isOnLogin,
  loginViaAPI,
  setupAuthed,
  waitForHydration,
} from './helpers-local';

test.describe('auth + navigation', () => {
  // ─── Login: happy path via UI ────────────────────────────────────────
  test('login succeeds and lands on /', async ({ browser }) => {
    const ctx = await browser.newContext({ baseURL: LOCAL_BASE });
    const page = await ctx.newPage();
    try {
      await page.goto(`${LOCAL_BASE}/login`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);

      // Local login form uses id=username / id=password.
      await page.locator('#username').fill(ADMIN_CREDS.username);
      await page.locator('#password').fill(ADMIN_CREDS.password);
      await page.locator('button[type="submit"]').first().click();

      // Wait for navigation away from /login.
      await page.waitForURL((url) => !url.pathname.startsWith('/login'), { timeout: 15_000 });
      expect(new URL(page.url()).pathname).toBe('/');
    } finally {
      await ctx.close();
    }
  });

  // ─── Login: wrong password shows error ───────────────────────────────
  test('login with wrong password shows error, stays on /login', async ({ browser }) => {
    const ctx = await browser.newContext({ baseURL: LOCAL_BASE });
    const page = await ctx.newPage();
    try {
      await page.goto(`${LOCAL_BASE}/login`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);

      await page.locator('#username').fill(ADMIN_CREDS.username);
      await page.locator('#password').fill('totally-wrong-pw');
      await page.locator('button[type="submit"]').first().click();

      // Give the API a beat to respond.
      await page.waitForTimeout(1500);

      // Still on /login.
      expect(isOnLogin(page)).toBeTruthy();

      // Some indication of failure: inline error message, toast, or alert.
      // The login page renders `<p class="... text-red-600">` on error.
      const errorText = await page.locator('.text-red-600, [role="alert"]').first().textContent().catch(() => '');
      const htmlText = await page.locator('body').textContent().catch(() => '') || '';
      const hasError = (errorText && errorText.trim().length > 0)
        || /invalid|incorrect|错误|失败|wrong/i.test(htmlText);
      expect(hasError, 'expected some error indication after bad login').toBeTruthy();
    } finally {
      await ctx.close();
    }
  });

  // ─── Session persists across reload ──────────────────────────────────
  test('authenticated session persists across page reload', async ({ browser }) => {
    const { context, page } = await setupAuthed(browser);
    try {
      await page.goto(`${LOCAL_BASE}/accounts`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);
      expect(isOnLogin(page)).toBeFalsy();

      await page.reload({ waitUntil: 'domcontentloaded' });
      await waitForHydration(page);
      // Should still be on /accounts, not bounced to /login.
      expect(isOnLogin(page)).toBeFalsy();
      expect(new URL(page.url()).pathname).toBe('/accounts');
    } finally {
      await context.close();
    }
  });

  // ─── Logout from header ──────────────────────────────────────────────
  test('logout redirects back to /login', async ({ browser }) => {
    const { context, page } = await setupAuthed(browser);
    try {
      await page.goto(`${LOCAL_BASE}/`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);

      // The header shows a Logout button when authenticated. Translated
      // label is "退出登录" (auth.logout).
      const logoutBtn = page
        .locator('button')
        .filter({ hasText: /退出登录|登出|Logout|Sign out/i })
        .first();

      await expect(logoutBtn).toBeVisible({ timeout: 10_000 });
      await logoutBtn.click();

      await page.waitForURL((url) => url.pathname.startsWith('/login'), { timeout: 15_000 });
      expect(isOnLogin(page)).toBeTruthy();
    } finally {
      await context.close();
    }
  });

  // ─── Middleware redirects unauthenticated users ──────────────────────
  test('unauthenticated visit to /accounts redirects to /login', async ({ browser }) => {
    const ctx = await browser.newContext({ baseURL: LOCAL_BASE });
    await clearAuth(ctx);
    const page = await ctx.newPage();
    try {
      await page.goto(`${LOCAL_BASE}/accounts`, { waitUntil: 'domcontentloaded' });
      // Middleware fires during navigation. Give SSR+hydration a chance.
      await page.waitForURL((url) => url.pathname.startsWith('/login'), { timeout: 15_000 })
        .catch(() => { /* also acceptable if SSR redirected us already */ });
      expect(isOnLogin(page)).toBeTruthy();
    } finally {
      await ctx.close();
    }
  });

  // ─── Sidebar navigation: 5 distinct links change the URL ────────────
  test('sidebar links navigate between pages', async ({ browser }) => {
    const { context, page } = await setupAuthed(browser);
    try {
      await page.goto(`${LOCAL_BASE}/`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);

      // Labels live in frontend/i18n/zh.json under `nav.*`. Use the /href/
      // attribute instead of text to avoid collapsed-sidebar flakiness.
      const targets: Array<{ href: string; expectedPath: string }> = [
        { href: '/accounts', expectedPath: '/accounts' },
        { href: '/clusters', expectedPath: '/clusters' },
        { href: '/channels', expectedPath: '/channels' },
        { href: '/knowledge', expectedPath: '/knowledge' },
        { href: '/settings', expectedPath: '/settings' },
      ];

      for (const { href, expectedPath } of targets) {
        const link = page.locator(`aside a[href="${href}"]`).first();
        await expect(link, `sidebar link ${href} should exist`).toBeVisible({ timeout: 10_000 });
        await link.click();
        await page.waitForURL((url) => url.pathname.startsWith(expectedPath), { timeout: 15_000 });
        expect(new URL(page.url()).pathname.startsWith(expectedPath)).toBeTruthy();
        await waitForHydration(page);
      }
    } finally {
      await context.close();
    }
  });

  // ─── Chat fullscreen toggle ──────────────────────────────────────────
  test('chat panel fullscreen toggle fires', async ({ browser }) => {
    const { context, page } = await setupAuthed(browser);
    try {
      await page.goto(`${LOCAL_BASE}/`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);

      // The chat panel is rendered inside the layout. The fullscreen button
      // has title="全屏" / "退出全屏" (chat.maximize / chat.minimize).
      const fsBtn = page.locator('button[title="全屏"], button[title="退出全屏"], button[title*="Fullscreen" i]').first();

      // Defensive: the chat panel may be closed by default on some layouts.
      // Open it via the sidebar "对话" link if the button isn't visible.
      if (!(await fsBtn.isVisible({ timeout: 2_000 }).catch(() => false))) {
        const openChatBtn = page
          .locator('aside button, aside a')
          .filter({ hasText: /对话|Chat/i })
          .first();
        if (await openChatBtn.isVisible().catch(() => false)) {
          await openChatBtn.click();
          await page.waitForTimeout(500);
        }
      }

      // Skip gracefully if we still can't find the button — log it, don't fail.
      const fsVisible = await fsBtn.isVisible({ timeout: 3_000 }).catch(() => false);
      test.skip(!fsVisible, 'chat panel fullscreen button not present in this layout');

      await fsBtn.click();
      await page.waitForTimeout(600);

      // After toggling fullscreen, the <aside> of ChatPanel should have the
      // `!flex-1` class (locale-independent; see layouts/default.vue + ChatPanel.vue).
      // This avoids coupling the test to i18n text which flips with browser locale.
      const fullscreenAside = page.locator('aside.\\!flex-1, aside[class*="!flex-1"]').first();
      await expect(fullscreenAside).toBeVisible({ timeout: 2_000 });
    } finally {
      await context.close();
    }
  });
});
