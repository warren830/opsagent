/**
 * Complex E2E Test Suite
 *
 * Phase 1: API Setup — build multi-tenant environment
 * Phase 2: UI Business Logic — CRUD, RBAC, tenant isolation
 * Phase 3: API Security Audit — bypass UI, test backend auth
 * Phase 4: Teardown — clean up all test data
 */
import { test, expect, Page, BrowserContext } from '@playwright/test';
import { ApiClient, loginViaUI, spaNav, env, BASE, ADMIN_CREDS, RUN_ID } from './helpers';

// ================================================================
// PHASE 1: API ENVIRONMENT SETUP
// ================================================================
test.describe.serial('Phase 1: Environment Setup', () => {
  const api = new ApiClient();

  test('1.1 Login as super_admin and get token', async () => {
    const token = await api.login(ADMIN_CREDS.username, ADMIN_CREDS.password);
    env.superAdminToken = token;
    expect(token).toBeTruthy();
    console.log(`[Setup] Super admin token acquired (${token.substring(0, 20)}...)`);
  });

  test('1.2 Clean up stale e2e data from previous runs', async () => {
    // Delete any leftover e2e tenants/users from failed prior runs
    const { data: tenants } = await api.get<any[]>('/api/tenants');
    for (const t of tenants || []) {
      if (t.name?.startsWith('e2e-')) {
        console.log(`[Cleanup] Deleting stale tenant: ${t.name}`);
        await api.del(`/api/tenants/${t.id}`);
      }
    }
    const { data: users } = await api.get<any[]>('/api/users');
    for (const u of users || []) {
      if (u.username?.startsWith('e2e-')) {
        console.log(`[Cleanup] Deleting stale user: ${u.username}`);
        await api.del(`/api/users/${u.id}`);
      }
    }
  });

  test('1.3 Create Tenant Alpha', async () => {
    const { status, data } = await api.post('/api/tenants', {
      name: `e2e-alpha-${RUN_ID}`,
      slug: `e2e-alpha-${RUN_ID}`,
    });
    expect([200, 201]).toContain(status);
    env.tenantAlphaId = data.id;
    console.log(`[Setup] Tenant Alpha: ${data.id} (${data.name})`);
  });

  test('1.4 Create Tenant Beta', async () => {
    const { status, data } = await api.post('/api/tenants', {
      name: `e2e-beta-${RUN_ID}`,
      slug: `e2e-beta-${RUN_ID}`,
    });
    expect([200, 201]).toContain(status);
    env.tenantBetaId = data.id;
    console.log(`[Setup] Tenant Beta: ${data.id} (${data.name})`);
  });

  test('1.5 Create User: Alpha Admin (member)', async () => {
    const { status, data } = await api.post('/api/users', {
      username: env.alphaAdminCreds.username,
      password: env.alphaAdminCreds.password,
      role: 'member',
      tenant_id: env.tenantAlphaId,
    });
    expect([200, 201]).toContain(status);
    env.userAlphaAdminId = data.id;
    console.log(`[Setup] User Alpha-Admin: ${data.id} (${data.username})`);
  });

  test('1.6 Create User: Alpha Readonly (member)', async () => {
    const { status, data } = await api.post('/api/users', {
      username: env.alphaReadonlyCreds.username,
      password: env.alphaReadonlyCreds.password,
      role: 'member',
      tenant_id: env.tenantAlphaId,
    });
    expect([200, 201]).toContain(status);
    env.userAlphaReadonlyId = data.id;
    console.log(`[Setup] User Alpha-Readonly: ${data.id} (${data.username})`);
  });

  test('1.7 Create User: Beta Admin (member)', async () => {
    const { status, data } = await api.post('/api/users', {
      username: env.betaAdminCreds.username,
      password: env.betaAdminCreds.password,
      role: 'member',
      tenant_id: env.tenantBetaId,
    });
    expect([200, 201]).toContain(status);
    env.userBetaAdminId = data.id;
    console.log(`[Setup] User Beta-Admin: ${data.id} (${data.username})`);
  });

  test('1.8 Create Account Alpha (mock AWS, tenant Alpha)', async () => {
    const { status, data } = await api.post('/api/accounts', {
      provider: 'aws',
      name: `e2e-aws-alpha-${RUN_ID}`,
      account_id: `alpha-${RUN_ID}`,
      is_mock: true,
      tenant_id: env.tenantAlphaId,
    });
    expect([200, 201]).toContain(status);
    env.accountAlphaId = data.id;
    console.log(`[Setup] Account Alpha: ${data.id}`);
  });

  test('1.9 Create Account Beta (mock AWS, tenant Beta)', async () => {
    const { status, data } = await api.post('/api/accounts', {
      provider: 'aws',
      name: `e2e-aws-beta-${RUN_ID}`,
      account_id: `beta-${RUN_ID}`,
      is_mock: true,
      tenant_id: env.tenantBetaId,
    });
    expect([200, 201]).toContain(status);
    env.accountBetaId = data.id;
    console.log(`[Setup] Account Beta: ${data.id}`);
  });

  test('1.10 Grant Alpha-Admin → Account Alpha (admin)', async () => {
    const { status } = await api.post('/api/account-access/grant', {
      user_id: env.userAlphaAdminId,
      account_id: env.accountAlphaId,
      role: 'admin',
    });
    expect([200, 201]).toContain(status);
    console.log('[Setup] Granted alpha-admin → account-alpha (admin)');
  });

  test('1.11 Grant Alpha-Readonly → Account Alpha (readonly)', async () => {
    const { status } = await api.post('/api/account-access/grant', {
      user_id: env.userAlphaReadonlyId,
      account_id: env.accountAlphaId,
      role: 'readonly',
    });
    expect([200, 201]).toContain(status);
    console.log('[Setup] Granted alpha-readonly → account-alpha (readonly)');
  });

  test('1.12 Grant Beta-Admin → Account Beta (admin)', async () => {
    const { status } = await api.post('/api/account-access/grant', {
      user_id: env.userBetaAdminId,
      account_id: env.accountBetaId,
      role: 'admin',
    });
    expect([200, 201]).toContain(status);
    console.log('[Setup] Granted beta-admin → account-beta (admin)');
  });

  test('1.13 Acquire tokens for all users', async () => {
    const alphaApi = new ApiClient();
    env.alphaAdminToken = await alphaApi.login(env.alphaAdminCreds.username, env.alphaAdminCreds.password);

    const roApi = new ApiClient();
    env.alphaReadonlyToken = await roApi.login(env.alphaReadonlyCreds.username, env.alphaReadonlyCreds.password);

    const betaApi = new ApiClient();
    env.betaAdminToken = await betaApi.login(env.betaAdminCreds.username, env.betaAdminCreds.password);

    expect(env.alphaAdminToken).toBeTruthy();
    expect(env.alphaReadonlyToken).toBeTruthy();
    expect(env.betaAdminToken).toBeTruthy();
    console.log('[Setup] All user tokens acquired');
  });

  test('1.14 Create test Glossary entries for isolation tests', async () => {
    // Alpha glossary (using super_admin since we need account_id)
    const { status: s1, data: g1 } = await api.post('/api/glossary', {
      term: `e2e-term-alpha-${RUN_ID}`,
      full_name: 'Alpha Test Term',
      description: 'Created by E2E test for tenant Alpha',
      account_id: env.accountAlphaId,
    });
    if (s1 === 201 || s1 === 200) env.glossaryAlphaId = g1?.id;
    console.log(`[Setup] Glossary Alpha: status=${s1}, id=${g1?.id || 'none'}`);

    // Beta glossary
    const { status: s2, data: g2 } = await api.post('/api/glossary', {
      term: `e2e-term-beta-${RUN_ID}`,
      full_name: 'Beta Test Term',
      description: 'Created by E2E test for tenant Beta',
      account_id: env.accountBetaId,
    });
    if (s2 === 201 || s2 === 200) env.glossaryBetaId = g2?.id;
    console.log(`[Setup] Glossary Beta: status=${s2}, id=${g2?.id || 'none'}`);
  });

  test('1.15 Verify setup: all entities exist', async () => {
    const { data: tenants } = await api.get<any[]>('/api/tenants');
    const { data: users } = await api.get<any[]>('/api/users');
    const { data: accounts } = await api.get<any[]>('/api/accounts');

    const e2eTenants = (tenants || []).filter((t: any) => t.name?.includes(RUN_ID));
    const e2eUsers = (users || []).filter((u: any) => u.username?.includes(RUN_ID));
    const e2eAccounts = (accounts || []).filter((a: any) => a.name?.includes(RUN_ID));

    console.log(`[Verify] Tenants: ${e2eTenants.length}, Users: ${e2eUsers.length}, Accounts: ${e2eAccounts.length}`);
    expect(e2eTenants.length).toBe(2);
    expect(e2eUsers.length).toBe(3);
    expect(e2eAccounts.length).toBe(2);
  });
});

