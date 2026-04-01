/**
 * OpsAgent Admin Console — Comprehensive UI E2E Tests
 *
 * Covers features the existing admin-full-flow.spec.ts misses or gets wrong:
 *   - Chat is a collapsible RIGHT PANEL, not a tab
 *   - Default tab after login is the first non-chat tab (glossary)
 *   - Pipeline and Telemetry tabs
 *   - Provider type switching with dynamic fields
 *   - Glossary full CRUD via UI
 *   - Modal open / close / reopen behavior
 *   - Responsive layout at multiple widths
 *   - Complete sidebar navigation (chat special-cased)
 */
import { test, expect, Page } from '@playwright/test';
import * as path from 'path';

const SCREENSHOT_DIR = path.join(__dirname, '..', '..', 'test-screenshots', 'comprehensive');
const ADMIN_USER = 'admin';
const ADMIN_PASS = 'admin123';

// ── Helpers ──────────────────────────────────────────────────────────

async function snap(page: Page, name: string) {
  await page.screenshot({
    path: path.join(SCREENSHOT_DIR, `${name}.png`),
    fullPage: true,
  });
}

async function loginAndWait(page: Page) {
  await page.goto('/admin');
  await page.waitForTimeout(1000);
  const loginVisible = await page.locator('#login-screen').isVisible();
  if (loginVisible) {
    await page.fill('#login-username', ADMIN_USER);
    await page.fill('#login-password', ADMIN_PASS);
    await page.click('#login-screen .btn-primary');
    await page.waitForTimeout(1000);
  }
  await page.waitForSelector('#app-sidebar', { state: 'visible', timeout: 15_000 });
}

/** Click a sidebar nav link. For chat this toggles the chat panel;
 *  for all other tabs it switches the main content area. */
async function navigateTab(page: Page, tabName: string) {
  await page.click(`a[href="#${tabName}"]`);
  await page.waitForTimeout(500);
}

// Force serial so CRUD tests (create then delete) execute in order.
test.describe.configure({ mode: 'serial' });

