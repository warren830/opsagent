import { test, expect, Page, BrowserContext } from '@playwright/test';

const BASE = 'https://dg00c54mwvycp.cloudfront.net';
const CREDS = { username: 'admin', password: 'admin123' };

// Helper: click sidebar link by text pattern
async function spaNav(page: Page, textPattern: RegExp, timeout = 3000): Promise<boolean> {
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
// 1. LOGIN PAGE (unauthenticated)
// ============================================================
test.describe('1. Login Page', () => {
  test('1.1 Login form displays correctly', async ({ page }) => {
    await page.goto(`${BASE}/login`);
    await page.waitForLoadState('networkidle');
    await page.screenshot({ path: 'screenshots/01-login-form.png', fullPage: true });

    await expect(page.locator('input').first()).toBeVisible();
    await expect(page.locator('input[type="password"]')).toBeVisible();
    await expect(page.locator('button[type="submit"]')).toBeVisible();
  });

  test('1.2 Invalid credentials show error', async ({ page }) => {
    await page.goto(`${BASE}/login`);
    await page.waitForLoadState('networkidle');
    await page.locator('input').first().fill('wronguser');
    await page.locator('input[type="password"]').fill('wrongpass');
    await page.locator('button[type="submit"]').click();
    await page.waitForTimeout(3000);
    await page.screenshot({ path: 'screenshots/01-login-error.png', fullPage: true });
    expect(page.url()).toContain('/login');
  });

  test('1.3 Unauthenticated redirect to login', async ({ page }) => {
    await page.goto(`${BASE}/clusters`);
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(2000);
    expect(page.url()).toContain('/login');
  });

  test('1.4 [BUG] Page refresh loses auth session (SSR cookie forwarding)', async ({ page }) => {
    // Login first
    await page.goto(`${BASE}/login`);
    await page.waitForLoadState('networkidle');
    await page.locator('input').first().fill(CREDS.username);
    await page.locator('input[type="password"]').fill(CREDS.password);
    await page.locator('button[type="submit"]').click();
    await page.waitForURL((url) => !url.pathname.includes('/login'), { timeout: 15000 });
    await page.waitForLoadState('networkidle');

    // Verify we're on dashboard
    const dashUrl = page.url();
    expect(dashUrl).not.toContain('/login');

    // Now do a hard refresh
    await page.goto(`${BASE}/clusters`);
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(2000);

    const afterRefreshUrl = page.url();
    const isBug = afterRefreshUrl.includes('/login');
    console.log(`[BUG] Hard navigation loses session: ${isBug ? 'CONFIRMED' : 'NOT REPRODUCED'}`);
    console.log(`  Expected: /clusters, Got: ${afterRefreshUrl}`);
    await page.screenshot({ path: 'screenshots/01-bug-hard-nav-loses-auth.png', fullPage: true });

    // Verify the SSR cookie forwarding fix works — hard navigation should NOT lose auth
    expect(isBug).toBe(false);
  });
});

// ============================================================
// 2. FULL APP WALKTHROUGH via SPA navigation
// ============================================================
test.describe('2. Full App Walkthrough', () => {
  let page: Page;
  let context: BrowserContext;
  const bugs: string[] = [];
  const observations: string[] = [];

  test.beforeAll(async ({ browser }) => {
    context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      ignoreHTTPSErrors: true,
    });
    page = await context.newPage();

    // Track console errors
    page.on('console', (msg) => {
      if (msg.type() === 'error') {
        bugs.push(`[CONSOLE ERROR] ${msg.text().substring(0, 200)}`);
      }
    });
    page.on('pageerror', (error) => {
      bugs.push(`[JS EXCEPTION] ${error.message.substring(0, 200)}`);
    });

    // Login
    await page.goto(`${BASE}/login`);
    await page.waitForLoadState('networkidle');
    await page.locator('input').first().fill(CREDS.username);
    await page.locator('input[type="password"]').fill(CREDS.password);
    await page.locator('button[type="submit"]').click();
    await page.waitForURL((url) => !url.pathname.includes('/login'), { timeout: 15000 });
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(2000);
  });

  test.afterAll(async () => {
    console.log('\n========== TEST SUMMARY ==========');
    if (bugs.length > 0) {
      console.log(`\nBUGS FOUND (${bugs.length}):`);
      bugs.forEach((b, i) => console.log(`  ${i + 1}. ${b}`));
    }
    if (observations.length > 0) {
      console.log(`\nOBSERVATIONS (${observations.length}):`);
      observations.forEach((o, i) => console.log(`  ${i + 1}. ${o}`));
    }
    console.log('==================================\n');
    await context.close();
  });

  // ----- DASHBOARD -----
  test('2.1 Dashboard - stats cards and layout', async () => {
    const bodyText = await page.textContent('body') || '';
    await page.screenshot({ path: 'screenshots/02-dashboard.png', fullPage: true });

    // Verify dashboard elements
    expect(bodyText).toContain('Welcome');
    expect(bodyText).toContain('admin');

    // Check stats cards
    const hasStats = /SESSIONS|TENANTS|USERS|SKILLS|CLUSTERS|OPEN ISSUES/i.test(bodyText);
    if (!hasStats) bugs.push('Dashboard: Missing stat cards');
    console.log(`Dashboard stats visible: ${hasStats}`);

    // Check sidebar sections
    const sidebarSections = ['PRINCIPALS', 'ASSETS', 'INTEGRATIONS', 'KNOWLEDGE', 'TOOLS', 'TELEMETRY', 'OPS'];
    for (const section of sidebarSections) {
      if (!bodyText.includes(section)) {
        bugs.push(`Dashboard: Missing sidebar section "${section}"`);
      }
    }
  });

  test('2.2 Dashboard - AI Chat panel visible', async () => {
    const bodyText = await page.textContent('body') || '';
    const hasChat = bodyText.includes('AI Chat') || bodyText.includes('AI\nChat');
    const hasChatInput = await page.locator('textarea, input[placeholder*="infrastructure" i]').first().isVisible().catch(() => false);
    const hasQuickActions = bodyText.includes('List EKS clusters') || bodyText.includes('Check pending pods');

    console.log(`Chat panel: ${hasChat}, Input: ${hasChatInput}, Quick actions: ${hasQuickActions}`);

    if (!hasChat) bugs.push('Dashboard: AI Chat panel not visible');
    if (!hasChatInput) bugs.push('Dashboard: Chat input not visible');

    await page.screenshot({ path: 'screenshots/02-dashboard-chat.png' });
  });

  test('2.3 Dashboard - header bar elements', async () => {
    // Check header elements
    const header = page.locator('header, [class*="header"]').first();
    const bodyText = await page.textContent('body') || '';

    const hasLogo = bodyText.includes('Ops');
    const hasUser = bodyText.includes('admin');
    const hasLogout = bodyText.includes('Logout');

    console.log(`Header - Logo: ${hasLogo}, User: ${hasUser}, Logout: ${hasLogout}`);
    if (!hasLogout) bugs.push('Header: Missing Logout button');
  });

  // ----- TENANTS -----
  test('2.4 Tenants - page loads and table displays', async () => {
    await spaNav(page, /Tenants/);
    await page.screenshot({ path: 'screenshots/03-tenants.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/tenants');

    const bodyText = await page.textContent('body') || '';
    const hasTenantContent = /tenant/i.test(bodyText);
    console.log(`Tenants page content: ${hasTenantContent}`);
  });

  test('2.5 Tenants - create dialog and CRUD', async () => {
    const createBtn = page.locator('button').filter({ hasText: /create|add|new|\+/i }).first();
    const hasCreateBtn = await createBtn.isVisible({ timeout: 3000 }).catch(() => false);
    console.log(`Create tenant button visible: ${hasCreateBtn}`);

    if (hasCreateBtn) {
      await createBtn.click();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: 'screenshots/03-tenants-dialog.png', fullPage: true });

      // Check dialog content
      const dialog = page.locator('[role="dialog"], [class*="Dialog"]');
      const dialogVisible = await dialog.first().isVisible({ timeout: 2000 }).catch(() => false);
      console.log(`Create dialog visible: ${dialogVisible}`);

      if (dialogVisible) {
        // Try to fill and create
        const nameInput = dialog.locator('input').first();
        if (await nameInput.isVisible().catch(() => false)) {
          await nameInput.fill('e2e-test-tenant');
          await page.waitForTimeout(500);

          // Check for slug auto-generation
          const inputs = await dialog.locator('input').all();
          if (inputs.length > 1) {
            const slugValue = await inputs[1].inputValue();
            console.log(`Auto-generated slug: "${slugValue}"`);
            if (!slugValue) observations.push('Tenants: Slug not auto-generated from name');
          }
        }
      }

      await page.keyboard.press('Escape');
      await page.waitForTimeout(500);
    } else {
      observations.push('Tenants: No create button visible');
    }
  });

  // ----- USERS -----
  test('2.6 Users - page loads and shows user list', async () => {
    await spaNav(page, /^Users$/);
    await page.screenshot({ path: 'screenshots/04-users.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/users');

    const bodyText = await page.textContent('body') || '';
    const hasAdmin = bodyText.includes('admin');
    console.log(`Users page shows admin user: ${hasAdmin}`);
    if (!hasAdmin) observations.push('Users: Admin user not shown in list');
  });

  test('2.7 Users - create user dialog', async () => {
    const createBtn = page.locator('button').filter({ hasText: /create|add|invite|new|\+/i }).first();
    if (await createBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await createBtn.click();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: 'screenshots/04-users-dialog.png', fullPage: true });

      const bodyText = await page.textContent('body') || '';
      const hasUserForm = /username|password|role|tenant/i.test(bodyText);
      console.log(`User creation form has fields: ${hasUserForm}`);

      await page.keyboard.press('Escape');
      await page.waitForTimeout(500);
    }
  });

  // ----- CLOUD ACCOUNTS -----
  test('2.8 Accounts - page loads and shows accounts', async () => {
    await spaNav(page, /Cloud Accounts/);
    await page.screenshot({ path: 'screenshots/05-accounts.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/accounts');

    const bodyText = await page.textContent('body') || '';
    const hasProviders = /AWS|Alicloud|Azure/i.test(bodyText);
    console.log(`Accounts shows cloud providers: ${hasProviders}`);
  });

  test('2.9 Accounts - add account dialog', async () => {
    const addBtn = page.locator('button').filter({ hasText: /add|create|new|\+/i }).first();
    if (await addBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await addBtn.click();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: 'screenshots/05-accounts-dialog.png', fullPage: true });

      const bodyText = await page.textContent('body') || '';
      const hasAccountForm = /provider|account|name|region/i.test(bodyText);
      console.log(`Account form fields: ${hasAccountForm}`);

      await page.keyboard.press('Escape');
      await page.waitForTimeout(500);
    }
  });

  // ----- CLUSTERS -----
  test('2.10 Clusters - page loads with sections', async () => {
    await spaNav(page, /^Clusters$/);
    await page.screenshot({ path: 'screenshots/06-clusters.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/clusters');

    const bodyText = await page.textContent('body') || '';
    const hasAutoDiscovery = bodyText.includes('AUTO DISCOVERY') || bodyText.includes('Auto Discovery');
    const hasStaticClusters = bodyText.includes('STATIC CLUSTERS') || bodyText.includes('Static Cluster');
    const hasDiscoveredClusters = bodyText.includes('DISCOVERED CLUSTERS') || bodyText.includes('Discovered');
    console.log(`Clusters - Auto Discovery: ${hasAutoDiscovery}, Static: ${hasStaticClusters}, Discovered: ${hasDiscoveredClusters}`);

    if (!hasAutoDiscovery) observations.push('Clusters: Auto Discovery section missing');
  });

  test('2.11 Clusters - Add Cluster dialog', async () => {
    const addBtn = page.locator('button').filter({ hasText: /Add Cluster/i }).first();
    if (await addBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await addBtn.click();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: 'screenshots/06-clusters-dialog.png', fullPage: true });

      const bodyText = await page.textContent('body') || '';
      const hasForm = /name|endpoint|provider/i.test(bodyText);
      console.log(`Cluster form: ${hasForm}`);

      await page.keyboard.press('Escape');
      await page.waitForTimeout(500);
    }
  });

  test('2.12 Clusters - Refresh Now button', async () => {
    const refreshBtn = page.locator('button').filter({ hasText: /Refresh Now/i }).first();
    const hasRefresh = await refreshBtn.isVisible({ timeout: 3000 }).catch(() => false);
    console.log(`Refresh Now button: ${hasRefresh}`);
  });

  // ----- SERVICE TOPOLOGY -----
  test('2.13 Topology - visualization loads', async () => {
    await spaNav(page, /Service Topology|Topology/);
    await page.waitForTimeout(2000);
    await page.screenshot({ path: 'screenshots/07-topology.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/topology');

    const bodyText = await page.textContent('body') || '';
    const hasTopology = /topology|service|ingress|deployment/i.test(bodyText);
    console.log(`Topology content: ${hasTopology}`);
  });

  // ----- SECURITY INSIGHTS / RESOURCES -----
  test('2.14 Resources - security insights page', async () => {
    await spaNav(page, /Security Insights|Resources/);
    await page.waitForTimeout(2000);
    await page.screenshot({ path: 'screenshots/08-resources.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/resources');

    const bodyText = await page.textContent('body') || '';
    const hasContent = /security|scan|finding|compliance|screener/i.test(bodyText);
    console.log(`Resources content: ${hasContent}`);
  });

  // ----- CHANNELS -----
  test('2.15 Channels - page loads', async () => {
    await spaNav(page, /^Channels$/);
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/09-channels.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/channels');

    const bodyText = await page.textContent('body') || '';
    console.log(`Channels page length: ${bodyText.length}`);
  });

  // ----- MODELS / PROVIDERS -----
  test('2.16 Providers - page loads', async () => {
    await spaNav(page, /^Models$/);
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/10-providers.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/providers');

    const bodyText = await page.textContent('body') || '';
    const hasProviderContent = /provider|model|bedrock|gateway/i.test(bodyText);
    console.log(`Providers content: ${hasProviderContent}`);
  });

  test('2.17 Providers - add provider dialog', async () => {
    const addBtn = page.locator('button').filter({ hasText: /add|create|new|\+/i }).first();
    if (await addBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await addBtn.click();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: 'screenshots/10-providers-dialog.png', fullPage: true });

      const bodyText = await page.textContent('body') || '';
      const hasForm = /provider|type|model|region/i.test(bodyText);
      console.log(`Provider form: ${hasForm}`);

      await page.keyboard.press('Escape');
      await page.waitForTimeout(500);
    }
  });

  // ----- GLOSSARY -----
  test('2.18 Glossary - page loads', async () => {
    await spaNav(page, /^Glossary$/);
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/11-glossary.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/glossary');
  });

  test('2.19 Glossary - add term dialog', async () => {
    const addBtn = page.locator('button').filter({ hasText: /add|create|new|\+/i }).first();
    if (await addBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await addBtn.click();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: 'screenshots/11-glossary-dialog.png', fullPage: true });
      await page.keyboard.press('Escape');
      await page.waitForTimeout(500);
    }
  });

  // ----- KNOWLEDGE -----
  test('2.20 Knowledge - page loads', async () => {
    await spaNav(page, /^Knowledge$/);
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/12-knowledge.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/knowledge');
  });

  // ----- SKILLS -----
  test('2.21 Skills - page loads', async () => {
    await spaNav(page, /^Skills$/);
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/13-skills.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/skills');

    const bodyText = await page.textContent('body') || '';
    const hasSkillsContent = /skill/i.test(bodyText);
    console.log(`Skills content: ${hasSkillsContent}`);
  });

  test('2.22 Skills - add skill dialog', async () => {
    const addBtn = page.locator('button').filter({ hasText: /add|create|import|new|\+/i }).first();
    if (await addBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await addBtn.click();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: 'screenshots/13-skills-dialog.png', fullPage: true });
      await page.keyboard.press('Escape');
      await page.waitForTimeout(500);
    }
  });

  // ----- MCP PLUGINS -----
  test('2.23 MCP Plugins - page loads', async () => {
    await spaNav(page, /MCP Plugins|MCP/);
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/14-mcp.png', fullPage: true });

    const url = page.url();
    console.log(`MCP URL: ${url}`);
  });

  // ----- SCHEDULED JOBS -----
  test('2.24 Scheduled Jobs - page loads', async () => {
    await spaNav(page, /Scheduled Jobs/);
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/15-scheduled-jobs.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/scheduled-jobs');
  });

  // ----- TELEMETRY -----
  test('2.25 Telemetry - page loads', async () => {
    await spaNav(page, /^Telemetry$/);
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/16-telemetry.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/telemetry');

    const bodyText = await page.textContent('body') || '';
    const hasTelemetry = /telemetry|grafana|datadog|dynatrace/i.test(bodyText);
    console.log(`Telemetry content: ${hasTelemetry}`);
  });

  test('2.26 Telemetry - add config dialog', async () => {
    const addBtn = page.locator('button').filter({ hasText: /add|create|new|configure|\+/i }).first();
    if (await addBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await addBtn.click();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: 'screenshots/16-telemetry-dialog.png', fullPage: true });
      await page.keyboard.press('Escape');
      await page.waitForTimeout(500);
    }
  });

  // ----- CODE REPOS -----
  test('2.27 Code Repos - page loads', async () => {
    await spaNav(page, /Code Repos/);
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/17-repos.png', fullPage: true });

    const url = page.url();
    console.log(`Repos URL: ${url}`);
  });

  // ----- DEPLOYMENTS -----
  test('2.28 Deployments - page loads', async () => {
    await spaNav(page, /^Deployments$/);
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/18-deployments.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/deployments');

    const bodyText = await page.textContent('body') || '';
    const hasContent = /deployment|rollout|cluster/i.test(bodyText);
    console.log(`Deployments content: ${hasContent}`);
  });

  // ----- ISSUES -----
  test('2.29 Issues - page loads', async () => {
    await spaNav(page, /^Issues$/);
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/19-issues.png', fullPage: true });

    const url = page.url();
    expect(url).toContain('/issues');

    const bodyText = await page.textContent('body') || '';
    const hasContent = /issue|incident|alert/i.test(bodyText);
    console.log(`Issues content: ${hasContent}`);
  });

  // ----- SETTINGS -----
  test('2.30 Settings - page loads with all sections', async () => {
    // Settings might be in the header/user menu area, navigate via URL
    // But we need SPA nav... let's try clicking user menu
    const settingsLink = page.locator('a[href="/settings"]').first();
    if (await settingsLink.isVisible({ timeout: 3000 }).catch(() => false)) {
      await settingsLink.click();
    } else {
      // Try user menu
      const userBtn = page.locator('button').filter({ hasText: /admin/i }).first();
      if (await userBtn.isVisible({ timeout: 2000 }).catch(() => false)) {
        await userBtn.click();
        await page.waitForTimeout(500);
        const settingsMenu = page.locator('a, button').filter({ hasText: /setting/i }).first();
        if (await settingsMenu.isVisible({ timeout: 2000 }).catch(() => false)) {
          await settingsMenu.click();
        }
      }
    }
    await page.waitForTimeout(1500);
    await page.screenshot({ path: 'screenshots/20-settings.png', fullPage: true });

    const bodyText = await page.textContent('body') || '';
    const hasLanguage = /language|语言/i.test(bodyText);
    const hasTheme = /theme|主题/i.test(bodyText);
    const hasPassword = /password|密码/i.test(bodyText);
    console.log(`Settings - Language: ${hasLanguage}, Theme: ${hasTheme}, Password: ${hasPassword}`);
  });

  // ----- APPROVALS -----
  test('2.31 Approvals - page loads', async () => {
    const approvalsLink = page.locator('a[href="/approvals"]').first();
    if (await approvalsLink.isVisible({ timeout: 3000 }).catch(() => false)) {
      await approvalsLink.click();
      await page.waitForTimeout(1500);
      await page.screenshot({ path: 'screenshots/21-approvals.png', fullPage: true });

      const url = page.url();
      expect(url).toContain('/approvals');
    } else {
      console.log('Approvals link not in sidebar');
      observations.push('Approvals: No direct sidebar link');
    }
  });
});

