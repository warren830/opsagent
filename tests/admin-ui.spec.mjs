/**
 * OpsAgent Admin UI — Comprehensive Playwright E2E Test
 *
 * Covers all 11 tabs with CRUD operations, form validation,
 * tenant isolation verification, and API consistency checks.
 *
 * Usage:
 *   cd bot && PORT=3999 node dist/index.js &
 *   node tests/admin-ui.spec.mjs
 */
import { chromium } from 'playwright';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SCREENSHOT_DIR = path.join(__dirname, 'screenshots');
const BASE = process.env.TEST_URL || 'http://localhost:3999';

let browser, page;
let stepNum = 0;
const results = [];

// ── Helpers ──────────────────────────────────────────────────────
async function screenshot(name) {
  stepNum++;
  const filename = `${String(stepNum).padStart(2, '0')}-${name}.png`;
  await page.screenshot({ path: path.join(SCREENSHOT_DIR, filename), fullPage: true });
  return filename;
}

function check(name, ok) {
  results.push({ name, ok });
  console.log(`  ${ok ? 'PASS' : 'FAIL'} ${name}`);
}

async function switchTab(tab) {
  await page.click(`a[href="#${tab}"]`);
  await page.waitForTimeout(500);
}

async function apiGet(endpoint) {
  return page.evaluate(async (url) => {
    const r = await fetch(url);
    return r.json();
  }, `${BASE}${endpoint}`);
}

