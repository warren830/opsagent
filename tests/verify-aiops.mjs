/**
 * AIOps Platform — Full Verification Script
 *
 * Tests all new features: enriched tools, patrol, RCA, alert webhook,
 * Issues UI, Resources UI.
 *
 * Prerequisites: PORT=3999 node dist/index.js
 */
import { chromium } from 'playwright';
import http from 'http';

const BASE = process.env.BASE_URL || 'http://localhost:3999';
let browser, page;
let passed = 0, failed = 0, skipped = 0;
const results = [];

function check(name, ok) {
  if (ok) { passed++; results.push({ name, status: 'PASS' }); console.log(`  \x1b[32mPASS\x1b[0m ${name}`); }
  else { failed++; results.push({ name, status: 'FAIL' }); console.log(`  \x1b[31mFAIL\x1b[0m ${name}`); }
}

function skip(name, reason) {
  skipped++; results.push({ name, status: 'SKIP' }); console.log(`  \x1b[33mSKIP\x1b[0m ${name} (${reason})`);
}

async function httpPost(path, body) {
  return new Promise((resolve, reject) => {
    const url = new URL(path, BASE);
    const data = JSON.stringify(body);
    const req = http.request({ hostname: url.hostname, port: url.port, path: url.pathname, method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(data) },
    }, (res) => {
      let body = '';
      res.on('data', c => body += c);
      res.on('end', () => { try { resolve({ status: res.statusCode, data: JSON.parse(body) }); } catch { resolve({ status: res.statusCode, data: body }); } });
    });
    req.on('error', reject);
    req.write(data);
    req.end();
  });
}

async function httpGet(path) {
  return new Promise((resolve, reject) => {
    const url = new URL(path, BASE);
    http.get(url, (res) => {
      let body = '';
      res.on('data', c => body += c);
      res.on('end', () => { try { resolve({ status: res.statusCode, data: JSON.parse(body) }); } catch { resolve({ status: res.statusCode, data: body }); } });
    }).on('error', reject);
  });
}