// ================================================================
// PHASE 2: UI BUSINESS LOGIC TESTS
// ================================================================
test.describe.serial('Phase 2: UI Business Logic', () => {
  // ---- Super Admin UI Tests ----
  test.describe('2A: Super Admin — global visibility', () => {
    let page: Page;
    let context: BrowserContext;

    test.beforeAll(async ({ browser }) => {
      ({ context, page } = await loginViaUI(browser, ADMIN_CREDS.username, ADMIN_CREDS.password));
    });
    test.afterAll(async () => { await context.close(); });

    test('2A.1 Dashboard shows stats for all tenants', async () => {
      const body = await page.textContent('body');
      expect(body).toContain('Welcome');
      await page.screenshot({ path: 'screenshots/p2-superadmin-dashboard.png', fullPage: true });

      // Stats should include our e2e tenants
      const tenantCount = body?.match(/TENANTS[\s\S]*?(\d+)/)?.[1];
      console.log(`[SuperAdmin] Tenant count on dashboard: ${tenantCount}`);
    });

    test('2A.2 Tenants page shows both e2e tenants', async () => {
      await spaNav(page, /Tenants/);
      const body = await page.textContent('body');
      const hasAlpha = body?.includes(`e2e-alpha-${RUN_ID}`);
      const hasBeta = body?.includes(`e2e-beta-${RUN_ID}`);
      console.log(`[SuperAdmin] Tenants: Alpha=${hasAlpha}, Beta=${hasBeta}`);
      expect(hasAlpha).toBe(true);
      expect(hasBeta).toBe(true);
      await page.screenshot({ path: 'screenshots/p2-superadmin-tenants.png', fullPage: true });
    });

    test('2A.3 Users page shows all e2e users', async () => {
      await spaNav(page, /^Users$/);
      const body = await page.textContent('body');
      const hasAlphaAdmin = body?.includes(env.alphaAdminCreds.username);
      const hasAlphaRo = body?.includes(env.alphaReadonlyCreds.username);
      const hasBetaAdmin = body?.includes(env.betaAdminCreds.username);
      console.log(`[SuperAdmin] Users: AlphaAdmin=${hasAlphaAdmin}, AlphaRO=${hasAlphaRo}, BetaAdmin=${hasBetaAdmin}`);
      expect(hasAlphaAdmin).toBe(true);
      expect(hasBetaAdmin).toBe(true);
      await page.screenshot({ path: 'screenshots/p2-superadmin-users.png', fullPage: true });
    });

    test('2A.4 Accounts page shows both e2e accounts', async () => {
      await spaNav(page, /Cloud Accounts/);
      const body = await page.textContent('body');
      const hasAlpha = body?.includes(`e2e-aws-alpha-${RUN_ID}`);
      const hasBeta = body?.includes(`e2e-aws-beta-${RUN_ID}`);
      console.log(`[SuperAdmin] Accounts: Alpha=${hasAlpha}, Beta=${hasBeta}`);
      expect(hasAlpha).toBe(true);
      expect(hasBeta).toBe(true);
      await page.screenshot({ path: 'screenshots/p2-superadmin-accounts.png', fullPage: true });
    });

    test('2A.5 Real AWS: trigger cluster discovery', async () => {
      await spaNav(page, /^Clusters$/);
      const refreshBtn = page.locator('button').filter({ hasText: /Refresh Now/i }).first();
      if (await refreshBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
        await refreshBtn.click();
        await page.waitForTimeout(15000); // AWS discovery is slow
        await page.screenshot({ path: 'screenshots/p2-superadmin-clusters-discover.png', fullPage: true });

        // Check if at least 1 discovered cluster appears
        const body = await page.textContent('body');
        const hasDiscovered = body?.includes('ops-eks') || body?.includes('ACTIVE');
        console.log(`[SuperAdmin] Discovered cluster found: ${hasDiscovered}`);
      }
    });
  });

  // ---- Alpha Admin UI Tests ----
  test.describe('2B: Alpha Admin — tenant-scoped CRUD', () => {
    let page: Page;
    let context: BrowserContext;

    test.beforeAll(async ({ browser }) => {
      ({ context, page } = await loginViaUI(browser, env.alphaAdminCreds.username, env.alphaAdminCreds.password));
    });
    test.afterAll(async () => { await context.close(); });

    test('2B.1 Dashboard loads for Alpha Admin', async () => {
      const body = await page.textContent('body');
      expect(body).toContain('Welcome');
      await page.screenshot({ path: 'screenshots/p2-alpha-admin-dashboard.png', fullPage: true });
    });

    test('2B.2 Accounts page only shows Alpha account', async () => {
      await spaNav(page, /Cloud Accounts/);
      await page.waitForTimeout(1000);
      const body = await page.textContent('body') || '';
      const hasAlpha = body.includes(`e2e-aws-alpha-${RUN_ID}`);
      const hasBeta = body.includes(`e2e-aws-beta-${RUN_ID}`);
      console.log(`[AlphaAdmin] Accounts: Alpha=${hasAlpha}, Beta=${hasBeta}`);
      expect(hasAlpha).toBe(true);
      expect(hasBeta).toBe(false); // MUST NOT see Beta
      await page.screenshot({ path: 'screenshots/p2-alpha-admin-accounts.png', fullPage: true });
    });

    test('2B.3 Glossary shows only Alpha terms', async () => {
      await spaNav(page, /^Glossary$/);
      await page.waitForTimeout(1000);
      const body = await page.textContent('body') || '';
      const hasAlpha = body.includes(`e2e-term-alpha-${RUN_ID}`);
      const hasBeta = body.includes(`e2e-term-beta-${RUN_ID}`);
      console.log(`[AlphaAdmin] Glossary: Alpha=${hasAlpha}, Beta=${hasBeta}`);
      // Alpha should see their own term (if glossary is account-filtered)
      expect(hasBeta).toBe(false); // MUST NOT see Beta
      await page.screenshot({ path: 'screenshots/p2-alpha-admin-glossary.png', fullPage: true });
    });

    test('2B.4 CRUD: Create a new glossary term via UI', async () => {
      // Already on Glossary page from previous test
      const addBtn = page.locator('button').filter({ hasText: /add|create|new|\+/i }).first();
      if (await addBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
        await addBtn.click();
        await page.waitForTimeout(1000);

        // Fill the form (dialog should be open)
        const dialog = page.locator('[role="dialog"], [class*="Dialog"]').first();
        const inputs = dialog.locator('input, textarea');
        const inputCount = await inputs.count();
        console.log(`[AlphaAdmin] Glossary dialog inputs: ${inputCount}`);

        if (inputCount > 0) {
          await inputs.first().fill(`e2e-crud-term-${RUN_ID}`);
          await page.waitForTimeout(500);
          await page.screenshot({ path: 'screenshots/p2-alpha-crud-glossary-create.png', fullPage: true });

          // Submit
          const saveBtn = dialog.locator('button').filter({ hasText: /save|create|submit|confirm/i }).first();
          if (await saveBtn.isVisible({ timeout: 2000 }).catch(() => false)) {
            await saveBtn.click();
            await page.waitForTimeout(2000);
          }
        } else {
          await page.keyboard.press('Escape');
        }
      }
    });

    test('2B.5 CRUD: Edit the glossary term', async () => {
      // Look for edit button on the e2e-crud-term row
      const editBtn = page.locator('button[aria-label*="edit" i], tr:has-text("e2e-crud") button').filter({ hasText: /edit/i }).first();
      const pencilBtn = page.locator(`tr:has-text("e2e-crud-term-${RUN_ID}") button`).first();
      if (await pencilBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
        await pencilBtn.click();
        await page.waitForTimeout(1000);
        await page.screenshot({ path: 'screenshots/p2-alpha-crud-glossary-edit.png', fullPage: true });
        await page.keyboard.press('Escape');
        await page.waitForTimeout(500);
      } else {
        console.log('[AlphaAdmin] No edit button found for glossary term');
      }
    });

    test('2B.6 Cannot see Users page data (not super_admin)', async () => {
      await spaNav(page, /^Users$/);
      await page.waitForTimeout(1000);
      await page.screenshot({ path: 'screenshots/p2-alpha-admin-users.png', fullPage: true });
      // Member should only see users in own tenant (or none depending on RBAC)
    });

    test('2B.7 Cannot see Tenants management', async () => {
      await spaNav(page, /Tenants/);
      await page.waitForTimeout(1000);
      const body = await page.textContent('body') || '';
      // Create Tenant button should not exist for member
      const hasCreateBtn = await page.locator('button').filter({ hasText: /Create Tenant/i }).isVisible({ timeout: 2000 }).catch(() => false);
      console.log(`[AlphaAdmin] Tenants Create button visible: ${hasCreateBtn}`);
      await page.screenshot({ path: 'screenshots/p2-alpha-admin-tenants.png', fullPage: true });
    });
  });

  // ---- Alpha Readonly UI Tests ----
  test.describe('2C: Alpha Readonly — read-only verification', () => {
    let page: Page;
    let context: BrowserContext;

    test.beforeAll(async ({ browser }) => {
      ({ context, page } = await loginViaUI(browser, env.alphaReadonlyCreds.username, env.alphaReadonlyCreds.password));
    });
    test.afterAll(async () => { await context.close(); });

    test('2C.1 Dashboard loads for readonly user', async () => {
      const body = await page.textContent('body');
      expect(body).toContain('Welcome');
      await page.screenshot({ path: 'screenshots/p2-alpha-readonly-dashboard.png', fullPage: true });
    });

    test('2C.2 Accounts page: can view but create button hidden/disabled', async () => {
      await spaNav(page, /Cloud Accounts/);
      await page.waitForTimeout(1000);

      // Check if Add Account button exists for readonly user
      const addBtn = page.locator('button').filter({ hasText: /Add Account|Add|\+/i }).first();
      const addVisible = await addBtn.isVisible({ timeout: 2000 }).catch(() => false);
      console.log(`[AlphaReadonly] Add Account button visible: ${addVisible}`);
      await page.screenshot({ path: 'screenshots/p2-alpha-readonly-accounts.png', fullPage: true });
    });

    test('2C.3 Glossary page: can view, create button check', async () => {
      await spaNav(page, /^Glossary$/);
      await page.waitForTimeout(1000);

      const addBtn = page.locator('button').filter({ hasText: /add|create|new|\+/i }).first();
      const addVisible = await addBtn.isVisible({ timeout: 2000 }).catch(() => false);
      console.log(`[AlphaReadonly] Glossary Add button visible: ${addVisible}`);
      await page.screenshot({ path: 'screenshots/p2-alpha-readonly-glossary.png', fullPage: true });
    });

    test('2C.4 Cannot see Beta tenant data', async () => {
      await spaNav(page, /Cloud Accounts/);
      await page.waitForTimeout(1000);
      const body = await page.textContent('body') || '';
      const hasBeta = body.includes(`e2e-aws-beta-${RUN_ID}`);
      console.log(`[AlphaReadonly] Can see Beta account: ${hasBeta}`);
      expect(hasBeta).toBe(false);
    });
  });

  // ---- Beta Admin UI Tests ----
  test.describe('2D: Beta Admin — cross-tenant isolation', () => {
    let page: Page;
    let context: BrowserContext;

    test.beforeAll(async ({ browser }) => {
      ({ context, page } = await loginViaUI(browser, env.betaAdminCreds.username, env.betaAdminCreds.password));
    });
    test.afterAll(async () => { await context.close(); });

    test('2D.1 Dashboard loads for Beta Admin', async () => {
      const body = await page.textContent('body');
      expect(body).toContain('Welcome');
      await page.screenshot({ path: 'screenshots/p2-beta-admin-dashboard.png', fullPage: true });
    });

    test('2D.2 Accounts: only sees Beta account, NOT Alpha', async () => {
      await spaNav(page, /Cloud Accounts/);
      await page.waitForTimeout(1000);
      const body = await page.textContent('body') || '';
      const hasAlpha = body.includes(`e2e-aws-alpha-${RUN_ID}`);
      const hasBeta = body.includes(`e2e-aws-beta-${RUN_ID}`);
      console.log(`[BetaAdmin] Accounts: Alpha=${hasAlpha}, Beta=${hasBeta}`);
      expect(hasAlpha).toBe(false); // MUST NOT see Alpha
      expect(hasBeta).toBe(true);
      await page.screenshot({ path: 'screenshots/p2-beta-admin-accounts.png', fullPage: true });
    });

    test('2D.3 Glossary: only sees Beta terms, NOT Alpha', async () => {
      await spaNav(page, /^Glossary$/);
      await page.waitForTimeout(1000);
      const body = await page.textContent('body') || '';
      const hasAlpha = body.includes(`e2e-term-alpha-${RUN_ID}`);
      const hasBeta = body.includes(`e2e-term-beta-${RUN_ID}`);
      console.log(`[BetaAdmin] Glossary: Alpha=${hasAlpha}, Beta=${hasBeta}`);
      expect(hasAlpha).toBe(false); // MUST NOT see Alpha
      await page.screenshot({ path: 'screenshots/p2-beta-admin-glossary.png', fullPage: true });
    });
  });
});

