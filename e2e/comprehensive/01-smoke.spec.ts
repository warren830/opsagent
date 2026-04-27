/**
 * Smoke test — every first-class route must load without console errors
 * and render meaningful body content.
 *
 * Run: E2E_BASE_URL=http://localhost:9999 npx playwright test \
 *      comprehensive/01-smoke.spec.ts --project=chromium
 */
import { test, expect } from '@playwright/test';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import {
  LOCAL_BASE,
  captureConsole,
  routeSlug,
  setupAuthed,
  waitForHydration,
} from './helpers-local';

// Authenticated routes — 22 total (excluding /login which has no auth).
const AUTH_ROUTES = [
  '/',
  '/accounts',
  '/approvals',
  '/channels',
  '/clusters',
  '/deployments',
  '/glossary',
  '/issues',
  '/knowledge',
  '/mcp',
  '/providers',
  '/repo',
  '/resources',
  '/scheduled-jobs',
  '/settings',
  '/skills',
  '/style-demo',
  '/telemetry',
  '/tenants',
  '/topology',
  '/users',
];

// Callback routes — unauthenticated, just must not crash the render.
const CALLBACK_ROUTES = [
  '/auth/cognito/callback',
  '/auth/invite',
  '/auth/microsoft/callback',
];

// Share one authenticated context across all smoke routes — spinning up a
// context per test is slow and unnecessary for read-only smoke checks.
let sharedContext: BrowserContext;
let sharedPage: Page;

test.describe.configure({ mode: 'serial' });

test.describe('smoke: every route renders', () => {
  test.beforeAll(async ({ browser }: { browser: Browser }) => {
    const setup = await setupAuthed(browser);
    sharedContext = setup.context;
    sharedPage = setup.page;
  });

  test.afterAll(async () => {
    await sharedContext?.close();
  });

  // Authenticated app routes
  for (const route of AUTH_ROUTES) {
    test(`route ${route} loads cleanly`, async () => {
      const page = await sharedContext.newPage();
      const console$ = captureConsole(page);
      try {
        await page.goto(`${LOCAL_BASE}${route}`, { waitUntil: 'domcontentloaded' });
        await waitForHydration(page);

        // Still on the intended route (not kicked back to /login).
        const finalPath = new URL(page.url()).pathname;
        expect(
          finalPath === route || finalPath.startsWith(route),
          `expected to stay on ${route}, got ${finalPath}`,
        ).toBeTruthy();

        // Body has non-trivial text content.
        const bodyText = (await page.locator('body').textContent()) || '';
        expect(bodyText.trim().length, `body looked empty on ${route}`).toBeGreaterThan(20);

        // Capture screenshot for human review.
        await page.screenshot({
          path: `screenshots/comprehensive/smoke-${routeSlug(route)}.png`,
          fullPage: true,
        });

        // No unhandled console errors.
        expect(console$.errors, `console errors on ${route}:\n${console$.errors.join('\n')}`).toEqual([]);
      } finally {
        await page.close();
      }
    });
  }

  // /login is public — must render even without auth.
  test('route /login loads cleanly (unauthenticated)', async ({ browser }) => {
    const ctx = await browser.newContext({ baseURL: LOCAL_BASE });
    const page = await ctx.newPage();
    const console$ = captureConsole(page);
    try {
      await page.goto(`${LOCAL_BASE}/login`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);
      expect(new URL(page.url()).pathname).toBe('/login');
      const bodyText = (await page.locator('body').textContent()) || '';
      expect(bodyText.trim().length).toBeGreaterThan(20);
      await page.screenshot({
        path: `screenshots/comprehensive/smoke-login.png`,
        fullPage: true,
      });
      expect(console$.errors).toEqual([]);
    } finally {
      await ctx.close();
    }
  });

  // Callback routes — no auth, don't crash.
  for (const route of CALLBACK_ROUTES) {
    test(`callback ${route} renders without crashing`, async ({ browser }) => {
      const ctx = await browser.newContext({ baseURL: LOCAL_BASE });
      const page = await ctx.newPage();
      // Don't assert zero console errors for callback routes — they deliberately
      // show an error state when query params are absent, which may log to console.
      try {
        await page.goto(`${LOCAL_BASE}${route}`, { waitUntil: 'domcontentloaded' });
        await waitForHydration(page);
        // Body must render something (not white-screen-of-death).
        const bodyText = (await page.locator('body').textContent()) || '';
        expect(bodyText.trim().length, `body was empty on ${route}`).toBeGreaterThan(0);
        await page.screenshot({
          path: `screenshots/comprehensive/smoke-${routeSlug(route)}.png`,
          fullPage: true,
        });
      } finally {
        await ctx.close();
      }
    });
  }
});