// ============================================================
// 3. DEEP INTERACTION TESTS
// ============================================================
test.describe('3. Deep Interactions', () => {
  let page: Page;
  let context: BrowserContext;

  test.beforeAll(async ({ browser }) => {
    context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      ignoreHTTPSErrors: true,
    });
    page = await context.newPage();

    await page.goto(`${BASE}/login`);
    await page.waitForLoadState('networkidle');
    await page.locator('input').first().fill(CREDS.username);
    await page.locator('input[type="password"]').fill(CREDS.password);
    await page.locator('button[type="submit"]').click();
    await page.waitForURL((url) => !url.pathname.includes('/login'), { timeout: 15000 });
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(2000);
  });

  test.afterAll(async () => {
    await context.close();
  });

  test('3.1 Tenant CRUD - create and verify', async () => {
    await spaNav(page, /Tenants/);

    const createBtn = page.locator('button').filter({ hasText: /create|add|new|\+/i }).first();
    if (await createBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await createBtn.click();
      await page.waitForTimeout(1000);

      // Fill form
      const dialog = page.locator('[role="dialog"], [class*="Dialog"]').first();
      const inputs = dialog.locator('input');
      const inputCount = await inputs.count();
      console.log(`Tenant dialog inputs: ${inputCount}`);

      if (inputCount > 0) {
        const testName = `e2e-${Date.now()}`;
        await inputs.first().fill(testName);
        await page.waitForTimeout(500);
        await page.screenshot({ path: 'screenshots/30-tenant-create-form.png', fullPage: true });

        // Submit
        const saveBtn = dialog.locator('button').filter({ hasText: /save|create|submit|confirm/i }).first();
        if (await saveBtn.isVisible({ timeout: 2000 }).catch(() => false)) {
          await saveBtn.click();
          await page.waitForTimeout(2000);
          await page.screenshot({ path: 'screenshots/30-tenant-created.png', fullPage: true });

          // Verify it appears in the list
          const bodyText = await page.textContent('body') || '';
          const created = bodyText.includes(testName);
          console.log(`Tenant "${testName}" created and visible: ${created}`);
        }
      }

      // Close if dialog still open
      await page.keyboard.press('Escape').catch(() => {});
      await page.waitForTimeout(500);
    }
  });

  test('3.2 Chat panel - send message', async () => {
    // Navigate back to dashboard
    await spaNav(page, /^Dashboard$/);
    await page.waitForTimeout(1500);

    // Find and fill chat input
    const chatInput = page.locator('textarea[placeholder*="infrastructure" i], input[placeholder*="infrastructure" i]').first();
    if (await chatInput.isVisible({ timeout: 5000 }).catch(() => false)) {
      await chatInput.fill('What services are running?');
      await page.waitForTimeout(500);
      await page.screenshot({ path: 'screenshots/31-chat-filled.png', fullPage: true });

      // Send
      const sendBtn = page.locator('button[type="submit"]').last();
      if (await sendBtn.isVisible().catch(() => false)) {
        await sendBtn.click();
        await page.waitForTimeout(8000); // Wait for response
        await page.screenshot({ path: 'screenshots/31-chat-response.png', fullPage: true });
      }
    } else {
      console.log('Chat input not found');
    }
  });

  test('3.3 Accounts - discover/test connection flow', async () => {
    await spaNav(page, /Cloud Accounts/);
    await page.waitForTimeout(1500);

    // Check for discover button
    const discoverBtn = page.locator('button').filter({ hasText: /discover|scan|sync/i }).first();
    const hasDiscover = await discoverBtn.isVisible({ timeout: 3000 }).catch(() => false);
    console.log(`Account discover button: ${hasDiscover}`);

    // Check for test connection on existing accounts
    const actionBtns = page.locator('button[class*="action"], [class*="dropdown-trigger"]');
    const actionCount = await actionBtns.count();
    console.log(`Account action buttons: ${actionCount}`);
    await page.screenshot({ path: 'screenshots/32-accounts-actions.png', fullPage: true });
  });

  test('3.4 Clusters - Refresh discovery', async () => {
    await spaNav(page, /^Clusters$/);
    await page.waitForTimeout(1500);

    const refreshBtn = page.locator('button').filter({ hasText: /Refresh Now/i }).first();
    if (await refreshBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await refreshBtn.click();
      await page.waitForTimeout(3000);
      await page.screenshot({ path: 'screenshots/33-clusters-refreshed.png', fullPage: true });
    }
  });

  test('3.5 Skills - search functionality', async () => {
    await spaNav(page, /^Skills$/);
    await page.waitForTimeout(1500);

    const searchInput = page.locator('input[placeholder*="search" i], input[type="search"]').first();
    if (await searchInput.isVisible({ timeout: 3000 }).catch(() => false)) {
      await searchInput.fill('deploy');
      await page.waitForTimeout(1000);
      await page.screenshot({ path: 'screenshots/34-skills-search.png', fullPage: true });
      console.log('Skills search works');
    } else {
      console.log('Skills search input not found');
    }
  });
});