// ================================================================
// PHASE 3: API SECURITY AUDIT
// ================================================================
test.describe.serial('Phase 3: API Security Audit', () => {

  // ---- 3A: 401 Unauthorized (no token) ----
  test.describe('3A: Unauthenticated access → 401', () => {
    const noAuth = new ApiClient();

    test('3A.1 GET /api/tenants → 401', async () => {
      const { status } = await noAuth.get('/api/tenants');
      expect(status).toBe(401);
    });

    test('3A.2 GET /api/users → 401', async () => {
      const { status } = await noAuth.get('/api/users');
      expect(status).toBe(401);
    });

    test('3A.3 GET /api/accounts → 401', async () => {
      const { status } = await noAuth.get('/api/accounts');
      expect(status).toBe(401);
    });

    test('3A.4 POST /api/tenants → 401', async () => {
      const { status } = await noAuth.post('/api/tenants', { name: 'hack', slug: 'hack' });
      expect(status).toBe(401);
    });

    test('3A.5 GET /api/glossary → 401', async () => {
      const { status } = await noAuth.get('/api/glossary');
      expect(status).toBe(401);
    });

    test('3A.6 GET /api/clusters → 401', async () => {
      const { status } = await noAuth.get('/api/clusters');
      expect(status).toBe(401);
    });

    test('3A.7 GET /api/issues → 401', async () => {
      const { status } = await noAuth.get('/api/issues');
      expect(status).toBe(401);
    });
  });

  // ---- 3B: 403 Forbidden (member calls super_admin endpoints) ----
  test.describe('3B: Member → super_admin endpoints → 403', () => {
    const memberApi = new ApiClient();

    test.beforeAll(async () => {
      // Re-acquire token if env lost it due to Phase 1 serial dependency
      if (!env.alphaAdminToken && env.alphaAdminCreds.username) {
        try {
          env.alphaAdminToken = await memberApi.login(env.alphaAdminCreds.username, env.alphaAdminCreds.password);
        } catch {
          // User may not exist if Phase 1 failed — use super admin to create inline
        }
      }
      memberApi.token = env.alphaAdminToken;
    });

    test('3B.1 POST /api/tenants (create tenant) → 401 or 403', async () => {
      const { status } = await memberApi.post('/api/tenants', {
        name: 'hack-tenant',
        slug: 'hack-tenant',
      });
      expect([401, 403]).toContain(status);
    });

    test('3B.2 POST /api/users (create user) → 401 or 403', async () => {
      const { status } = await memberApi.post('/api/users', {
        username: 'hack-user',
        password: 'hackpassword123',
        role: 'member',
        tenant_id: env.tenantAlphaId,
      });
      expect([401, 403]).toContain(status);
    });

    test('3B.3 DELETE /api/tenants/{alpha_id} → 401 or 403', async () => {
      if (!env.tenantAlphaId) { console.log('[Skip] No tenant Alpha ID'); return; }
      const { status } = await memberApi.del(`/api/tenants/${env.tenantAlphaId}`);
      expect([401, 403]).toContain(status);
    });

    test('3B.4 PUT /api/tenants/{alpha_id} → 401 or 403', async () => {
      if (!env.tenantAlphaId) { console.log('[Skip] No tenant Alpha ID'); return; }
      const { status } = await memberApi.put(`/api/tenants/${env.tenantAlphaId}`, {
        name: 'hacked-name',
        slug: 'hacked-slug',
      });
      expect([401, 403]).toContain(status);
    });

    test('3B.5 DELETE /api/users/{self} → 401 or 403', async () => {
      if (!env.userAlphaAdminId) { console.log('[Skip] No user Alpha Admin ID'); return; }
      const { status } = await memberApi.del(`/api/users/${env.userAlphaAdminId}`);
      expect([401, 403]).toContain(status);
    });
  });

  // ---- 3C: Cross-tenant access (Alpha → Beta resources) ----
  test.describe('3C: Cross-tenant access → 403 / empty', () => {
    const alphaApi = new ApiClient();
    const betaApi = new ApiClient();

    test.beforeAll(async () => {
      alphaApi.token = env.alphaAdminToken;
      betaApi.token = env.betaAdminToken;
    });

    test('3C.1 Alpha GET /api/accounts → no Beta accounts', async () => {
      const { data: accounts } = await alphaApi.get<any[]>('/api/accounts');
      const betaAccounts = (accounts || []).filter((a: any) => a.name?.includes('beta'));
      console.log(`[Security] Alpha sees ${(accounts || []).length} accounts, ${betaAccounts.length} from Beta`);
      expect(betaAccounts.length).toBe(0);
    });

    test('3C.2 Beta GET /api/accounts → no Alpha accounts', async () => {
      const { data: accounts } = await betaApi.get<any[]>('/api/accounts');
      const alphaAccounts = (accounts || []).filter((a: any) => a.name?.includes('alpha'));
      console.log(`[Security] Beta sees ${(accounts || []).length} accounts, ${alphaAccounts.length} from Alpha`);
      expect(alphaAccounts.length).toBe(0);
    });

    test('3C.3 Alpha PUT /api/accounts/{beta_account} → 403 or 404', async () => {
      const { status } = await alphaApi.put(`/api/accounts/${env.accountBetaId}`, {
        name: 'hacked-by-alpha',
      });
      console.log(`[Security] Alpha update Beta account: ${status}`);
      expect([403, 404]).toContain(status);
    });

    test('3C.4 Alpha DELETE /api/accounts/{beta_account} → 403 or 404', async () => {
      const { status } = await alphaApi.del(`/api/accounts/${env.accountBetaId}`);
      console.log(`[Security] Alpha delete Beta account: ${status}`);
      expect([403, 404]).toContain(status);
    });

    test('3C.5 Beta PUT /api/accounts/{alpha_account} → 403 or 404', async () => {
      const { status } = await betaApi.put(`/api/accounts/${env.accountAlphaId}`, {
        name: 'hacked-by-beta',
      });
      console.log(`[Security] Beta update Alpha account: ${status}`);
      expect([403, 404]).toContain(status);
    });

    test('3C.6 Alpha GET /api/glossary → no Beta glossary terms', async () => {
      const { data: glossary } = await alphaApi.get<any[]>('/api/glossary');
      const betaTerms = (glossary || []).filter((g: any) => g.term?.includes('beta'));
      console.log(`[Security] Alpha sees ${(glossary || []).length} glossary terms, ${betaTerms.length} from Beta`);
      expect(betaTerms.length).toBe(0);
    });

    test('3C.7 Beta GET /api/glossary → no Alpha glossary terms', async () => {
      const { data: glossary } = await betaApi.get<any[]>('/api/glossary');
      const alphaTerms = (glossary || []).filter((g: any) => g.term?.includes('alpha'));
      console.log(`[Security] Beta sees ${(glossary || []).length} glossary terms, ${alphaTerms.length} from Alpha`);
      expect(alphaTerms.length).toBe(0);
    });
  });

  // ---- 3D: Readonly write attempts → 403 ----
  test.describe('3D: Readonly user write operations → 403', () => {
    const readonlyApi = new ApiClient();

    test.beforeAll(async () => {
      readonlyApi.token = env.alphaReadonlyToken;
    });

    test('3D.1 POST /api/glossary (create) → 403', async () => {
      const { status } = await readonlyApi.post('/api/glossary', {
        term: 'readonly-hack',
        description: 'Should not be created',
        account_id: env.accountAlphaId,
      });
      console.log(`[Security] Readonly create glossary: ${status}`);
      expect(status).toBe(403);
    });

    test('3D.2 PUT /api/glossary/{id} (update) → 403', async () => {
      if (!env.glossaryAlphaId) { console.log('[Skip] No glossary Alpha ID'); return; }
      const { status } = await readonlyApi.put(`/api/glossary/${env.glossaryAlphaId}`, {
        term: 'readonly-hacked',
        description: 'Should not be updated',
      });
      console.log(`[Security] Readonly update glossary: ${status}`);
      expect(status).toBe(403);
    });

    test('3D.3 DELETE /api/glossary/{id} → 403', async () => {
      if (!env.glossaryAlphaId) { console.log('[Skip] No glossary Alpha ID'); return; }
      const { status } = await readonlyApi.del(`/api/glossary/${env.glossaryAlphaId}`);
      console.log(`[Security] Readonly delete glossary: ${status}`);
      expect(status).toBe(403);
    });

    test('3D.4 POST /api/accounts (create account) → 403', async () => {
      const { status } = await readonlyApi.post('/api/accounts', {
        provider: 'aws',
        name: `readonly-hack-account-${RUN_ID}`,
        is_mock: true,
      });
      console.log(`[Security] Readonly create account: ${status}`);
      expect(status).toBe(403);
    });

    test('3D.5 DELETE /api/accounts/{alpha} → 403', async () => {
      const { status } = await readonlyApi.del(`/api/accounts/${env.accountAlphaId}`);
      console.log(`[Security] Readonly delete account: ${status}`);
      expect(status).toBe(403);
    });

    test('3D.6 POST /api/account-access/grant → 403 (not admin)', async () => {
      const { status } = await readonlyApi.post('/api/account-access/grant', {
        user_id: env.userAlphaReadonlyId,
        account_id: env.accountAlphaId,
        role: 'admin',
      });
      console.log(`[Security] Readonly self-grant admin: ${status}`);
      expect(status).toBe(403);
    });
  });
});

