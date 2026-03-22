/**
 * OpsAgent Demo — Full Video Recording via Playwright
 *
 * Records a complete product demo as .webm video.
 * Simulates real user interactions with deliberate pacing.
 *
 * Output: tests/demo-video/opsagent-demo.webm
 *
 * Prerequisites:
 *   cd bot && npm run build && PORT=3978 node dist/index.js
 *   Users: admin/admin123, alpha-ops/admin123, beta-ops/admin123
 */
import { chromium } from 'playwright';
import path from 'path';
import { fileURLToPath } from 'url';
import fs from 'fs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const VIDEO_DIR = path.join(__dirname, 'demo-video');
const BASE = 'http://localhost:3978';

let browser, context, page;

// ── Helpers ──────────────────────────────────────────────────────

async function pause(ms = 1000) {
  await page.waitForTimeout(ms);
}

async function typeSlowly(selector, text, delay = 50) {
  await page.click(selector);
  await page.fill(selector, '');
  await page.type(selector, text, { delay });
}

async function switchTab(tab) {
  await page.click(`a[href="#${tab}"]`);
  await pause(800);
}

async function sendChatAndWait(message, timeout = 120000) {
  // Wait for send button enabled
  await page.waitForFunction(() => !document.getElementById('chat-send-btn')?.disabled, { timeout: 120000 });
  await pause(300);
  // Clear old bubbles
  await page.evaluate(() => document.getElementById('chat-messages').innerHTML = '');
  await pause(200);
  await typeSlowly('#chat-input', message, 30);
  await pause(500);
  await page.click('#chat-send-btn');
  // Wait for bot response
  try {
    await page.waitForFunction(() => {
      const bots = document.querySelectorAll('.chat-bubble.bot');
      const last = bots[bots.length - 1];
      return last && last.style.display !== 'none' && last.textContent.trim().length > 5;
    }, { timeout });
    await pause(2000); // Let viewer read the response
    return true;
  } catch {
    await pause(1000);
    return false;
  }
}