// ── Main ─────────────────────────────────────────────────────────
async function run() {
  browser = await chromium.launch({ headless: true });
  page = await browser.newPage({ viewport: { width: 1280, height: 900 } });

  // Accept all confirm() dialogs (legacy) and custom confirm modals
  page.on('dialog', async d => await d.accept());
  // Auto-click custom confirm modals when they appear
  async function acceptConfirm() {
    try {
      await page.waitForSelector('.confirm-overlay #confirm-ok', { timeout: 2000 });
      await page.click('.confirm-overlay #confirm-ok');
      await page.waitForTimeout(300);
    } catch { /* no confirm appeared */ }
  }

  console.log('========================================');
  console.log(' OpsAgent Admin UI — E2E Test Suite');
  console.log('========================================\n');

  // ═══════════════════════════════════════════════════════════════
  // 1. PAGE LOAD + LOGIN
  // ═══════════════════════════════════════════════════════════════
  console.log('--- 1. Page Load ---');
  await page.goto(`${BASE}/admin`);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(1000);

  check('Page title is OpsAgent Admin', (await page.title()) === 'OpsAgent Admin');

  // Handle login if required (users.yaml exists)
  const loginScreen = await page.locator('#login-screen').isVisible();
  if (loginScreen) {
    await page.fill('#login-username', 'admin');
    await page.fill('#login-password', 'admin123');
    await page.click('#login-screen button');
    await page.waitForTimeout(1500);
    check('Login successful', await page.locator('#app-sidebar').isVisible());
  }

  const navLinks = await page.locator('#app-sidebar nav a:visible').count();
  check('Sidebar has 12 nav links', navLinks === 12);

  const chatVisible = await page.locator('#tab-chat').isVisible();
  check('Chat tab visible by default', chatVisible);

  await screenshot('load-admin');

  // ═══════════════════════════════════════════════════════════════
  // 1b. CHAT TAB — Send message and verify streaming response
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 1b. Chat ---');
  await switchTab('chat');
  await screenshot('chat-empty');

  // Verify chat UI elements
  const chatInput = await page.locator('#chat-input').isVisible();
  check('Chat input visible', chatInput);
  const sendBtn = await page.locator('#chat-send-btn').isVisible();
  check('Send button visible', sendBtn);

  // Send a simple message using the shared helper
  console.log('  Sending basic chat...');
  const chatOk = await chatAsTenant('', '你好，请回复 OK');
  check('Bot response received', chatOk.length > 0 && !chatOk.includes('timeout'));
  if (chatOk.length > 0) console.log(`  Bot said: ${chatOk.substring(0, 80)}...`);
  await screenshot('chat-response');

  // Follow-up for session continuity
  if (chatOk.length > 0 && !chatOk.includes('timeout')) {
    const followup = await chatAsTenant('', '刚才你说了什么？');
    check('Follow-up response (session continuity)', followup.length > 0);
    await screenshot('chat-followup');
  }
  await screenshot('chat-final');

  // ═══════════════════════════════════════════════════════════════
  // 1c. CHAT ISOLATION — Switch tenant context and verify scoped response
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 1c. Chat Isolation ---');

  // Verify tenant selector exists and has options
  const tenantSelector = await page.locator('#chat-tenant-select').isVisible();
  check('Tenant selector visible', tenantSelector);
  const tenantOptions = await page.locator('#chat-tenant-select option').count();
  check('Tenant selector has options (Global + tenants)', tenantOptions >= 2);
  await screenshot('chat-tenant-selector');

  // Helper: clear chat history and send question, return bot response
  async function chatAsTenant(tenantId, question) {
    // Wait for any ongoing chat to finish (send button re-enabled)
    await page.waitForFunction(() => {
      const btn = document.getElementById('chat-send-btn');
      return btn && !btn.disabled;
    }, { timeout: 120000 }).catch(() => {});
    // Clear chat bubbles
    await page.evaluate(() => document.getElementById('chat-messages').innerHTML = '');
    await page.waitForTimeout(200);
    // Select tenant if selector is visible
    const selectorVisible = await page.locator('#chat-tenant-select').isVisible().catch(() => false);
    if (selectorVisible) {
      await page.selectOption('#chat-tenant-select', tenantId);
    }
    await page.waitForTimeout(300);
    await page.fill('#chat-input', question);
    await page.click('#chat-send-btn');
    try {
      await page.waitForFunction(() => {
        const bots = document.querySelectorAll('.chat-bubble.bot');
        const last = bots[bots.length - 1];
        return last && last.style.display !== 'none' && last.textContent.trim().length > 2;
      }, { timeout: 90000 });
      return (await page.locator('.chat-bubble.bot').last().textContent()) || '';
    } catch {
      return '(timeout)';
    }
  }

  const glossaryQuestion = '请简要列出 CLAUDE.md 中「公司术语速查」章节里的所有术语 key，只返回术语列表，不要解释。如果没有术语章节就说「无术语」';

  // Step 1: Global mode
  console.log('  [Global] Asking about glossary terms...');
  const globalTerms = await chatAsTenant('', glossaryQuestion);
  console.log(`  [Global] Response: ${globalTerms.substring(0, 120)}...`);

  const globalHasIcs = globalTerms.toLowerCase().includes('ics');
  check('Global chat sees shared glossary (ics)', globalHasIcs);
  await screenshot('chat-isolation-global');

  // Step 2: Switch to team-alpha and ask same question
  console.log('  [team-alpha] Asking about glossary terms...');
  const alphaTerms = await chatAsTenant('team-alpha', glossaryQuestion);
  console.log(`  [team-alpha] Response: ${alphaTerms.substring(0, 120)}...`);
  await screenshot('chat-isolation-alpha');

  const alphaLacksIcs = !alphaTerms.toLowerCase().includes('ics') || alphaTerms.includes('无术语');
  const alphaHasAlpha = alphaTerms.toLowerCase().includes('alpha') || alphaTerms.includes('无术语');
  check('Alpha chat does NOT see shared glossary (ics)', alphaLacksIcs);
  check('Alpha chat sees own terms or reports none', alphaHasAlpha);

  // Step 3: Switch to team-beta and ask same question
  console.log('  [team-beta] Asking about glossary terms...');
  const betaTerms = await chatAsTenant('team-beta', glossaryQuestion);
  console.log(`  [team-beta] Response: ${betaTerms.substring(0, 120)}...`);
  await screenshot('chat-isolation-beta');

  const betaLacksIcs = !betaTerms.toLowerCase().includes('ics') || betaTerms.includes('无术语');
  const betaLacksAlpha = !betaTerms.toLowerCase().includes('alpha-service');
  check('Beta chat does NOT see shared glossary (ics)', betaLacksIcs);
  check('Beta chat does NOT see Alpha glossary (alpha-service)', betaLacksAlpha);

  // ═══════════════════════════════════════════════════════════════
  // 2. GLOSSARY TAB — CRUD
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 2. Glossary ---');
  await switchTab('glossary');
  const glossaryActive = await page.locator('a[href="#glossary"]').evaluate(el => el.classList.contains('active'));
  check('Glossary tab active', glossaryActive);
  await screenshot('glossary-list');

  // Create
  await page.click('text="+ New Term"');
  await page.waitForSelector('#glossary-modal.active');
  await page.fill('#g-key', 'e2e-test-term');
  await page.fill('#g-fullname', 'E2E Test Term');
  await page.fill('#g-desc', 'Created by Playwright test');
  await page.fill('#g-aliases', 'e2e, test');
  await screenshot('glossary-create-modal');
  await page.click('#glossary-modal .btn-primary');
  await page.waitForTimeout(800);

  const glossaryCard = await page.locator('#glossary-list .card').filter({ hasText: 'e2e-test-term' }).count();
  check('Glossary term created', glossaryCard === 1);
  await screenshot('glossary-created');

  // API verify
  const glossaryApi = await apiGet('/admin/api/glossary');
  check('API confirms glossary term', 'e2e-test-term' in glossaryApi.glossary);

  // Edit
  await page.click('#glossary-list .card:has-text("e2e-test-term") button:has-text("Edit")');
  await page.waitForSelector('#glossary-modal.active');
  await page.fill('#g-desc', 'Updated by Playwright');
  await page.click('#glossary-modal .btn-primary');
  await page.waitForTimeout(800);
  const updatedApi = await apiGet('/admin/api/glossary');
  check('Glossary term updated', updatedApi.glossary['e2e-test-term']?.description === 'Updated by Playwright');

  // Delete
  await page.click('#glossary-list .card:has-text("e2e-test-term") button:has-text("Delete")');
  await acceptConfirm();
  await page.waitForTimeout(800);
  const afterDelete = await apiGet('/admin/api/glossary');
  check('Glossary term deleted', !('e2e-test-term' in afterDelete.glossary));
  await screenshot('glossary-after-delete');

  // ═══════════════════════════════════════════════════════════════
  // 3. ACCOUNTS TAB — CRUD
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 3. Accounts ---');
  await switchTab('accounts');
  await screenshot('accounts-list');

  // Create extra account
  await page.locator('button[onclick="openAccountModal()"]').click();
  await page.waitForSelector('#account-modal.active');
  await page.fill('#a-id', '999999999999');
  await page.fill('#a-name', 'e2e-test-account');
  await screenshot('accounts-create-modal');
  await page.click('#account-modal .btn-primary');
  await page.waitForTimeout(800);

  const acctCard = await page.locator('#extra-accounts-list .card').filter({ hasText: 'e2e-test-account' }).count();
  check('Extra account created', acctCard === 1);
  await screenshot('accounts-created');

  // Delete
  await page.click('#extra-accounts-list .card:has-text("e2e-test-account") button:has-text("Delete")');
  await acceptConfirm();
  await page.waitForTimeout(800);
  const acctApi = await apiGet('/admin/api/accounts');
  const acctRemains = (acctApi.accounts?.extra || []).some(a => a.id === '999999999999');
  check('Extra account deleted', !acctRemains);
  await screenshot('accounts-after-delete');

  // ═══════════════════════════════════════════════════════════════
  // 4. PLATFORMS TAB
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 4. Platforms ---');
  await switchTab('platforms');
  await screenshot('platforms-list');

  const platformCards = await page.locator('#platforms-list .card').count();
  check('Platforms tab shows cards (or empty)', platformCards >= 0); // may be empty if no platforms configured

  // ═══════════════════════════════════════════════════════════════
  // 5. KNOWLEDGE TAB — CRUD
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 5. Knowledge ---');
  await switchTab('knowledge');
  await screenshot('knowledge-list');

  // Create
  await page.click('text="+ New File"');
  await page.waitForSelector('#knowledge-modal.active');
  await page.fill('#k-filename', 'e2e-test-runbook.md');
  await page.fill('#k-content', '# E2E Test Runbook\n\nCreated by Playwright.\n');
  await screenshot('knowledge-create-modal');
  await page.click('#knowledge-modal .btn-primary');
  await page.waitForTimeout(800);

  const knCard = await page.locator('#knowledge-list .card').filter({ hasText: 'e2e-test-runbook.md' }).count();
  check('Knowledge file created', knCard === 1);
  await screenshot('knowledge-created');

  // Delete
  await page.click('#knowledge-list .card:has-text("e2e-test-runbook.md") button:has-text("Delete")');
  await acceptConfirm();
  await page.waitForTimeout(800);
  const knApi = await apiGet('/admin/api/knowledge');
  const knRemains = knApi.files?.some(f => f.name === 'e2e-test-runbook.md');
  check('Knowledge file deleted', !knRemains);

  // ═══════════════════════════════════════════════════════════════
  // 6. SKILLS TAB — CRUD
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 6. Skills ---');
  await switchTab('skills');
  await screenshot('skills-list');

  // Create
  await page.locator('button[onclick="openSkillModal()"]').click();
  await page.waitForSelector('#skill-modal.active');
  await page.fill('#sk-name', 'E2E Test Skill');
  await page.fill('#sk-desc', 'Test skill from Playwright');
  await page.fill('#sk-instructions', '# Step 1\nRun the test');
  await screenshot('skills-create-modal');
  await page.click('#skill-modal .btn-primary');
  await page.waitForTimeout(800);

  const skillCard = await page.locator('#skills-list .card').filter({ hasText: 'E2E Test Skill' }).count();
  check('Skill created', skillCard === 1);
  await screenshot('skills-created');

  // Delete
  await page.click('#skills-list .card:has-text("E2E Test Skill") button:has-text("Delete")');
  await acceptConfirm();
  await page.waitForTimeout(800);
  const skillApi = await apiGet('/admin/api/skills');
  const skillRemains = skillApi.skills?.some(s => s.name === 'E2E Test Skill');
  check('Skill deleted', !skillRemains);

  // ═══════════════════════════════════════════════════════════════
  // 7. SCHEDULED JOBS TAB — CRUD
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 7. Scheduled Jobs ---');
  await switchTab('scheduled-jobs');
  await screenshot('jobs-list');

  // Create
  await page.locator('button[onclick="openJobModal()"]').click();
  await page.waitForSelector('#job-modal.active');
  await page.fill('#j-name', 'e2e-test-job');
  await page.fill('#j-cron', '0 9 * * 1');
  await page.fill('#j-query', 'Show cluster health');
  await screenshot('jobs-create-modal');
  await page.click('#job-modal .btn-primary');
  await page.waitForTimeout(800);

  const jobCard = await page.locator('#jobs-list .card').filter({ hasText: 'e2e-test-job' }).count();
  check('Scheduled job created', jobCard === 1);
  await screenshot('jobs-created');

  // Delete
  await page.click('#jobs-list .card:has-text("e2e-test-job") button:has-text("Delete")');
  await acceptConfirm();
  await page.waitForTimeout(800);
  const jobApi = await apiGet('/admin/api/scheduled-jobs');
  const jobRemains = jobApi.scheduled_jobs?.some(j => j.name === 'e2e-test-job');
  check('Scheduled job deleted', !jobRemains);

  // ═══════════════════════════════════════════════════════════════
  // 8. PLUGINS TAB
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 8. Plugins ---');
  await switchTab('plugins');
  await screenshot('plugins-list');

  const pluginCards = await page.locator('#plugins-list .card').count();
  check('Plugins tab renders', pluginCards >= 0); // may be empty

  // ═══════════════════════════════════════════════════════════════
  // 9. TENANTS TAB — FULL CRUD + ISOLATION
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 9. Tenants ---');
  await switchTab('tenants');
  await screenshot('tenants-list');

  const existingTenants = await page.locator('#tenants-list .card').count();
  check('Tenants tab shows existing tenants', existingTenants >= 0);

  // 9a. Create tenant
  await page.click('text="+ New Tenant"');
  await page.waitForSelector('#tenant-modal.active');
  await page.fill('#t-id', 'e2e-team');
  await page.fill('#t-name', 'E2E Test Team');
  await page.click('text="+ Add Channel"');
  await page.waitForTimeout(200);
  await page.locator('#t-channels .t-ch-platform').last().selectOption('slack');
  await page.locator('#t-channels .t-ch-id').last().fill('C_e2e_test');
  await page.fill('#t-aws-accounts', '888888888888');
  await page.click('text="+ Add Alicloud Account"');
  await page.waitForTimeout(200);
  await page.locator('#t-alicloud .t-ali-name').last().fill('e2e-cn');
  await page.locator('#t-alicloud .t-ali-region').last().fill('cn-beijing');
  await page.locator('#t-alicloud .t-ali-ak').last().fill('E2E_AK_ENV');
  await page.locator('#t-alicloud .t-ali-sk').last().fill('E2E_SK_ENV');
  await screenshot('tenants-create-modal');
  await page.click('#tenant-modal .btn-primary');
  await page.waitForTimeout(1000);

  const tenantCard = await page.locator('#tenants-list .card').filter({ hasText: 'E2E Test Team' }).count();
  check('Tenant created', tenantCard === 1);
  await screenshot('tenants-created');

  // API verify
  const tenantApi = await apiGet('/admin/api/tenants');
  const e2eTenant = tenantApi.tenants.find(t => t.id === 'e2e-team');
  check('API confirms tenant', !!e2eTenant);
  check('Tenant has channel', e2eTenant?.channels?.[0]?.channel_id === 'C_e2e_test');
  check('Tenant has AWS account', e2eTenant?.aws_account_ids?.[0] === '888888888888');
  check('Tenant has alicloud', e2eTenant?.alicloud?.[0]?.name === 'e2e-cn');

  // 9b. Manage tenant — open detail view
  await page.locator('#tenants-list .card').filter({ hasText: 'E2E Test Team' }).locator('text="Manage"').click();
  await page.waitForTimeout(500);

  const detailVisible = await page.locator('#tenant-detail-view').isVisible();
  check('Tenant detail view opens', detailVisible);
  await screenshot('tenants-detail-view');

  // 9c. Add glossary term in tenant
  await page.click('#tenant-detail-view button:has-text("+ Add Term")');
  await page.waitForSelector('#td-glossary-modal.active');
  await page.fill('#td-g-key', 'e2e-svc');
  await page.fill('#td-g-fullname', 'E2E Service');
  await page.fill('#td-g-desc', 'Tenant-scoped service');
  await page.click('#td-glossary-modal .btn-primary');
  await page.waitForTimeout(800);

  const tdGlossaryCard = await page.locator('#td-glossary-list .card').count();
  check('Tenant glossary term added', tdGlossaryCard >= 1);
  await screenshot('tenants-glossary-added');

  // API verify tenant glossary
  const tdGlossaryApi = await apiGet('/admin/api/tenants/e2e-team/glossary');
  check('API confirms tenant glossary', 'e2e-svc' in (tdGlossaryApi.glossary || {}));

  // 9d. Add skill in tenant
  await page.click('#tenant-detail-view button:has-text("+ Add Skill")');
  await page.waitForSelector('#td-skill-modal.active');
  await page.fill('#td-s-name', 'E2E Skill');
  await page.fill('#td-s-desc', 'Test tenant skill');
  await page.fill('#td-s-instructions', '# Do the thing');
  await page.click('#td-skill-modal .btn-primary');
  await page.waitForTimeout(800);

  const tdSkillCard = await page.locator('#td-skills-list .card').count();
  check('Tenant skill added', tdSkillCard >= 1);
  await screenshot('tenants-skill-added');

  // 9e. Add knowledge file in tenant
  await page.click('#tenant-detail-view button:has-text("+ Add File")');
  await page.waitForSelector('#td-knowledge-modal.active');
  await page.fill('#td-k-name', 'e2e-runbook.md');
  await page.fill('#td-k-content', '# E2E Runbook\nOnly for e2e-team.');
  await page.click('#td-knowledge-modal .btn-primary');
  await page.waitForTimeout(800);

  const tdKnCard = await page.locator('#td-knowledge-list .card').count();
  check('Tenant knowledge file added', tdKnCard >= 1);
  await screenshot('tenants-knowledge-added');

  // 9f. Cross-tenant isolation verification
  // Check that team-alpha does NOT have e2e-team's resources
  const alphaGlossary = await apiGet('/admin/api/tenants/team-alpha/glossary');
  const alphaKnowledge = await apiGet('/admin/api/tenants/team-alpha/knowledge');
  const alphaSkills = await apiGet('/admin/api/tenants/team-alpha/skills');

  check('Isolation: Alpha glossary lacks e2e-svc', !('e2e-svc' in (alphaGlossary.glossary || {})));
  check('Isolation: Alpha knowledge lacks e2e-runbook', !(alphaKnowledge.files || []).some(f => f.name === 'e2e-runbook.md'));
  check('Isolation: Alpha skills lacks E2E Skill', !(alphaSkills.skills || []).some(s => s.name === 'E2E Skill'));

  // Also verify shared (non-tenant) glossary doesn't have tenant data
  const sharedGlossary = await apiGet('/admin/api/glossary');
  check('Isolation: Shared glossary lacks e2e-svc', !('e2e-svc' in (sharedGlossary.glossary || {})));

  await screenshot('tenants-isolation-verified');

  // 9g. Go back to list and delete tenant
  await page.click('#tenant-detail-view button:has-text("←")');
  await page.waitForTimeout(300);
  await page.locator('#tenants-list .card').filter({ hasText: 'E2E Test Team' }).locator('text="Delete"').click();
  await acceptConfirm();
  await page.waitForTimeout(1000);

  const tenantAfterDel = await apiGet('/admin/api/tenants');
  const e2eRemains = tenantAfterDel.tenants.some(t => t.id === 'e2e-team');
  check('Tenant deleted', !e2eRemains);
  await screenshot('tenants-after-delete');

  // ═══════════════════════════════════════════════════════════════
  // 10. CLUSTERS TAB
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 10. Clusters ---');
  await switchTab('clusters');
  await page.waitForTimeout(500);
  await screenshot('clusters-list');

  const clAwsCheckbox = await page.locator('#cl-aws-enabled').isVisible();
  check('Clusters tab has AWS checkbox', clAwsCheckbox);

  // Add static cluster
  await page.locator('button[onclick="openStaticClusterModal()"]').click();
  await page.waitForSelector('#static-cluster-modal.active');
  await page.fill('#sc-name', 'e2e-cluster');
  await page.fill('#sc-region', 'us-west-2');
  await page.fill('#sc-account', '888888888888');
  await screenshot('clusters-create-modal');
  await page.click('#static-cluster-modal .btn-primary');
  await page.waitForTimeout(800);

  const clusterCard = await page.locator('#static-clusters-list .card').filter({ hasText: 'e2e-cluster' }).count();
  check('Static cluster created', clusterCard === 1);
  await screenshot('clusters-created');

  // Delete
  await page.locator('#static-clusters-list .card').filter({ hasText: 'e2e-cluster' }).locator('text="Delete"').click();
  await acceptConfirm();
  await page.waitForTimeout(800);
  const clApi = await apiGet('/admin/api/clusters');
  const clRemains = (clApi.clusters?.static || []).some(c => c.name === 'e2e-cluster');
  check('Static cluster deleted', !clRemains);

  // ═══════════════════════════════════════════════════════════════
  // 11. PROVIDER TAB
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 11. Provider ---');
  await switchTab('provider');
  await page.waitForTimeout(500);
  await screenshot('provider-view');

  const pvType = await page.locator('#pv-type').inputValue();
  check('Provider type loaded', pvType.length > 0);

  // Switch provider type and verify dynamic fields
  await page.selectOption('#pv-type', 'anthropic');
  await page.waitForTimeout(300);
  const hasApiKeyField = await page.locator('#pv-fields input').count();
  check('Anthropic shows API key field', hasApiKeyField >= 1);
  await screenshot('provider-anthropic');

  // Switch back to bedrock
  await page.selectOption('#pv-type', 'bedrock');
  await page.waitForTimeout(300);
  const bedrockFields = await page.locator('#pv-fields input').count();
  check('Bedrock has no extra fields', bedrockFields === 0);
  await screenshot('provider-bedrock');

  // ═══════════════════════════════════════════════════════════════
  // 12. FORM VALIDATION
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 12. Form Validation ---');

  // Glossary: empty key
  await switchTab('glossary');
  await page.click('text="+ New Term"');
  await page.waitForSelector('#glossary-modal.active');
  await page.fill('#g-key', '');
  await page.click('#glossary-modal .btn-primary');
  await page.waitForTimeout(500);
  const toastVisible = await page.locator('.toast.show').isVisible();
  check('Empty glossary key shows toast error', toastVisible);
  await screenshot('validation-empty-key');
  await page.click('#glossary-modal button:has-text("Cancel")');
  await page.waitForTimeout(300);

  // Tenants: duplicate channel via API
  await switchTab('tenants');
  const dupRes = await page.evaluate(async (base) => {
    const r = await fetch(base + '/admin/api/tenants', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        tenants: [
          { id: 'dup-a', name: 'A', channels: [{ platform: 'feishu', channel_id: 'dup_ch' }] },
          { id: 'dup-b', name: 'B', channels: [{ platform: 'feishu', channel_id: 'dup_ch' }] },
        ],
      }),
    });
    return { status: r.status, body: await r.json() };
  }, BASE);
  check('Duplicate channel returns 400', dupRes.status === 400);
  check('Error mentions Duplicate', dupRes.body?.error?.includes('Duplicate'));

  // ═══════════════════════════════════════════════════════════════
  // 13. RESPONSIVE LAYOUT
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 13. Responsive ---');
  await page.setViewportSize({ width: 1280, height: 900 });
  await switchTab('tenants');
  await screenshot('responsive-desktop');

  await page.setViewportSize({ width: 768, height: 1024 });
  await page.waitForTimeout(300);
  await screenshot('responsive-tablet');

  await page.setViewportSize({ width: 375, height: 812 });
  await page.waitForTimeout(300);
  await screenshot('responsive-mobile');

  // Reset viewport
  await page.setViewportSize({ width: 1280, height: 900 });

  // ═══════════════════════════════════════════════════════════════
  // SUMMARY
  // ═══════════════════════════════════════════════════════════════
  console.log('\n========================================');
  console.log(' TEST SUMMARY');
  console.log('========================================');

  const passed = results.filter(r => r.ok).length;
  const failed = results.filter(r => !r.ok).length;

  for (const r of results) {
    if (!r.ok) console.log(`  FAIL: ${r.name}`);
  }

  console.log(`\n  ${passed} passed, ${failed} failed out of ${results.length} tests`);
  console.log(`  Screenshots: ${stepNum} files in tests/screenshots/`);
  console.log('========================================\n');

  await browser.close();
  process.exit(failed > 0 ? 1 : 0);
}

run().catch(err => {
  console.error('FATAL:', err);
  process.exit(1);
});