// ============================================================
// 4. API & NETWORK MONITORING
// ============================================================
test.describe('4. API & Network', () => {
  let page: Page;
  let context: BrowserContext;

  test.beforeAll(async ({ browser }) => {
    context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      ignoreHTTPSErrors: true,
    });
    page = await context.newPage();

    await page.goto(`${BASE}/login`);
    await page.waitForLoadState('networkidle');
    await page.locator('input').first().fill(CREDS.username);
    await page.locator('input[type="password"]').fill(CREDS.password);
    await page.locator('button[type="submit"]').click();
    await page.waitForURL((url) => !url.pathname.includes('/login'), { timeout: 15000 });
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(2000);
  });

  test.afterAll(async () => {
    await context.close();
  });

  test('4.1 Track all API calls across pages', async () => {
    const apiCalls: { method: string; url: string; status: number }[] = [];

    page.on('response', (resp) => {
      if (resp.url().includes('/api/')) {
        apiCalls.push({
          method: resp.request().method(),
          url: resp.url().replace(BASE, ''),
          status: resp.status(),
        });
      }
    });

    // Navigate to all key pages via SPA
    const navs: [RegExp, string][] = [
      [/^Dashboard$/, 'Dashboard'],
      [/Tenants/, 'Tenants'],
      [/^Users$/, 'Users'],
      [/Cloud Accounts/, 'Accounts'],
      [/^Clusters$/, 'Clusters'],
      [/Service Topology/, 'Topology'],
      [/Security Insights/, 'Resources'],
      [/^Channels$/, 'Channels'],
      [/^Models$/, 'Providers'],
      [/^Glossary$/, 'Glossary'],
      [/^Knowledge$/, 'Knowledge'],
      [/^Skills$/, 'Skills'],
      [/^Telemetry$/, 'Telemetry'],
      [/^Deployments$/, 'Deployments'],
      [/^Issues$/, 'Issues'],
    ];

    for (const [pattern, name] of navs) {
      const success = await spaNav(page, pattern);
      if (!success) console.log(`  Could not navigate to ${name}`);
    }

    console.log('\n=== API Call Summary ===');
    const uniqueCalls = new Map<string, { status: number; count: number }>();
    for (const call of apiCalls) {
      const key = `${call.method} ${call.url}`;
      const existing = uniqueCalls.get(key);
      if (existing) {
        existing.count++;
      } else {
        uniqueCalls.set(key, { status: call.status, count: 1 });
      }
    }

    const errors: string[] = [];
    for (const [key, val] of uniqueCalls) {
      const icon = val.status >= 200 && val.status < 400 ? '✓' : '✗';
      console.log(`  ${icon} [${val.status}] ${key} (×${val.count})`);
      if (val.status >= 400 && val.status !== 404) {
        errors.push(`[${val.status}] ${key}`);
      }
    }

    if (errors.length > 0) {
      console.log(`\n  API Errors: ${errors.length}`);
      errors.forEach(e => console.log(`    ✗ ${e}`));
    }
  });
});