// ═══════════════════════════════════════════════════════════════════
async function run() {
  console.log('===========================================');
  console.log('  OpsAgent Demo — Video Recording');
  console.log('===========================================\n');

  browser = await chromium.launch({ headless: true });
  context = await browser.newContext({
    viewport: { width: 1280, height: 720 },
    recordVideo: {
      dir: VIDEO_DIR,
      size: { width: 1280, height: 720 },
    },
  });
  page = await context.newPage();
  page.on('dialog', async d => await d.accept());

  // ════════════════════════════════════════════════════════════════
  // ACT 1: LOGIN (15s)
  // ════════════════════════════════════════════════════════════════
  console.log('ACT 1: Login');
  await page.goto(`${BASE}/admin`);
  await page.waitForLoadState('networkidle');
  await pause(2000); // Show login page

  await typeSlowly('#login-username', 'admin', 80);
  await pause(300);
  await typeSlowly('#login-password', 'admin123', 80);
  await pause(500);
  await page.click('#login-screen button');
  await pause(2000); // Show dashboard

  // ════════════════════════════════════════════════════════════════
  // ACT 2: GLOSSARY — Create term (20s)
  // ════════════════════════════════════════════════════════════════
  console.log('ACT 2: Glossary');
  await switchTab('glossary');
  await pause(1500); // Show existing terms

  await page.click('text="+ New Term"');
  await page.waitForSelector('#glossary-modal.active');
  await pause(500);
  await typeSlowly('#g-key', 'payment-gateway', 40);
  await typeSlowly('#g-fullname', 'Payment Gateway Service', 40);
  await typeSlowly('#g-desc', 'Handles all payment processing on ecommerce-prod cluster', 30);
  await typeSlowly('#g-aliases', 'pg, payments', 40);
  await typeSlowly('#g-accounts', '034362076319', 40);
  await pause(500);
  await page.click('#glossary-modal .btn-primary');
  await pause(1500);

  // ════════════════════════════════════════════════════════════════
  // ACT 3: ACCOUNTS (8s)
  // ════════════════════════════════════════════════════════════════
  console.log('ACT 3: Accounts');
  await switchTab('accounts');
  await pause(2000); // Show accounts config
  // Scroll down to show overrides/multi-cloud
  await page.evaluate(() => window.scrollTo(0, 500));
  await pause(1500);
  await page.evaluate(() => window.scrollTo(0, 0));

  // ════════════════════════════════════════════════════════════════
  // ACT 4: KNOWLEDGE — Create runbook (20s)
  // ════════════════════════════════════════════════════════════════
  console.log('ACT 4: Knowledge');
  await switchTab('knowledge');
  await pause(1500);

  await page.click('text="+ New File"');
  await page.waitForSelector('#knowledge-modal.active');
  await pause(300);
  await typeSlowly('#k-filename', 'runbook-payment.md', 40);
  await page.fill('#k-content', `# Payment Service Runbook

## Architecture
Payment service runs on ecommerce-prod cluster, payment namespace.

## Health Check
\`\`\`bash
kubectl --context ecommerce-prod -n payment get pods
\`\`\`

## Common Issues
- High latency: check RDS connections
- 5xx errors: check payment-gateway logs
`);
  await pause(800);
  await page.click('#knowledge-modal .btn-primary');
  await pause(1500);

  // ════════════════════════════════════════════════════════════════
  // ACT 5: SKILLS (5s)
  // ════════════════════════════════════════════════════════════════
  console.log('ACT 5: Skills');
  await switchTab('skills');
  await pause(2000);

  // ════════════════════════════════════════════════════════════════
  // ACT 6: PROVIDER (10s)
  // ════════════════════════════════════════════════════════════════
  console.log('ACT 6: Provider');
  await switchTab('provider');
  await pause(1500);
  // Switch to Anthropic to show dynamic fields
  await page.selectOption('#pv-type', 'anthropic');
  await pause(1500);
  // Switch back to Bedrock
  await page.selectOption('#pv-type', 'bedrock');
  await pause(1000);

  // ════════════════════════════════════════════════════════════════
  // ACT 7: TENANTS — Create + Manage (25s)
  // ════════════════════════════════════════════════════════════════
  console.log('ACT 7: Tenants');
  await switchTab('tenants');
  await pause(1500);

  // Create new tenant
  await page.click('text="+ New Tenant"');
  await page.waitForSelector('#tenant-modal.active');
  await pause(300);
  await typeSlowly('#t-id', 'team-payments', 40);
  await typeSlowly('#t-name', 'Payments Team', 40);

  await page.click('text="+ Add Channel"');
  await pause(200);
  await page.locator('#t-channels .t-ch-platform').last().selectOption('feishu');
  await page.locator('#t-channels .t-ch-id').last().fill('oc_payments_channel');

  await typeSlowly('#t-aws-accounts', '034362076319', 40);
  await pause(300);
  await page.click('#tenant-modal .btn-primary');
  await pause(1500);

  // Open Team Alpha detail
  await page.locator('#tenants-list .card').filter({ hasText: 'Team Alpha' }).locator('text="Manage"').click();
  await pause(1500);

  // Add glossary term to Alpha
  await page.click('#tenant-detail-view button:has-text("+ Add Term")');
  await page.waitForSelector('#td-glossary-modal.active');
  await typeSlowly('#td-g-key', 'alpha-api', 40);
  await typeSlowly('#td-g-fullname', 'Alpha Internal API', 40);
  await page.click('#td-glossary-modal .btn-primary');
  await pause(1500);

  // Go back
  await page.click('#tenant-detail-view button:has-text("←")');
  await pause(800);

  // ════════════════════════════════════════════════════════════════
  // ACT 8: CHAT — Global query (30s)
  // ════════════════════════════════════════════════════════════════
  console.log('ACT 8: Chat (Global)');
  await switchTab('chat');
  await page.selectOption('#chat-tenant-select', '');
  await pause(500);

  console.log('  Q1: Glossary...');
  await sendChatAndWait('列出所有已配置的公司术语，用表格展示 key、全称和描述');
  console.log('  Q1 done');

  // ════════════════════════════════════════════════════════════════
  // ACT 9: CHAT — Tenant isolation (30s)
  // ════════════════════════════════════════════════════════════════
  console.log('ACT 9: Chat Isolation');

  // Alpha
  await page.selectOption('#chat-tenant-select', 'team-alpha');
  await pause(800);
  console.log('  Alpha...');
  await sendChatAndWait('列出所有已配置的公司术语');
  console.log('  Alpha done');

  // Beta
  await page.selectOption('#chat-tenant-select', 'team-beta');
  await pause(800);
  console.log('  Beta...');
  await sendChatAndWait('列出所有已配置的公司术语');
  console.log('  Beta done');

  // ════════════════════════════════════════════════════════════════
  // ACT 10: USER PERMISSIONS (20s)
  // ════════════════════════════════════════════════════════════════
  console.log('ACT 10: User Permissions');
  await switchTab('users');
  await pause(2000);

  // Logout
  await page.click('#logout-link');
  await pause(1500);

  // Login as alpha-ops
  await typeSlowly('#login-username', 'alpha-ops', 60);
  await typeSlowly('#login-password', 'admin123', 60);
  await pause(300);
  await page.click('#login-screen button');
  await pause(2000); // Show limited tabs

  // Show glossary
  await switchTab('glossary');
  await pause(2000);

  // Show chat (auto-scoped)
  await switchTab('chat');
  await pause(1500);

  // Logout back to admin
  await page.click('#logout-link');
  await pause(1000);
  await typeSlowly('#login-username', 'admin', 60);
  await typeSlowly('#login-password', 'admin123', 60);
  await page.click('#login-screen button');
  await pause(1500);

  // ════════════════════════════════════════════════════════════════
  // ACT 11: ADVANCED FEATURES (15s)
  // ════════════════════════════════════════════════════════════════
  console.log('ACT 11: Advanced Features');
  await switchTab('scheduled-jobs');
  await pause(1500);
  await switchTab('plugins');
  await pause(1500);
  await switchTab('clusters');
  await pause(1500);
  await switchTab('platforms');
  await pause(1500);

  // Final: back to chat
  await switchTab('chat');
  await pause(2000);

  // ════════════════════════════════════════════════════════════════
  // CLEANUP
  // ════════════════════════════════════════════════════════════════
  console.log('\nCleanup...');

  // Delete test tenant
  await switchTab('tenants');
  await pause(300);
  const pt = page.locator('#tenants-list .card').filter({ hasText: 'Payments Team' });
  if (await pt.count() > 0) {
    await pt.locator('text="Delete"').click();
    await pause(500);
  }

  // Delete payment-gateway from glossary
  await switchTab('glossary');
  await pause(300);
  await page.evaluate(() => {
    const cards = document.querySelectorAll('#glossary-list .card');
    for (const c of cards) {
      if (c.textContent.includes('payment-gateway') && !c.textContent.includes('pci-zone')) {
        c.querySelector('button.btn-danger')?.click();
      }
    }
  });
  await pause(500);

  // Delete runbook-payment.md
  await switchTab('knowledge');
  await pause(300);
  await page.evaluate(() => {
    const cards = document.querySelectorAll('#knowledge-list .card');
    for (const c of cards) {
      if (c.textContent.includes('runbook-payment.md')) {
        c.querySelector('button.btn-danger')?.click();
      }
    }
  });
  await pause(500);

  // Delete alpha-api from alpha's glossary
  await page.evaluate(async () => {
    const res = await fetch('/admin/api/tenants/team-alpha/glossary');
    const data = await res.json();
    delete data.glossary['alpha-api'];
    await fetch('/admin/api/tenants/team-alpha/glossary', {
      method: 'PUT', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
  });

  // ════════════════════════════════════════════════════════════════
  // FINISH — Close and save video
  // ════════════════════════════════════════════════════════════════
  console.log('Saving video...');
  await page.close(); // This triggers video save
  await context.close();
  await browser.close();

  // Find the recorded video and rename it
  const files = fs.readdirSync(VIDEO_DIR).filter(f => f.endsWith('.webm'));
  if (files.length > 0) {
    const src = path.join(VIDEO_DIR, files[files.length - 1]);
    const dest = path.join(VIDEO_DIR, 'opsagent-demo.webm');
    if (src !== dest) fs.renameSync(src, dest);
    const size = fs.statSync(dest).size;
    console.log(`\n===========================================`);
    console.log(`  Video saved: tests/demo-video/opsagent-demo.webm`);
    console.log(`  Size: ${(size / 1024 / 1024).toFixed(1)} MB`);
    console.log(`===========================================\n`);
  }
}

run().catch(err => { console.error('FATAL:', err); process.exit(1); });
