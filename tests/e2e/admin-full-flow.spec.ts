/**
 * OpsAgent Admin Console — Full Flow E2E Test
 * Tests every page/tab with screenshots at each step.
 */
import { test, expect, Page } from '@playwright/test';
import * as path from 'path';

const SCREENSHOT_DIR = path.join(__dirname, '..', '..', 'test-screenshots');
const ADMIN_USER = 'admin';
const ADMIN_PASS = 'admin123';

/** Helper: take a named screenshot */
async function snap(page: Page, name: string) {
  await page.screenshot({ path: path.join(SCREENSHOT_DIR, `${name}.png`), fullPage: true });
}

/** Helper: click a sidebar nav tab and wait for it to render */
async function switchTab(page: Page, tabName: string) {
  await page.click(`a[href="#${tabName}"]`);
  await page.waitForTimeout(500);
}

/** Helper: login via UI if login screen is shown, then wait for sidebar */
async function loginAndWait(page: Page) {
  await page.goto('/admin');
  await page.waitForTimeout(1000);

  // Check if login screen is visible
  const loginVisible = await page.locator('#login-screen').isVisible();
  if (loginVisible) {
    await page.fill('#login-username', ADMIN_USER);
    await page.fill('#login-password', ADMIN_PASS);
    await page.click('#login-screen .btn-primary');
    await page.waitForTimeout(1000);
  }

  // Wait for sidebar to appear (either from login or api_key mode)
  await page.waitForSelector('#app-sidebar', { state: 'visible', timeout: 15_000 });
}

// Force serial execution — tests depend on ordering (create then delete)
test.describe.configure({ mode: 'serial' });