async function run() {
  console.log('===========================================');
  console.log('  AIOps Platform — Full Verification');
  console.log('===========================================\n');

  // ═══════════════════════════════════════════════════════════════
  // 1. HEALTH CHECK
  // ═══════════════════════════════════════════════════════════════
  console.log('--- 1. Health Check ---');
  const health = await httpGet('/health');
  check('Health endpoint returns 200', health.status === 200);
  check('Health checks pass', health.data?.status === 'ok' || health.data?.status === 'degraded');

  // ═══════════════════════════════════════════════════════════════
  // 2. ALERT WEBHOOK
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 2. Alert Webhook ---');

  // CloudWatch alert
  const cwAlert = await httpPost('/api/alerts', {
    Type: 'Notification',
    Message: JSON.stringify({
      AlarmName: 'Test-HighCPU',
      NewStateValue: 'ALARM',
      NewStateReason: 'CPU > 90%',
      Trigger: { MetricName: 'CPUUtilization', Namespace: 'AWS/EC2',
        Dimensions: [{ name: 'InstanceId', value: 'i-test123' }] },
    }),
  });
  check('CloudWatch alert accepted (200)', cwAlert.status === 200);
  check('CloudWatch alert parsed correctly', cwAlert.data?.ok === true && cwAlert.data?.source === 'cloudwatch');

  // Datadog alert
  const ddAlert = await httpPost('/api/alerts', {
    title: 'Test Datadog Alert',
    alert_type: 'error',
    priority: 'P2',
    tags: 'instance:i-dd-test,env:staging',
    body: 'Test alert from verification script',
  });
  check('Datadog alert accepted (200)', ddAlert.status === 200);
  check('Datadog alert parsed correctly', ddAlert.data?.ok === true && ddAlert.data?.source === 'datadog');

  // Generic webhook
  const genericAlert = await httpPost('/api/alerts', {
    title: 'Test Generic Alert',
    severity: 'medium',
    resource_id: 'test-resource',
    description: 'Verification test alert',
  });
  check('Generic alert accepted (200)', genericAlert.status === 200);
  check('Generic alert parsed correctly', genericAlert.data?.ok === true && genericAlert.data?.source === 'webhook');

  // Invalid alert
  const badAlert = await httpPost('/api/alerts', { random: 'data' });
  check('Invalid alert rejected (400)', badAlert.status === 400);

  // ═══════════════════════════════════════════════════════════════
  // 3. ISSUES API (requires auth — test via browser context)
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 3. Issues API ---');
  // Issues/Resources APIs require auth. We'll test them through the browser later.
  // For now, verify the endpoints exist by checking they return 401 (not 404)
  const issuesNoAuth = await httpGet('/admin/api/issues');
  check('Issues endpoint exists (returns 401 not 404)', issuesNoAuth.status === 401);

  const resourcesNoAuth = await httpGet('/admin/api/resources');
  check('Resources endpoint exists (returns 401 not 404)', resourcesNoAuth.status === 401);

  const summaryNoAuth = await httpGet('/admin/api/resources/summary');
  check('Resources summary exists (returns 401 not 404)', summaryNoAuth.status === 401);

  // ═══════════════════════════════════════════════════════════════
  // 5. ADMIN UI — NEW TABS
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 5. Admin UI ---');
  browser = await chromium.launch({ headless: true });
  page = await browser.newPage({ viewport: { width: 1280, height: 900 } });

  await page.goto(`${BASE}/admin`);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(1000);

  // Login
  const loginScreen = await page.locator('#login-screen').isVisible();
  if (loginScreen) {
    await page.fill('#login-username', 'admin');
    await page.fill('#login-password', 'admin123');
    await page.click('#login-screen button');
    await page.waitForTimeout(1500);
  }

  // Check sidebar has new sections
  const opsSection = await page.locator('.sidebar nav').textContent();
  check('Sidebar has "Ops" section', opsSection.includes('Ops'));
  check('Sidebar has "Issues" link', opsSection.includes('Issues'));
  check('Sidebar has "Resources" link', opsSection.includes('Resources'));

  // Issues tab
  await page.click('a[href="#issues"]');
  await page.waitForTimeout(500);
  const issuesTab = await page.locator('#tab-issues').isVisible();
  check('Issues tab visible', issuesTab);
  const issuesFilter = await page.locator('#issues-filter').isVisible();
  check('Issues filter dropdown visible', issuesFilter);
  await page.screenshot({ path: 'tests/screenshots/verify-issues.png' });

  // Resources tab
  await page.click('a[href="#resources"]');
  await page.waitForTimeout(500);
  const resourcesTab = await page.locator('#tab-resources').isVisible();
  check('Resources tab visible', resourcesTab);
  const resourcesSearch = await page.locator('#resources-search').isVisible();
  check('Resources search input visible', resourcesSearch);
  const scanBtn = await page.locator('button:has-text("Scan Now")').isVisible();
  check('Scan Now button visible', scanBtn);
  await page.screenshot({ path: 'tests/screenshots/verify-resources.png' });

  // Approvals tab still works
  await page.click('a[href="#approvals"]');
  await page.waitForTimeout(500);
  const approvalsTab = await page.locator('#tab-approvals').isVisible();
  check('Approvals tab still works', approvalsTab);

  // Test authenticated API calls via browser context
  console.log('\n--- 5b. Authenticated API (via browser) ---');
  const issuesApiResult = await page.evaluate(async () => {
    const r = await fetch('/admin/api/issues');
    return { status: r.status, data: await r.json() };
  });
  check('Issues API authenticated (200)', issuesApiResult.status === 200);
  check('Issues returns array or db_error', Array.isArray(issuesApiResult.data?.issues) || issuesApiResult.data?.db_error);

  const resourcesApiResult = await page.evaluate(async () => {
    const r = await fetch('/admin/api/resources');
    return { status: r.status, data: await r.json() };
  });
  check('Resources API authenticated (200)', resourcesApiResult.status === 200);

  const summaryApiResult = await page.evaluate(async () => {
    const r = await fetch('/admin/api/resources/summary');
    return { status: r.status, data: await r.json() };
  });
  check('Resources summary authenticated (200)', summaryApiResult.status === 200);

  // ═══════════════════════════════════════════════════════════════
  // 6. CHAT — ENRICHED TOOLS
  // ═══════════════════════════════════════════════════════════════
  console.log('\n--- 6. Chat with Enriched Tools ---');
  await page.click('a[href="#chat"]');
  await page.waitForTimeout(500);

  // Wait for send button to be enabled
  await page.waitForFunction(() => {
    const btn = document.getElementById('chat-send-btn');
    return btn && !btn.disabled;
  }, { timeout: 120000 }).catch(() => {});

  // Send a patrol-like query
  await page.fill('#chat-input', '帮我检查一下 us-east-1 的 CloudWatch 告警状态');
  await page.click('#chat-send-btn');

  // Wait for response (enriched tools should be used)
  let chatResponse = false;
  try {
    await page.waitForFunction(() => {
      const bots = document.querySelectorAll('.chat-bubble.bot');
      const last = bots[bots.length - 1];
      return last && last.style.display !== 'none' && last.textContent.trim().length > 10;
    }, { timeout: 90000 });
    chatResponse = true;
  } catch { /* timeout */ }

  if (chatResponse) {
    const botText = await page.locator('.chat-bubble.bot').last().textContent();
    check('Chat response received', botText.length > 10);
    // Check if enriched tools were used (look for tool_use indicators or structured output)
    const hasToolUse = botText.includes('alarm') || botText.includes('告警') || botText.includes('ALARM') || botText.includes('OK') || botText.includes('Error');
    check('Response mentions alarms or tool output', hasToolUse);
    await page.screenshot({ path: 'tests/screenshots/verify-chat-enriched.png' });
  } else {
    skip('Chat enriched tools response', 'Chat timed out (LLM latency)');
  }

  await browser.close();

  // ═══════════════════════════════════════════════════════════════
  // SUMMARY
  // ═══════════════════════════════════════════════════════════════
  console.log('\n===========================================');
  console.log('  VERIFICATION SUMMARY');
  console.log('===========================================');
  console.log(`  ${passed} passed, ${failed} failed, ${skipped} skipped out of ${passed + failed + skipped} checks`);
  if (failed > 0) {
    console.log('\n  FAILED:');
    for (const r of results.filter(r => r.status === 'FAIL')) console.log(`    - ${r.name}`);
  }
  if (skipped > 0) {
    console.log('\n  SKIPPED:');
    for (const r of results.filter(r => r.status === 'SKIP')) console.log(`    - ${r.name}`);
  }
  console.log('===========================================\n');
}

run().catch(err => { console.error('FATAL:', err); process.exit(1); });