// ============================================================
// 5. RESPONSIVE LAYOUT
// ============================================================
test.describe('5. Responsive', () => {
  let page: Page;
  let context: BrowserContext;

  test.beforeAll(async ({ browser }) => {
    context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      ignoreHTTPSErrors: true,
    });
    page = await context.newPage();

    await page.goto(`${BASE}/login`);
    await page.waitForLoadState('networkidle');
    await page.locator('input').first().fill(CREDS.username);
    await page.locator('input[type="password"]').fill(CREDS.password);
    await page.locator('button[type="submit"]').click();
    await page.waitForURL((url) => !url.pathname.includes('/login'), { timeout: 15000 });
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(2000);
  });

  test.afterAll(async () => {
    await context.close();
  });

  test('5.1 Desktop 1440x900', async () => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.waitForTimeout(1000);
    await page.screenshot({ path: 'screenshots/50-desktop.png', fullPage: true });
  });

  test('5.2 Tablet 1024x768', async () => {
    await page.setViewportSize({ width: 1024, height: 768 });
    await page.waitForTimeout(1000);
    await page.screenshot({ path: 'screenshots/50-tablet.png', fullPage: true });
  });

  test('5.3 Mobile 375x812', async () => {
    await page.setViewportSize({ width: 375, height: 812 });
    await page.waitForTimeout(1000);
    await page.screenshot({ path: 'screenshots/50-mobile.png', fullPage: true });
  });
});

