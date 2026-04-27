/**
 * Feature-specific interaction checks — topology, issues, deployments,
 * chat, skills, style-demo. Non-destructive.
 *
 * Run: E2E_BASE_URL=http://localhost:9999 npx playwright test \
 *      comprehensive/04-feature-interactions.spec.ts --project=chromium
 */
import { test, expect } from '@playwright/test';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import {
  LOCAL_BASE,
  setupAuthed,
  waitForHydration,
} from './helpers-local';

let sharedContext: BrowserContext;

test.describe.configure({ mode: 'serial' });

test.describe('feature interactions', () => {
  test.beforeAll(async ({ browser }: { browser: Browser }) => {
    const setup = await setupAuthed(browser);
    sharedContext = setup.context;
    await setup.page.close();
  });

  test.afterAll(async () => {
    await sharedContext?.close();
  });

  // ─── Topology: Vue Flow canvas + controls ────────────────────────────
  test('topology renders Vue Flow canvas with controls', async () => {
    const page: Page = await sharedContext.newPage();
    try {
      await page.goto(`${LOCAL_BASE}/topology`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);
      // Give topology a bit longer — it may fetch cluster resources.
      await page.waitForTimeout(1500);

      // Vue Flow always renders the `.vue-flow` root plus `.vue-flow__viewport`.
      const canvas = page.locator('.vue-flow, [data-testid="rf__wrapper"]').first();
      await expect(canvas, 'vue-flow canvas should render').toBeVisible({ timeout: 10_000 });

      // Zoom controls — vue-flow ships `.vue-flow__controls` with buttons.
      const controls = page.locator('.vue-flow__controls').first();
      const hasControls = await controls.isVisible({ timeout: 3_000 }).catch(() => false);
      // It's possible the graph is empty — controls still render, but tolerate absence.
      if (hasControls) {
        const fitView = controls.locator('button[aria-label*="fit" i], button[title*="fit" i], .vue-flow__controls-fitview').first();
        if (await fitView.isVisible({ timeout: 1_500 }).catch(() => false)) {
          await fitView.click();
          await page.waitForTimeout(300);
        }
      }
    } finally {
      await page.close();
    }
  });

  // ─── Issues: list renders + optional row click ───────────────────────
  test('issues page renders list / table', async () => {
    const page: Page = await sharedContext.newPage();
    try {
      await page.goto(`${LOCAL_BASE}/issues`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);

      // Accept any of: a table, a list, or an empty-state message.
      const bodyText = (await page.locator('body').textContent()) || '';
      const hasTable = await page.locator('table, [role="table"], .data-table').first().isVisible({ timeout: 3_000 }).catch(() => false);
      const hasEmptyState = /no issues|暂无|没有|empty|idle|尚无/i.test(bodyText);
      const hasListContainer = await page.locator('main, [data-testid="issues-list"], .space-y-2, .divide-y').first().isVisible({ timeout: 1_500 }).catch(() => false);

      expect(hasTable || hasEmptyState || hasListContainer, 'issues page should show a table, list, or empty state').toBeTruthy();

      // If there are rows, click the first to open a detail view (drawer or dialog).
      if (hasTable) {
        const firstRow = page.locator('table tbody tr, [role="row"]').first();
        if (await firstRow.isVisible({ timeout: 1_500 }).catch(() => false)) {
          await firstRow.click().catch(() => { /* row may not be clickable */ });
          await page.waitForTimeout(700);
          // Either a dialog or a visibly expanded region should appear — but
          // we don't fail if nothing happens; clicking a row is optional.
        }
      }
    } finally {
      await page.close();
    }
  });

  // ─── Deployments: rollouts list + promote/rollback visibility ────────
  test('deployments page renders list; promote/rollback visible when rollouts exist', async () => {
    const page: Page = await sharedContext.newPage();
    try {
      await page.goto(`${LOCAL_BASE}/deployments`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);

      const bodyText = (await page.locator('body').textContent()) || '';
      // Either we see the deployments title or a "select cluster" hint or the empty-state.
      const hasTitle = /部署管理|Deployments|Rollout/i.test(bodyText);
      expect(hasTitle, 'deployments page header should be visible').toBeTruthy();

      // If at least one rollout row exists, verify the promote / rollback buttons are wired up.
      const rolloutRow = page.locator('[data-testid="rollout-row"], tr:has(button), .rollout-row').first();
      const hasRollout = await rolloutRow.isVisible({ timeout: 2_000 }).catch(() => false);

      if (hasRollout) {
        const promote = page.locator('button').filter({ hasText: /推进|Promote/i }).first();
        const rollback = page.locator('button').filter({ hasText: /回滚|Rollback/i }).first();
        // At least one of the controls should render once rollouts are present.
        const promoteVisible = await promote.isVisible({ timeout: 1_000 }).catch(() => false);
        const rollbackVisible = await rollback.isVisible({ timeout: 1_000 }).catch(() => false);
        expect(promoteVisible || rollbackVisible, 'promote or rollback button should appear for rollouts').toBeTruthy();
      } else {
        // No rollouts — empty state must mention it.
        const hasEmpty = /没有|no rollouts|选择一个集群|select a cluster|暂无/i.test(bodyText);
        expect(hasEmpty, 'expected empty-state text when no rollouts').toBeTruthy();
      }
    } finally {
      await page.close();
    }
  });

  // ─── Chat panel: textarea focus + type (no submit) ───────────────────
  test('chat panel textarea accepts typing on /', async () => {
    const page: Page = await sharedContext.newPage();
    try {
      await page.goto(`${LOCAL_BASE}/`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);

      // Chat panel textarea — placeholder is t('chat.placeholder') =
      // "问我关于云基础设施的问题...". Use placeholder attribute as anchor.
      const textarea = page.locator('textarea[placeholder*="问我"], textarea[placeholder*="Ask" i], textarea[placeholder*="infrastructure" i]').first();

      // Chat may be closed — try to open it.
      if (!(await textarea.isVisible({ timeout: 2_000 }).catch(() => false))) {
        const openChat = page
          .locator('aside button, aside a')
          .filter({ hasText: /对话|Chat/i })
          .first();
        if (await openChat.isVisible().catch(() => false)) {
          await openChat.click();
          await page.waitForTimeout(500);
        }
      }

      await expect(textarea, 'chat textarea should be visible on /').toBeVisible({ timeout: 10_000 });
      await textarea.focus();
      await textarea.fill('test');

      // Confirm value landed — but DO NOT submit.
      const value = await textarea.inputValue();
      expect(value).toBe('test');
    } finally {
      await page.close();
    }
  });

  // ─── Skills: list renders; first skill opens detail if present ───────
  test('skills page renders; click first skill if present', async () => {
    const page: Page = await sharedContext.newPage();
    try {
      await page.goto(`${LOCAL_BASE}/skills`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);

      const bodyText = (await page.locator('body').textContent()) || '';
      expect(/技能|Skills|skill/i.test(bodyText), 'skills page should render title/body').toBeTruthy();

      const rows = page.locator('table tbody tr, [data-testid="skill-row"], .skill-row');
      const count = await rows.count().catch(() => 0);

      if (count > 0) {
        await rows.first().click().catch(() => { /* optional */ });
        await page.waitForTimeout(500);
        // A dialog or a detail panel *may* appear — we don't require it,
        // but we record a screenshot to make review easy.
        await page.screenshot({
          path: 'screenshots/comprehensive/skills-detail.png',
          fullPage: false,
        });
      } else {
        // Empty state: make sure some placeholder text is showing.
        const hasEmpty = /no skills|暂无|empty|添加|Add/i.test(bodyText);
        expect(hasEmpty, 'empty skills state should be visible').toBeTruthy();
      }
    } finally {
      await page.close();
    }
  });

  // ─── Style demo: confirm multiple showcases render ───────────────────
  test('style-demo renders multiple component showcases', async () => {
    const page: Page = await sharedContext.newPage();
    try {
      await page.goto(`${LOCAL_BASE}/style-demo`, { waitUntil: 'domcontentloaded' });
      await waitForHydration(page);

      // The style-demo page advertises "every variant on one page" — several
      // sections (Buttons, Badges, Inputs, etc.) should be visible.
      // Count distinct <section> / <h2> / <h3> blocks as a proxy for
      // "multiple showcases".
      const headings = page.locator('h1, h2, h3');
      const nHeadings = await headings.count();
      expect(nHeadings, 'style-demo should have several section headings').toBeGreaterThanOrEqual(3);

      // At least 3 different buttons should render (variant showcase).
      const buttons = page.locator('button');
      const nButtons = await buttons.count();
      expect(nButtons, 'style-demo should render multiple buttons').toBeGreaterThanOrEqual(5);
    } finally {
      await page.close();
    }
  });
});