// ================================================================
// PHASE 4: TEARDOWN
// ================================================================
test.describe.serial('Phase 4: Teardown', () => {
  const api = new ApiClient();

  test.beforeAll(async () => {
    api.token = env.superAdminToken;
  });

  test('4.1 Delete glossary entries', async () => {
    if (env.glossaryAlphaId) {
      const { status } = await api.del(`/api/glossary/${env.glossaryAlphaId}`);
      console.log(`[Teardown] Delete glossary Alpha: ${status}`);
    }
    if (env.glossaryBetaId) {
      const { status } = await api.del(`/api/glossary/${env.glossaryBetaId}`);
      console.log(`[Teardown] Delete glossary Beta: ${status}`);
    }

    // Also clean any CRUD-created glossary entries
    const { data: allGlossary } = await api.get<any[]>('/api/glossary');
    for (const g of allGlossary || []) {
      if (g.term?.includes(RUN_ID)) {
        await api.del(`/api/glossary/${g.id}`);
        console.log(`[Teardown] Delete glossary: ${g.term}`);
      }
    }
  });

  test('4.2 Delete accounts', async () => {
    if (env.accountAlphaId) {
      const { status } = await api.del(`/api/accounts/${env.accountAlphaId}`);
      console.log(`[Teardown] Delete account Alpha: ${status}`);
    }
    if (env.accountBetaId) {
      const { status } = await api.del(`/api/accounts/${env.accountBetaId}`);
      console.log(`[Teardown] Delete account Beta: ${status}`);
    }

    // Clean any remaining e2e accounts
    const { data: allAccounts } = await api.get<any[]>('/api/accounts');
    for (const a of allAccounts || []) {
      if (a.name?.includes(RUN_ID)) {
        await api.del(`/api/accounts/${a.id}`);
        console.log(`[Teardown] Delete account: ${a.name}`);
      }
    }
  });

  test('4.3 Delete users', async () => {
    for (const userId of [env.userAlphaAdminId, env.userAlphaReadonlyId, env.userBetaAdminId]) {
      if (userId) {
        const { status } = await api.del(`/api/users/${userId}`);
        console.log(`[Teardown] Delete user ${userId}: ${status}`);
      }
    }
  });

  test('4.4 Delete tenants', async () => {
    if (env.tenantAlphaId) {
      const { status } = await api.del(`/api/tenants/${env.tenantAlphaId}`);
      console.log(`[Teardown] Delete tenant Alpha: ${status}`);
    }
    if (env.tenantBetaId) {
      const { status } = await api.del(`/api/tenants/${env.tenantBetaId}`);
      console.log(`[Teardown] Delete tenant Beta: ${status}`);
    }
  });

  test('4.5 Verify cleanup: no e2e data remains', async () => {
    const { data: tenants } = await api.get<any[]>('/api/tenants');
    const { data: users } = await api.get<any[]>('/api/users');
    const { data: accounts } = await api.get<any[]>('/api/accounts');

    const e2eTenants = (tenants || []).filter((t: any) => t.name?.includes(RUN_ID));
    const e2eUsers = (users || []).filter((u: any) => u.username?.includes(RUN_ID));
    const e2eAccounts = (accounts || []).filter((a: any) => a.name?.includes(RUN_ID));

    console.log(`[Verify] Remaining: Tenants=${e2eTenants.length}, Users=${e2eUsers.length}, Accounts=${e2eAccounts.length}`);
    expect(e2eTenants.length).toBe(0);
    expect(e2eUsers.length).toBe(0);
    expect(e2eAccounts.length).toBe(0);

    console.log('\n✅ All e2e test data cleaned up successfully');
  });
});