test.describe('Admin Console Full Flow', () => {

  test.beforeAll(async () => {
    const fs = await import('fs');
    fs.mkdirSync(SCREENSHOT_DIR, { recursive: true });
  });

  // ── Health & Bootstrap ─────────────────────────────────────────

  test('01 — Health check', async ({ request }) => {
    const res = await request.get('/health');
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.status).toBe('ok');
  });

  test('02 — Login screen', async ({ page }) => {
    await page.goto('/admin');
    await page.waitForTimeout(1500);
    await snap(page, '02-login-screen');
  });

  test('03 — Login and see admin console', async ({ page }) => {
    await page.goto('/admin');
    await page.waitForTimeout(1000);

    const loginVisible = await page.locator('#login-screen').isVisible();
    if (loginVisible) {
      await page.fill('#login-username', ADMIN_USER);
      await page.fill('#login-password', ADMIN_PASS);
      await snap(page, '03a-login-filled');

      await page.click('#login-screen .btn-primary');
      await page.waitForTimeout(1500);
    }

    await page.waitForSelector('#app-sidebar', { state: 'visible', timeout: 15_000 });
    await expect(page.locator('#app-sidebar')).toBeVisible();
    await expect(page.locator('#app-main')).toBeVisible();
    await snap(page, '03b-admin-loaded');
  });

  // ── Chat Tab ───────────────────────────────────────────────────

  test('04 — Chat tab (default)', async ({ page }) => {
    await loginAndWait(page);
    await expect(page.locator('#tab-chat')).toBeVisible();
    await expect(page.locator('#chat-welcome')).toBeVisible();
    await expect(page.locator('.hint-card')).toHaveCount(4);
    await snap(page, '04-chat-tab');
  });

  test('05 — Chat: type a message', async ({ page }) => {
    await loginAndWait(page);
    await page.fill('#chat-input', '你好，帮我检查一下 EKS 集群状态');
    await snap(page, '05a-chat-typed');

    await page.click('#chat-send-btn');
    await page.waitForTimeout(2000);
    await snap(page, '05b-chat-sent');
  });

  // ── Data Tabs ──────────────────────────────────────────────────

  test('06 — Glossary tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'glossary');
    await expect(page.locator('#tab-glossary')).toBeVisible();
    await snap(page, '06-glossary-tab');
  });

  test('07 — Glossary: add a new term', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'glossary');

    await page.click('button:has-text("New Term")');
    await page.waitForSelector('#glossary-modal.active', { timeout: 3000 });
    await snap(page, '07a-glossary-modal-open');

    await page.fill('#g-key', 'e2e-test');
    await page.fill('#g-fullname', 'E2E Test Term');
    await page.fill('#g-desc', 'Created by Playwright E2E test.');
    await page.fill('#g-aliases', 'e2e, playwright');
    await snap(page, '07b-glossary-modal-filled');

    await page.click('#glossary-modal .btn-primary');
    await page.waitForTimeout(1000);
    await snap(page, '07c-glossary-saved');
    await expect(page.locator('#glossary-list')).toContainText('e2e-test');
  });

  test('08 — Glossary: search', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'glossary');
    await page.waitForTimeout(500);
    await page.fill('#glossary-search', 'beta');
    await page.waitForTimeout(300);
    await snap(page, '08-glossary-search');
  });

  test('09 — Knowledge tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'knowledge');
    await expect(page.locator('#tab-knowledge')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '09-knowledge-tab');
  });

  test('10 — Skills tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'skills');
    await expect(page.locator('#tab-skills')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '10-skills-tab');
  });

  // ── Infrastructure Tabs ────────────────────────────────────────

  test('11 — Accounts tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'accounts');
    await expect(page.locator('#tab-accounts')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '11-accounts-tab');
  });

  test('12 — Accounts: add extra account', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'accounts');
    await page.waitForTimeout(500);

    // Find the first "+ Add" button in the Extra Accounts section
    const addBtns = page.locator('#tab-accounts .section-title button:has-text("+ Add")');
    await addBtns.first().click();
    await page.waitForSelector('#account-modal.active', { timeout: 3000 });
    await snap(page, '12a-account-modal-open');

    await page.fill('#a-id', '999999999999');
    await page.fill('#a-name', 'e2e-test-account');
    await page.fill('#a-regions', 'us-west-2');
    await snap(page, '12b-account-modal-filled');

    await page.click('#account-modal .btn-primary');
    await page.waitForTimeout(1000);
    await snap(page, '12c-account-saved');
  });

  test('13 — Clusters tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'clusters');
    await expect(page.locator('#tab-clusters')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '13-clusters-tab');
  });

  test('14 — Platforms tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'platforms');
    await expect(page.locator('#tab-platforms')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '14-platforms-tab');
  });

  // ── Automation Tabs ────────────────────────────────────────────

  test('15 — Scheduled Jobs tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'scheduled-jobs');
    await expect(page.locator('#tab-scheduled-jobs')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '15-scheduled-jobs-tab');
  });

  test('16 — Plugins tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'plugins');
    await expect(page.locator('#tab-plugins')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '16-plugins-tab');
  });

  // ── Ops Tabs ───────────────────────────────────────────────────

  test('17 — Issues tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'issues');
    await expect(page.locator('#tab-issues')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '17-issues-tab');
  });

  test('18 — Resources tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'resources');
    await expect(page.locator('#tab-resources')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '18-resources-tab');
  });

  // ── Admin Tabs ─────────────────────────────────────────────────

  test('19 — Approvals tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'approvals');
    await expect(page.locator('#tab-approvals')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '19-approvals-tab');
  });

  test('20 — Tenants tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'tenants');
    await expect(page.locator('#tab-tenants')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '20-tenants-tab');
  });

  test('21 — Provider tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'provider');
    await expect(page.locator('#tab-provider')).toBeVisible();
    await expect(page.locator('#pv-type')).toBeVisible();
    await expect(page.locator('#pv-model')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '21-provider-tab');
  });

  test('22 — Users tab', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'users');
    await expect(page.locator('#tab-users')).toBeVisible();
    await page.waitForTimeout(500);
    await snap(page, '22-users-tab');
  });

  // ── CRUD: create user then delete ──────────────────────────────

  test('23 — Users: create new user', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'users');
    await page.waitForTimeout(500);

    await page.click('button:has-text("+ Add User")');
    await page.waitForSelector('#user-modal.active', { timeout: 3000 });
    await snap(page, '23a-user-modal-open');

    await page.fill('#u-username', 'e2e-user');
    await page.fill('#u-password', 'TestPass123!');
    await snap(page, '23b-user-modal-filled');

    await page.click('#user-modal .btn-primary');
    await page.waitForTimeout(1000);
    await snap(page, '23c-user-created');
    await expect(page.locator('#users-list')).toContainText('e2e-user');
  });

  test('24 — Users: delete test user', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'users');
    await page.waitForTimeout(500);

    const userCard = page.locator('.card', { hasText: 'e2e-user' });
    if (await userCard.count() > 0) {
      page.on('dialog', d => d.accept());
      await userCard.locator('button:has-text("Delete")').click();
      await page.waitForTimeout(1000);
    }
    await snap(page, '24-user-deleted');
  });

  // ── CRUD: glossary cleanup ─────────────────────────────────────

  test('25 — Glossary: delete test term', async ({ page }) => {
    await loginAndWait(page);
    await switchTab(page, 'glossary');
    await page.waitForTimeout(500);

    // Use the glossary-specific delete button to avoid matching other cards
    const deleteBtn = page.locator('button[onclick="deleteGlossary(\'e2e-test\')"]');
    if (await deleteBtn.count() > 0) {
      page.on('dialog', d => d.accept());
      await deleteBtn.click();
      await page.waitForTimeout(1000);
    }
    await snap(page, '25-glossary-cleaned');
  });

  // ── Responsive ─────────────────────────────────────────────────

  test('26 — Responsive: multiple viewports', async ({ page }) => {
    await loginAndWait(page);

    await page.setViewportSize({ width: 1280, height: 800 });
    await snap(page, '26a-responsive-wide');

    await page.setViewportSize({ width: 768, height: 800 });
    await page.waitForTimeout(300);
    await snap(page, '26b-responsive-tablet');

    await page.setViewportSize({ width: 375, height: 800 });
    await page.waitForTimeout(300);
    await snap(page, '26c-responsive-mobile');
  });

  // ── Navigation round-trip ──────────────────────────────────────

  test('27 — All tabs navigate correctly', async ({ page }) => {
    await loginAndWait(page);

    const tabs = [
      'chat', 'glossary', 'knowledge', 'skills', 'accounts', 'clusters',
      'platforms', 'scheduled-jobs', 'plugins', 'issues', 'resources',
      'approvals', 'tenants', 'provider', 'users',
    ];

    for (const tab of tabs) {
      await switchTab(page, tab);
      await expect(page.locator(`#tab-${tab}`)).toBeVisible();
      await expect(page.locator(`a[href="#${tab}"]`)).toHaveClass(/active/);
    }
    await snap(page, '27-all-tabs-ok');
  });

  // ── Logout ─────────────────────────────────────────────────────

  test('28 — Logout returns to login screen', async ({ page }) => {
    await loginAndWait(page);
    await snap(page, '28a-before-logout');

    // Click logout
    const logoutLink = page.locator('#logout-link');
    if (await logoutLink.isVisible()) {
      await logoutLink.click();
      await page.waitForTimeout(1000);
      await expect(page.locator('#login-screen')).toBeVisible();
      await snap(page, '28b-logged-out');
    }
  });
});