// ============================================================
// 6. PERFORMANCE
// ============================================================
test.describe('6. Performance', () => {
  let page: Page;
  let context: BrowserContext;

  test.beforeAll(async ({ browser }) => {
    context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      ignoreHTTPSErrors: true,
    });
    page = await context.newPage();

    await page.goto(`${BASE}/login`);
    await page.waitForLoadState('networkidle');
    await page.locator('input').first().fill(CREDS.username);
    await page.locator('input[type="password"]').fill(CREDS.password);
    await page.locator('button[type="submit"]').click();
    await page.waitForURL((url) => !url.pathname.includes('/login'), { timeout: 15000 });
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(2000);
  });

  test.afterAll(async () => {
    await context.close();
  });

  test('6.1 SPA navigation performance', async () => {
    const navs: [RegExp, string][] = [
      [/Tenants/, 'Tenants'],
      [/^Users$/, 'Users'],
      [/Cloud Accounts/, 'Accounts'],
      [/^Clusters$/, 'Clusters'],
      [/^Deployments$/, 'Deployments'],
      [/^Issues$/, 'Issues'],
      [/^Skills$/, 'Skills'],
      [/^Glossary$/, 'Glossary'],
      [/^Knowledge$/, 'Knowledge'],
      [/^Telemetry$/, 'Telemetry'],
    ];

    console.log('\n=== SPA Navigation Times ===');
    for (const [pattern, name] of navs) {
      const start = Date.now();
      await spaNav(page, pattern);
      const elapsed = Date.now() - start;
      const icon = elapsed < 2000 ? '✓' : elapsed < 5000 ? '⚠' : '✗';
      console.log(`  ${icon} ${name}: ${elapsed}ms`);
    }
  });
});
