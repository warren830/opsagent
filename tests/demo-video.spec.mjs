/**
 * OpsAgent Demo Video — Playwright Automated Screenshot Sequence
 *
 * Generates a complete screenshot series for a product demo video.
 * Each "act" corresponds to a section of the video script.
 *
 * Prerequisites:
 *   cd bot && npm run build
 *   PORT=3978 node dist/index.js
 *   Users: admin/admin123, alpha-ops/admin123, beta-ops/admin123
 *
 * Usage:
 *   node tests/demo-video.spec.mjs
 */
import { chromium } from 'playwright';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SCREENSHOT_DIR = path.join(__dirname, 'demo-screenshots');
const BASE = 'http://localhost:3978';
const CHAT_TIMEOUT = 90000;

let browser, page;
let shotNum = 0;

async function shot(name) {
  shotNum++;
  const filename = `${String(shotNum).padStart(2, '0')}-${name}.png`;
  await page.screenshot({ path: path.join(SCREENSHOT_DIR, filename), fullPage: true });
  console.log(`  [screenshot] ${filename}`);
  return filename;
}

async function login(username, password) {
  await page.goto(`${BASE}/admin`);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(500);
  // If login screen visible, login
  const loginVisible = await page.locator('#login-screen').isVisible();
  if (loginVisible) {
    await page.fill('#login-username', username);
    await page.fill('#login-password', password);
    await page.click('#login-screen button');
    await page.waitForTimeout(1000);
  }
}

async function switchTab(tab) {
  await page.click(`a[href="#${tab}"]`);
  await page.waitForTimeout(600);
}

async function sendChat(message, timeout = CHAT_TIMEOUT) {
  // Wait for send button to be enabled (previous chat may still be streaming)
  await page.waitForFunction(() => {
    const btn = document.getElementById('chat-send-btn');
    return btn && !btn.disabled;
  }, { timeout: 120000 });
  // Clear old bubbles
  await page.evaluate(() => document.getElementById('chat-messages').innerHTML = '');
  await page.waitForTimeout(200);
  await page.fill('#chat-input', message);
  await page.click('#chat-send-btn');
  try {
    await page.waitForFunction(() => {
      const bots = document.querySelectorAll('.chat-bubble.bot');
      const last = bots[bots.length - 1];
      return last && last.style.display !== 'none' && last.textContent.trim().length > 2;
    }, { timeout });
    await page.waitForTimeout(500); // let final render settle
    return true;
  } catch {
    return false;
  }
}