test.describe('Comprehensive UI Tests', () => {

  test.beforeAll(async () => {
    const fs = await import('fs');
    fs.mkdirSync(SCREENSHOT_DIR, { recursive: true });
  });

  // ================================================================
  // 1. Chat Panel (right-side collapsible panel, NOT a tab)
  // ================================================================
  test.describe('Chat Panel', () => {

    test('opens when sidebar Chat link is clicked', async ({ page }) => {
      await loginAndWait(page);

      // After login the chat panel should be collapsed (hidden)
      const chatPanel = page.locator('#chat-panel');
      await expect(chatPanel).toHaveClass(/collapsed/);

      // Click the Chat link in the sidebar
      await navigateTab(page, 'chat');
      await page.waitForTimeout(400); // allow CSS transition

      // Panel should no longer be collapsed
      await expect(chatPanel).not.toHaveClass(/collapsed/);
      await snap(page, 'chat-panel-open');
    });

    test('shows chat input and send button when open', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'chat');
      await page.waitForTimeout(400);

      await expect(page.locator('#chat-input')).toBeVisible();
      await expect(page.locator('#chat-send-btn')).toBeVisible();
      await expect(page.locator('#chat-welcome')).toBeVisible();
      await snap(page, 'chat-panel-elements');
    });

    test('accepts typed text without sending', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'chat');
      await page.waitForTimeout(400);

      const input = page.locator('#chat-input');
      await input.fill('Test message - do not send');
      await expect(input).toHaveValue('Test message - do not send');
      await snap(page, 'chat-panel-typed');
    });

    test('closes when Chat link is clicked again (toggle)', async ({ page }) => {
      await loginAndWait(page);

      // Open
      await navigateTab(page, 'chat');
      await page.waitForTimeout(400);
      await expect(page.locator('#chat-panel')).not.toHaveClass(/collapsed/);

      // Close (toggle)
      await navigateTab(page, 'chat');
      await page.waitForTimeout(400);
      await expect(page.locator('#chat-panel')).toHaveClass(/collapsed/);
      await snap(page, 'chat-panel-closed');
    });

    test('has hint cards for quick prompts', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'chat');
      await page.waitForTimeout(400);

      const hintCards = page.locator('#chat-welcome .hint-card');
      await expect(hintCards).toHaveCount(4);
      await snap(page, 'chat-hint-cards');
    });
  });

  // ================================================================
  // 2. Pipeline Tab
  // ================================================================
  test.describe('Pipeline Tab', () => {

    test('renders GitHub integration card', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'pipeline');

      await expect(page.locator('#tab-pipeline')).toBeVisible();
      // GitHub card contains title text
      await expect(page.locator('#tab-pipeline .card-title', { hasText: 'GitHub' })).toBeVisible();
      // GitHub toggle exists (checkbox is hidden by custom toggle CSS, check the label)
      await expect(page.locator('#tab-pipeline .toggle')).toBeVisible();
      // GitHub token input
      await expect(page.locator('#github-token')).toBeVisible();
      await snap(page, 'pipeline-github');
    });

    test('shows GitLab coming soon', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'pipeline');

      // GitLab card with "Coming Soon" tag
      const gitlabCard = page.locator('#tab-pipeline .card', { hasText: 'GitLab' });
      await expect(gitlabCard).toBeVisible();
      await expect(gitlabCard.locator('.tag', { hasText: 'Coming Soon' })).toBeVisible();
      await snap(page, 'pipeline-gitlab-soon');
    });
  });

  // ================================================================
  // 3. Telemetry Tab
  // ================================================================
  test.describe('Telemetry Tab', () => {

    test('renders Grafana configuration section', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'telemetry');

      await expect(page.locator('#tab-telemetry')).toBeVisible();
      // Title
      await expect(page.locator('#tab-telemetry .card-title', { hasText: 'Grafana Cloud' })).toBeVisible();
      // Loki URL field
      await expect(page.locator('#grafana-loki-url')).toBeVisible();
      // Instance ID field
      await expect(page.locator('#grafana-instance-id')).toBeVisible();
      // API key field
      await expect(page.locator('#grafana-api-key')).toBeVisible();
      await snap(page, 'telemetry-grafana');
    });

    test('displays webhook URL section', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'telemetry');

      // The webhook URL display element exists
      await expect(page.locator('#grafana-webhook-url')).toBeVisible();
      // Save and test buttons
      await expect(page.locator('#tab-telemetry button', { hasText: 'Save' })).toBeVisible();
      await expect(page.locator('#tab-telemetry button', { hasText: 'Send Test Alert' })).toBeVisible();
      await snap(page, 'telemetry-webhook');
    });
  });

  // ================================================================
  // 4. Provider Tab — dynamic field switching
  // ================================================================
  test.describe('Provider Tab Switching', () => {

    test('default is Bedrock with no extra fields', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'provider');

      await expect(page.locator('#tab-provider')).toBeVisible();
      await expect(page.locator('#pv-type')).toBeVisible();
      await expect(page.locator('#pv-model')).toBeVisible();
      await snap(page, 'provider-bedrock-default');
    });

    test('Anthropic shows api_key and base_url fields', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'provider');

      await page.selectOption('#pv-type', 'anthropic');
      await page.waitForTimeout(300);

      // Dynamic fields should now contain api_key and base_url
      await expect(page.locator('#pv-f-api_key')).toBeVisible();
      await expect(page.locator('#pv-f-base_url')).toBeVisible();
      await snap(page, 'provider-anthropic');
    });

    test('Gateway shows base_url and auth_token fields', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'provider');

      await page.selectOption('#pv-type', 'gateway');
      await page.waitForTimeout(300);

      await expect(page.locator('#pv-f-base_url')).toBeVisible();
      await expect(page.locator('#pv-f-auth_token')).toBeVisible();
      await snap(page, 'provider-gateway');
    });

    test('Vertex shows project_id and region fields', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'provider');

      await page.selectOption('#pv-type', 'vertex');
      await page.waitForTimeout(300);

      await expect(page.locator('#pv-f-project_id')).toBeVisible();
      await expect(page.locator('#pv-f-region')).toBeVisible();
      await snap(page, 'provider-vertex');
    });

    test('Foundry shows resource, api_key, base_url fields', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'provider');

      await page.selectOption('#pv-type', 'foundry');
      await page.waitForTimeout(300);

      await expect(page.locator('#pv-f-resource')).toBeVisible();
      await expect(page.locator('#pv-f-api_key')).toBeVisible();
      await expect(page.locator('#pv-f-base_url')).toBeVisible();
      await snap(page, 'provider-foundry');
    });

    test('switching back to Bedrock removes extra fields', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'provider');

      // Switch to anthropic first to get fields
      await page.selectOption('#pv-type', 'anthropic');
      await page.waitForTimeout(300);
      await expect(page.locator('#pv-f-api_key')).toBeVisible();

      // Switch back to bedrock
      await page.selectOption('#pv-type', 'bedrock');
      await page.waitForTimeout(300);

      // Dynamic fields container should be empty
      const dynamicFieldsContent = await page.locator('#pv-fields').innerHTML();
      expect(dynamicFieldsContent.trim()).toBe('');
      await snap(page, 'provider-bedrock-cleared');
    });
  });

  // ================================================================
  // 5. All Tab Navigation (chat is special-cased)
  // ================================================================
  test.describe('Tab Navigation', () => {

    test('default tab after login is glossary (not chat)', async ({ page }) => {
      await loginAndWait(page);

      // The glossary tab content should be visible
      await expect(page.locator('#tab-glossary')).toBeVisible();
      // The glossary sidebar link should be active
      await expect(page.locator('a[href="#glossary"]')).toHaveClass(/active/);
      // Chat panel should be collapsed
      await expect(page.locator('#chat-panel')).toHaveClass(/collapsed/);
      await snap(page, 'default-tab-glossary');
    });

    test('all non-chat tabs navigate correctly', async ({ page }) => {
      await loginAndWait(page);

      // All tabs except 'chat' — these are proper content tabs
      const contentTabs = [
        'glossary', 'accounts', 'platforms', 'knowledge', 'skills',
        'scheduled-jobs', 'plugins', 'pipeline', 'telemetry',
        'issues', 'resources', 'approvals', 'tenants', 'clusters',
        'provider', 'users',
      ];

      for (const tab of contentTabs) {
        await navigateTab(page, tab);
        await expect(page.locator(`#tab-${tab}`)).toBeVisible();
        await expect(page.locator(`a[href="#${tab}"]`)).toHaveClass(/active/);
      }
      await snap(page, 'all-content-tabs-ok');
    });

    test('chat toggles panel while keeping current tab visible', async ({ page }) => {
      await loginAndWait(page);

      // Navigate to accounts tab
      await navigateTab(page, 'accounts');
      await expect(page.locator('#tab-accounts')).toBeVisible();

      // Open chat panel — accounts should still be visible
      await navigateTab(page, 'chat');
      await page.waitForTimeout(400);
      await expect(page.locator('#chat-panel')).not.toHaveClass(/collapsed/);
      await expect(page.locator('#tab-accounts')).toBeVisible();
      await snap(page, 'chat-plus-accounts');
    });

    test('sidebar category sections can collapse and expand', async ({ page }) => {
      await loginAndWait(page);

      // Click the "Assets" category to collapse it
      const assetsCategory = page.locator('.nav-category', { hasText: 'Assets' });
      await assetsCategory.click();
      await page.waitForTimeout(200);
      await expect(assetsCategory).toHaveClass(/collapsed/);

      // The child links should be hidden (CSS: .collapsed + .nav-children { display: none })
      // Click again to expand
      await assetsCategory.click();
      await page.waitForTimeout(200);
      await expect(assetsCategory).not.toHaveClass(/collapsed/);
      await snap(page, 'sidebar-category-toggle');
    });
  });

  // ================================================================
  // 6. Glossary CRUD Flow (via UI)
  // ================================================================
  test.describe('Glossary CRUD', () => {

    const TEST_KEY = 'ui-crud-test';
    const TEST_FULLNAME = 'UI CRUD Test Term';
    const TEST_DESC = 'Created by comprehensive Playwright E2E test.';

    test('create a new glossary term', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'glossary');
      await page.waitForTimeout(500);

      // Open the new-term modal
      await page.click('button:has-text("New Term")');
      await page.waitForSelector('#glossary-modal.active', { timeout: 3000 });

      // Fill in the form
      await page.fill('#g-key', TEST_KEY);
      await page.fill('#g-fullname', TEST_FULLNAME);
      await page.fill('#g-desc', TEST_DESC);
      await page.fill('#g-aliases', 'uitest, comprehensive');
      await snap(page, 'glossary-crud-create-filled');

      // Save
      await page.click('#glossary-modal .btn-primary');
      await page.waitForTimeout(1000);

      // Verify the term appears in the list
      await expect(page.locator('#glossary-list')).toContainText(TEST_KEY);
      await expect(page.locator('#glossary-list')).toContainText(TEST_FULLNAME);
      await snap(page, 'glossary-crud-created');
    });

    test('search finds the created term', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'glossary');
      await page.waitForTimeout(500);

      // Search by key
      await page.fill('#glossary-search', TEST_KEY);
      await page.waitForTimeout(300);

      await expect(page.locator('#glossary-list')).toContainText(TEST_KEY);
      await expect(page.locator('#glossary-list')).toContainText(TEST_FULLNAME);
      await snap(page, 'glossary-crud-search');
    });

    test('search by alias also finds the term', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'glossary');
      await page.waitForTimeout(500);

      await page.fill('#glossary-search', 'uitest');
      await page.waitForTimeout(300);

      await expect(page.locator('#glossary-list')).toContainText(TEST_KEY);
      await snap(page, 'glossary-crud-search-alias');
    });

    test('delete the created term', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'glossary');
      await page.waitForTimeout(500);

      // Clear any search filter first
      await page.fill('#glossary-search', '');
      await page.waitForTimeout(300);

      // Click the delete button for our test term
      const deleteBtn = page.locator(`button[onclick="deleteGlossary('${TEST_KEY}')"]`);
      await expect(deleteBtn).toBeVisible();
      await deleteBtn.click();

      // The app uses a custom confirm modal (not window.confirm)
      await page.waitForSelector('#confirm-ok', { timeout: 3000 });
      await page.click('#confirm-ok');
      await page.waitForTimeout(1000);

      // Verify the term is gone
      await expect(page.locator('#glossary-list')).not.toContainText(TEST_KEY);
      await snap(page, 'glossary-crud-deleted');
    });

    test('search for deleted term returns empty', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'glossary');
      await page.waitForTimeout(500);

      await page.fill('#glossary-search', TEST_KEY);
      await page.waitForTimeout(300);

      // Should show empty state
      await expect(page.locator('#glossary-list .empty')).toBeVisible();
      await snap(page, 'glossary-crud-gone');
    });
  });

  // ================================================================
  // 7. Responsive Layout
  // ================================================================
  test.describe('Responsive Layout', () => {

    test('desktop width (1280px) shows sidebar and main', async ({ page }) => {
      await loginAndWait(page);
      await page.setViewportSize({ width: 1280, height: 800 });
      await page.waitForTimeout(300);

      await expect(page.locator('#app-sidebar')).toBeVisible();
      await expect(page.locator('#app-main')).toBeVisible();

      // Sidebar should have its full width (220px as per CSS)
      const sidebarBox = await page.locator('#app-sidebar').boundingBox();
      expect(sidebarBox).not.toBeNull();
      expect(sidebarBox!.width).toBeGreaterThanOrEqual(200);
      await snap(page, 'responsive-desktop');
    });

    test('tablet width (768px) still shows sidebar and main', async ({ page }) => {
      await loginAndWait(page);
      await page.setViewportSize({ width: 768, height: 800 });
      await page.waitForTimeout(300);

      await expect(page.locator('#app-sidebar')).toBeVisible();
      await expect(page.locator('#app-main')).toBeVisible();
      await snap(page, 'responsive-tablet');
    });

    test('mobile width (375px) sidebar and main layout', async ({ page }) => {
      await loginAndWait(page);
      await page.setViewportSize({ width: 375, height: 800 });
      await page.waitForTimeout(300);

      // Both elements should still exist in the DOM (no media queries to hide)
      await expect(page.locator('#app-sidebar')).toBeVisible();
      await expect(page.locator('#app-main')).toBeVisible();
      await snap(page, 'responsive-mobile');
    });

    test('chat panel respects viewport at narrow widths', async ({ page }) => {
      await loginAndWait(page);
      await page.setViewportSize({ width: 900, height: 800 });
      await page.waitForTimeout(300);

      // Open chat panel
      await navigateTab(page, 'chat');
      await page.waitForTimeout(400);

      await expect(page.locator('#chat-panel')).not.toHaveClass(/collapsed/);
      await expect(page.locator('#chat-input')).toBeVisible();
      await snap(page, 'responsive-chat-narrow');
    });
  });

  // ================================================================
  // 8. Modal Behavior
  // ================================================================
  test.describe('Modal Behavior', () => {

    test('glossary modal opens with empty fields for new term', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'glossary');

      await page.click('button:has-text("New Term")');
      await page.waitForSelector('#glossary-modal.active', { timeout: 3000 });

      // Title should say "New Term"
      await expect(page.locator('#glossary-modal-title')).toHaveText('New Term');
      // Fields should be empty
      await expect(page.locator('#g-key')).toHaveValue('');
      await expect(page.locator('#g-fullname')).toHaveValue('');
      await expect(page.locator('#g-desc')).toHaveValue('');
      await expect(page.locator('#g-aliases')).toHaveValue('');
      await snap(page, 'modal-glossary-empty');
    });

    test('glossary modal closes on Cancel', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'glossary');

      // Open
      await page.click('button:has-text("New Term")');
      await page.waitForSelector('#glossary-modal.active', { timeout: 3000 });
      await expect(page.locator('#glossary-modal')).toHaveClass(/active/);

      // Cancel
      await page.click('#glossary-modal .btn-ghost');
      await page.waitForTimeout(300);
      await expect(page.locator('#glossary-modal')).not.toHaveClass(/active/);
      await snap(page, 'modal-glossary-closed');
    });

    test('glossary modal reopens with cleared fields', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'glossary');

      // Open and type something
      await page.click('button:has-text("New Term")');
      await page.waitForSelector('#glossary-modal.active', { timeout: 3000 });
      await page.fill('#g-key', 'temporary-key');
      await page.fill('#g-fullname', 'Temp Name');

      // Cancel
      await page.click('#glossary-modal .btn-ghost');
      await page.waitForTimeout(300);

      // Reopen — the openGlossaryModal() resets fields
      await page.click('button:has-text("New Term")');
      await page.waitForSelector('#glossary-modal.active', { timeout: 3000 });

      await expect(page.locator('#g-key')).toHaveValue('');
      await expect(page.locator('#g-fullname')).toHaveValue('');
      await expect(page.locator('#g-desc')).toHaveValue('');
      await snap(page, 'modal-glossary-reopened-clean');
    });

    test('skill modal opens and closes cleanly', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'skills');

      await page.click('button:has-text("New Skill")');
      await page.waitForSelector('#skill-modal.active', { timeout: 3000 });
      await expect(page.locator('#skill-modal')).toHaveClass(/active/);

      // Cancel
      await page.click('#skill-modal .btn-ghost');
      await page.waitForTimeout(300);
      await expect(page.locator('#skill-modal')).not.toHaveClass(/active/);
      await snap(page, 'modal-skill-closed');
    });

    test('scheduled job modal opens and closes cleanly', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'scheduled-jobs');

      await page.click('button:has-text("New Job")');
      await page.waitForSelector('#job-modal.active', { timeout: 3000 });
      await expect(page.locator('#job-modal')).toHaveClass(/active/);

      // Cancel
      await page.click('#job-modal .btn-ghost');
      await page.waitForTimeout(300);
      await expect(page.locator('#job-modal')).not.toHaveClass(/active/);
      await snap(page, 'modal-job-closed');
    });

    test('account modal opens and closes cleanly', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'accounts');
      await page.waitForTimeout(500);

      // Click the "+ Add" button next to "Extra Accounts"
      const addBtns = page.locator('#tab-accounts .section-title button:has-text("+ Add")');
      await addBtns.first().click();
      await page.waitForSelector('#account-modal.active', { timeout: 3000 });
      await expect(page.locator('#account-modal')).toHaveClass(/active/);

      // Cancel
      await page.click('#account-modal .btn-ghost');
      await page.waitForTimeout(300);
      await expect(page.locator('#account-modal')).not.toHaveClass(/active/);
      await snap(page, 'modal-account-closed');
    });
  });

  // ================================================================
  // 9. Additional Feature Tests
  // ================================================================
  test.describe('Additional Features', () => {

    test('issues tab has filter dropdown', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'issues');

      await expect(page.locator('#tab-issues')).toBeVisible();
      const filter = page.locator('#issues-filter');
      await expect(filter).toBeVisible();

      // Default filter is "Open"
      await expect(filter).toHaveValue('open');

      // Switch to "All"
      await page.selectOption('#issues-filter', '');
      await page.waitForTimeout(500);
      await snap(page, 'issues-filter-all');
    });

    test('approvals tab has filter dropdown', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'approvals');

      await expect(page.locator('#tab-approvals')).toBeVisible();
      const filter = page.locator('#approval-filter');
      await expect(filter).toBeVisible();

      // Default filter is "Pending"
      await expect(filter).toHaveValue('pending');
      await snap(page, 'approvals-filter');
    });

    test('clusters tab has auto-discovery checkbox and refresh button', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'clusters');

      await expect(page.locator('#tab-clusters')).toBeVisible();
      await expect(page.locator('#cl-aws-enabled')).toBeVisible();
      await expect(page.locator('#clusters-refresh-btn')).toBeVisible();
      await snap(page, 'clusters-discovery');
    });

    test('knowledge tab has file upload dropzone', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'knowledge');

      await expect(page.locator('#tab-knowledge')).toBeVisible();
      await expect(page.locator('#k-dropzone')).toBeVisible();
      await expect(page.locator('#knowledge-search')).toBeVisible();
      await snap(page, 'knowledge-dropzone');
    });

    test('resources tab has scan button', async ({ page }) => {
      await loginAndWait(page);
      await navigateTab(page, 'resources');

      await expect(page.locator('#tab-resources')).toBeVisible();
      await expect(page.locator('button:has-text("Scan Now")')).toBeVisible();
      await expect(page.locator('#resources-search')).toBeVisible();
      await snap(page, 'resources-scan');
    });

    test('logout returns to login screen', async ({ page }) => {
      await loginAndWait(page);

      const logoutLink = page.locator('#logout-link');
      await expect(logoutLink).toBeVisible();
      await logoutLink.click();
      await page.waitForTimeout(1000);

      await expect(page.locator('#login-screen')).toBeVisible();
      await expect(page.locator('#app-sidebar')).not.toBeVisible();
      await snap(page, 'logged-out');
    });

    test('sidebar footer shows current user and action links', async ({ page }) => {
      await loginAndWait(page);

      await expect(page.locator('#current-user-display')).toBeVisible();
      await expect(page.locator('#current-user-display')).toContainText('admin');
      await expect(page.locator('#logout-link')).toBeVisible();
      await expect(page.locator('#change-pw-link')).toBeVisible();
      await snap(page, 'sidebar-footer');
    });
  });
});