// ═══════════════════════════════════════════════════════════════════
async function run() {
  browser = await chromium.launch({ headless: true });
  page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  page.on('dialog', async d => await d.accept());

  console.log('===========================================');
  console.log('  OpsAgent Demo Video — Screenshot Capture');
  console.log('===========================================\n');

  // ── ACT 1: Login ──────────────────────────────────────────────
  console.log('ACT 1: Login');
  await page.goto(`${BASE}/admin`);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(500);
  await shot('login-page');

  await page.fill('#login-username', 'admin');
  await page.fill('#login-password', 'admin123');
  await shot('login-filled');

  await page.click('#login-screen button');
  await page.waitForTimeout(1000);
  await shot('admin-dashboard');

  // ── ACT 2a: Glossary ──────────────────────────────────────────
  console.log('\nACT 2a: Glossary');
  await switchTab('glossary');
  await shot('glossary-list');

  await page.click('text="+ New Term"');
  await page.waitForSelector('#glossary-modal.active');
  await page.fill('#g-key', 'payment-gateway');
  await page.fill('#g-fullname', 'Payment Gateway Service');
  await page.fill('#g-desc', 'Handles all payment processing, runs on ecommerce-prod cluster in payment namespace');
  await page.fill('#g-aliases', 'pg, payments');
  await page.fill('#g-accounts', '034362076319');
  await shot('glossary-create');

  await page.click('#glossary-modal .btn-primary');
  await page.waitForTimeout(800);
  await shot('glossary-saved');

  // ── ACT 2b: Accounts ─────────────────────────────────────────
  console.log('\nACT 2b: Accounts');
  await switchTab('accounts');
  await shot('accounts-config');

  // ── ACT 2c: Provider ─────────────────────────────────────────
  console.log('\nACT 2c: Provider');
  await switchTab('provider');
  await page.waitForTimeout(500);
  await shot('provider-bedrock');

  await page.selectOption('#pv-type', 'anthropic');
  await page.waitForTimeout(400);
  await shot('provider-anthropic');

  await page.selectOption('#pv-type', 'bedrock');
  await page.waitForTimeout(400);

  // ── ACT 3a: Knowledge ────────────────────────────────────────
  console.log('\nACT 3a: Knowledge');
  await switchTab('knowledge');
  await page.waitForTimeout(500);
  await shot('knowledge-list');

  await page.click('text="+ New File"');
  await page.waitForSelector('#knowledge-modal.active');
  await page.fill('#k-filename', 'runbook-payment.md');
  await page.fill('#k-content', `# Payment Service Runbook

## Architecture
Payment service runs on ecommerce-prod cluster, payment namespace.
Main components: payment-gateway, payment-processor, payment-notifier.

## Health Check
\`\`\`bash
kubectl --context ecommerce-prod -n payment get pods
kubectl --context ecommerce-prod -n payment get svc
\`\`\`

## Common Issues
- **High latency**: Check RDS connection pool and slow queries
- **5xx errors**: Check payment-gateway deployment logs
- **Payment timeout**: Verify SQS queue depth and Lambda concurrency
`);
  await shot('knowledge-create');

  await page.click('#knowledge-modal .btn-primary');
  await page.waitForTimeout(800);
  await shot('knowledge-saved');

  // ── ACT 3b: Skills ───────────────────────────────────────────
  console.log('\nACT 3b: Skills');
  await switchTab('skills');
  await page.waitForTimeout(500);
  await shot('skills-list');

  // ── ACT 4a: Tenants ──────────────────────────────────────────
  console.log('\nACT 4a: Tenants');
  await switchTab('tenants');
  await page.waitForTimeout(500);
  await shot('tenants-list');

  // Create new tenant
  await page.click('text="+ New Tenant"');
  await page.waitForSelector('#tenant-modal.active');
  await page.fill('#t-id', 'team-payments');
  await page.fill('#t-name', 'Payments Team');
  await page.click('text="+ Add Channel"');
  await page.waitForTimeout(200);
  await page.locator('#t-channels .t-ch-platform').last().selectOption('feishu');
  await page.locator('#t-channels .t-ch-id').last().fill('oc_payments_channel');
  await page.fill('#t-aws-accounts', '034362076319');
  await page.click('text="+ Add Alicloud Account"');
  await page.waitForTimeout(200);
  await page.locator('#t-alicloud .t-ali-name').last().fill('payments-cn');
  await page.locator('#t-alicloud .t-ali-region').last().fill('cn-hangzhou');
  await page.locator('#t-alicloud .t-ali-ak').last().fill('PAYMENTS_AK');
  await page.locator('#t-alicloud .t-ali-sk').last().fill('PAYMENTS_SK');
  await shot('tenants-create');

  await page.click('#tenant-modal .btn-primary');
  await page.waitForTimeout(1000);
  await shot('tenants-created');

  // ── ACT 4b: Tenant Manage ────────────────────────────────────
  console.log('\nACT 4b: Tenant Manage');
  await page.locator('#tenants-list .card').filter({ hasText: 'Team Alpha' }).locator('text="Manage"').click();
  await page.waitForTimeout(500);
  await shot('tenant-detail-alpha');

  // Add a term to Alpha's glossary
  await page.click('#tenant-detail-view button:has-text("+ Add Term")');
  await page.waitForSelector('#td-glossary-modal.active');
  await page.fill('#td-g-key', 'alpha-api');
  await page.fill('#td-g-fullname', 'Alpha Internal API');
  await page.fill('#td-g-desc', 'Internal microservice API for Alpha team');
  await page.click('#td-glossary-modal .btn-primary');
  await page.waitForTimeout(800);
  await shot('tenant-glossary-added');

  // Go back
  await page.click('#tenant-detail-view button:has-text("←")');
  await page.waitForTimeout(300);

  // ── ACT 5a: Chat — Global ────────────────────────────────────
  console.log('\nACT 5a: Chat (Global)');
  await switchTab('chat');
  await page.selectOption('#chat-tenant-select', '');
  await page.waitForTimeout(300);

  // Q1: Glossary
  console.log('  Sending glossary query...');
  const q1ok = await sendChat('列出所有已配置的公司术语，用表格展示 key、全称和描述');
  await shot('chat-global-glossary');
  console.log(`  Q1: ${q1ok ? 'OK' : 'timeout'}`);

  // Q2: AWS query
  console.log('  Sending AWS query...');
  const q2ok = await sendChat('查询 hub-account 的 S3 bucket 列表');
  await shot('chat-global-aws');
  console.log(`  Q2: ${q2ok ? 'OK' : 'timeout'}`);

  // ── ACT 5b: Chat — Tenant Isolation ──────────────────────────
  console.log('\nACT 5b: Chat Isolation');

  // Alpha
  console.log('  [Alpha] Sending glossary query...');
  await page.selectOption('#chat-tenant-select', 'team-alpha');
  await page.waitForTimeout(300);
  const alphaOk = await sendChat('列出所有已配置的公司术语');
  await shot('chat-alpha-terms');
  console.log(`  Alpha: ${alphaOk ? 'OK' : 'timeout'}`);

  // Beta
  console.log('  [Beta] Sending glossary query...');
  await page.selectOption('#chat-tenant-select', 'team-beta');
  await page.waitForTimeout(300);
  const betaOk = await sendChat('列出所有已配置的公司术语');
  await shot('chat-beta-terms');
  console.log(`  Beta: ${betaOk ? 'OK' : 'timeout'}`);

  // ── ACT 6: User Permissions ──────────────────────────────────
  console.log('\nACT 6: User Permissions');
  await switchTab('users');
  await page.waitForTimeout(500);
  await shot('users-tab');

  // Open user create modal
  await page.click('text="+ Add User"');
  await page.waitForSelector('#user-modal.active');
  await page.fill('#u-username', 'demo-user');
  await page.fill('#u-password', 'demo123');
  await page.selectOption('#u-role', 'tenant_admin');
  await page.waitForTimeout(200);
  await shot('users-create-modal');
  await page.click('#user-modal button:has-text("Cancel")');
  await page.waitForTimeout(300);

  // Logout
  await page.click('#logout-link');
  await page.waitForTimeout(500);
  await shot('logout-login-page');

  // Login as alpha-ops
  await page.fill('#login-username', 'alpha-ops');
  await page.fill('#login-password', 'admin123');
  await page.click('#login-screen button');
  await page.waitForTimeout(1000);
  await shot('alpha-ops-dashboard');

  // Show limited tabs — count visible nav links
  const visibleTabs = await page.locator('#app-sidebar nav a:visible').count();
  console.log(`  alpha-ops sees ${visibleTabs} tabs`);

  // Glossary — should be Alpha-scoped
  await switchTab('glossary');
  await page.waitForTimeout(500);
  await shot('alpha-ops-glossary');

  // Chat — no tenant selector (auto-scoped)
  await switchTab('chat');
  await page.waitForTimeout(300);
  await shot('alpha-ops-chat');

  // Logout and re-login as admin
  await page.click('#logout-link');
  await page.waitForTimeout(500);
  await login('admin', 'admin123');

  // ── ACT 7: Advanced Features ─────────────────────────────────
  console.log('\nACT 7: Advanced Features');
  await switchTab('scheduled-jobs');
  await page.waitForTimeout(500);
  await shot('scheduled-jobs');

  await switchTab('plugins');
  await page.waitForTimeout(500);
  await shot('plugins');

  await switchTab('clusters');
  await page.waitForTimeout(500);
  await shot('clusters');

  await switchTab('platforms');
  await page.waitForTimeout(500);
  await shot('platforms');

  // Final: back to chat
  await switchTab('chat');
  await page.selectOption('#chat-tenant-select', '');
  await page.waitForTimeout(300);
  await shot('final-chat');

  // ── Cleanup: delete test tenant ──────────────────────────────
  console.log('\nCleanup...');
  await switchTab('tenants');
  await page.waitForTimeout(500);
  const paymentsTenant = page.locator('#tenants-list .card').filter({ hasText: 'Payments Team' });
  if (await paymentsTenant.count() > 0) {
    await paymentsTenant.locator('text="Delete"').click();
    await page.waitForTimeout(500);
  }

  // Delete payment-gateway glossary entry
  await switchTab('glossary');
  await page.waitForTimeout(500);
  const pgCard = page.locator('#glossary-list .card').filter({ hasText: 'payment-gateway' });
  if (await pgCard.count() > 0) {
    await pgCard.locator('text="Delete"').click();
    await page.waitForTimeout(500);
  }

  // Delete runbook-payment.md
  await switchTab('knowledge');
  await page.waitForTimeout(500);
  const knCard = page.locator('#knowledge-list .card').filter({ hasText: 'runbook-payment.md' });
  if (await knCard.count() > 0) {
    await knCard.locator('text="Delete"').click();
    await page.waitForTimeout(500);
  }

  // ── Summary ──────────────────────────────────────────────────
  console.log('\n===========================================');
  console.log(`  DONE: ${shotNum} screenshots captured`);
  console.log(`  Output: tests/demo-screenshots/`);
  console.log('===========================================\n');

  await browser.close();
}

run().catch(err => { console.error('FATAL:', err); process.exit(1); });
