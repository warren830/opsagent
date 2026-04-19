/**
 * OpsAgent demo recording — time-budgeted, 120s body.
 * Selectors verified via scout.spec.ts.
 */
import { test } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const BASE = 'https://dg00c54mwvycp.cloudfront.net';
const USER = 'admin';
const PASS = 'admin123';
const FPS = 30;
const FRAME_INTERVAL_MS = Math.round(1000 / FPS);
const MAX_RUNTIME_MS = 600_000;

const FRAMES_DIR = path.resolve(__dirname, '../frames');
const issuesUrl = `${BASE}/issues`;
const deploymentsUrl = `${BASE}/deployments`;

// Issue title default includes "rca-demo" (the actual title in DB).
const ISSUE_TITLE = process.env.DEMO_ISSUE_TITLE || '订单服务 rca-demo 错误率飙升至 6.8%';

async function selectCluster(page: import('@playwright/test').Page) {
  const trigger = page.getByRole('button', { name: 'Select Cluster' });
  if (await trigger.count() === 0) return false;
  await trigger.click({ timeout: 3_000 }).catch(() => {});
  await page.waitForTimeout(800);
  const row = page.getByText(/ops-eks-ap-southeast-1-default/).first();
  if (await row.count() === 0) return false;
  await row.click({ timeout: 3_000 }).catch(() => {});
  await page.waitForTimeout(500);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(1_500);
  return true;
}

test.describe.configure({ mode: 'serial' });

test('record demo (time-budgeted 120s)', async ({ page }) => {
  test.setTimeout(MAX_RUNTIME_MS);

  fs.rmSync(FRAMES_DIR, { recursive: true, force: true });
  fs.mkdirSync(FRAMES_DIR, { recursive: true });

  const bootStart = Date.now();
  const t = () => ((Date.now() - bootStart) / 1000).toFixed(1);
  const mark = (name: string) => console.log(`[${t()}s] ${name}`);

  // ─── Log in (before recording) ────────────────────────────────────────
  mark('login');
  await page.goto(`${BASE}/login`);
  await page.fill('input[type="text"]', USER);
  await page.fill('input[type="password"]', PASS);
  await page.click('button[type="submit"]');
  await page.waitForURL((url) => !url.pathname.startsWith('/login'), { timeout: 15_000 });
  await page.waitForTimeout(500);

  // ─── Pre-select cluster ────────────────────────────────────────
  mark('pre-select cluster');
  await page.goto(deploymentsUrl);
  await page.waitForLoadState('networkidle').catch(() => {});
  await page.waitForTimeout(1_500);
  await selectCluster(page);
  // Wait for inventory row to render
  await page.getByText('inventory-service').first().waitFor({ state: 'visible', timeout: 10_000 }).catch(() => {
    console.warn('  ! inventory-service still not visible after cluster select');
  });

  // ─── Go to /issues + find target ──────────────────────────────────────
  mark('goto /issues');
  await page.goto(issuesUrl);
  await page.waitForLoadState('networkidle').catch(() => {});
  await page.waitForTimeout(3_000);

  const titleRe = new RegExp(ISSUE_TITLE.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'i');
  const issueTitleBtn = page.locator('button.font-medium.text-foreground').filter({ hasText: titleRe }).first();
  try {
    await issueTitleBtn.waitFor({ state: 'visible', timeout: 8_000 });
  } catch {
    mark('  ! issue row slow — reloading');
    await page.reload().catch(() => {});
    await page.waitForLoadState('networkidle').catch(() => {});
    await issueTitleBtn.waitFor({ state: 'visible', timeout: 10_000 });
  }

  // ─── Start CDP screencast ─────────────────────────────────────
  mark('screencast ON');
  const client = await page.context().newCDPSession(page);
  let latestData: string | null = null;
  let stopRequested = false;

  client.on('Page.screencastFrame', async (params) => {
    latestData = params.data;
    await client.send('Page.screencastFrameAck', { sessionId: params.sessionId }).catch(() => {});
  });

  await client.send('Page.startScreencast', {
    format: 'png',
    everyNthFrame: 1,
    maxWidth: 1920,
    maxHeight: 1080,
  });

  // Wait for first frame
  const firstFrameDeadline = Date.now() + 3_000;
  while (!latestData && Date.now() < firstFrameDeadline) {
    await new Promise((r) => setTimeout(r, 50));
  }

  const recordStart = Date.now();
  const elapsed = () => (Date.now() - recordStart) / 1000;
  const waitUntil = async (sec: number) => {
    const target = recordStart + sec * 1000;
    const remain = target - Date.now();
    if (remain > 0) await page.waitForTimeout(remain);
  };
  let frameIdx = 0;
  const markers: Array<{ name: string; sec: number }> = [];
  const rmark = (name: string) => {
    const sec = elapsed();
    markers.push({ name, sec });
    console.log(`  [rec ${sec.toFixed(1)}s] ${name}`);
  };

  const writer = (async () => {
    while (!stopRequested) {
      const target = recordStart + frameIdx * FRAME_INTERVAL_MS;
      const wait = target - Date.now();
      if (wait > 0) await new Promise((r) => setTimeout(r, wait));
      if (latestData) {
        const fname = `frame-${String(frameIdx).padStart(5, '0')}.png`;
        fs.writeFileSync(path.join(FRAMES_DIR, fname), Buffer.from(latestData, 'base64'));
      }
      frameIdx++;
    }
  })();

  // =========================================================================
  //  0-5.5s  STEP 1: /issues list — mouse drift on firing row
  // =========================================================================
  rmark('STEP 1 — /issues hold');
  page.mouse.move(600, 250, { steps: 20 }).catch(() => {});
  await waitUntil(2);
  page.mouse.move(900, 260, { steps: 30 }).catch(() => {});
  await waitUntil(5.5);

  // =========================================================================
  //  5.5-7.5s  STEP 2: click issue → Dialog
  // =========================================================================
  rmark('STEP 2 — click issue');
  await issueTitleBtn.click().catch(() => {});
  await page
    .getByRole('heading', { name: /^(detail|详情)$/i })
    .first()
    .waitFor({ state: 'visible', timeout: 6_000 })
    .catch(() => console.warn('  ! Detail heading not found'));
  await waitUntil(7.5);

  // =========================================================================
  //  7.5-10s  STEP 3: click "Start RCA Analysis"
  // =========================================================================
  rmark('STEP 3 — click Start RCA');
  const startRcaBtn = page.getByRole('button', { name: 'Start RCA Analysis', exact: true }).first();
  if ((await startRcaBtn.count()) > 0 && (await startRcaBtn.isVisible().catch(() => false))) {
    await startRcaBtn.click().catch(() => {});
  } else {
    console.warn('  ! Start RCA button missing — maybe already streaming');
  }

  await Promise.race([
    page.waitForSelector('.animate-spin.text-orange-400', { state: 'visible', timeout: 10_000 }).catch(() => null),
    page.waitForSelector('.rca-markdown', { state: 'visible', timeout: 10_000 }).catch(() => null),
  ]);
  rmark('STEP 3 — stream started');
  await waitUntil(10);

  // =========================================================================
  //  10-70s  STEP 4: hold on Dialog — stream + scroll, try to reach completion
  // =========================================================================
  rmark('STEP 4 — stream hold');
  const rcaPane = page.locator('.rca-markdown').first();
  let streamEndedAt: number | null = null;
  // Hold until either:
  //  - the streaming spinner has been gone for 3s (RCA really completed), OR
  //  - we hit the 70s hard deadline
  while (elapsed() < 70) {
    await rcaPane
      .evaluate((el: HTMLElement) => el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' }))
      .catch(() => {});
    const stillStreaming = await page.locator('.animate-spin.text-orange-400').first().isVisible().catch(() => false);
    if (!stillStreaming) {
      if (streamEndedAt === null) streamEndedAt = elapsed();
      if (elapsed() - streamEndedAt > 3) {
        console.log(`  [rec ${elapsed().toFixed(1)}s] RCA fully done, exit STEP 4 early`);
        break;
      }
    } else {
      streamEndedAt = null;
    }
    await page.waitForTimeout(1_500);
  }

  // =========================================================================
  //  70-75s  STEP 5: close dialog → /deployments + re-select cluster
  // =========================================================================
  rmark('STEP 5 — close dialog + goto /deployments');
  const closeBtn = page.locator('[role="dialog"] button').filter({ hasText: /^close$/i }).last();
  if (await closeBtn.isVisible().catch(() => false)) {
    await closeBtn.click({ timeout: 2_000 }).catch(() => {});
  } else {
    await page.keyboard.press('Escape').catch(() => {});
  }
  await page.waitForTimeout(600);
  const dialogStillOpen = await page.locator('[role="dialog"]').first().isVisible().catch(() => false);
  if (dialogStillOpen) {
    await page.mouse.click(50, 400).catch(() => {});
    await page.waitForTimeout(400);
  }

  await page.goto(deploymentsUrl);
  await page.waitForLoadState('networkidle').catch(() => {});
  await page.waitForTimeout(1_000);
  // Re-select cluster (isolated context doesn't persist)
  await selectCluster(page);
  await page.getByText('inventory-service').first().waitFor({ state: 'visible', timeout: 8_000 }).catch(() => {
    console.warn('  ! inventory-service not visible after re-select');
  });
  await waitUntil(75);

  // =========================================================================
  //  75-85s  STEP 6: inventory card hold (10s), hover
  // =========================================================================
  rmark('STEP 6 — inventory card hold');
  const invRow = page
    .locator('div.group')
    .filter({ hasText: /inventory-service/i })
    .filter({ hasText: /Paused|Progressing|Degraded/i })
    .first();
  await invRow.scrollIntoViewIfNeeded().catch(() => {});
  await invRow.hover({ timeout: 2_000 }).catch(() => {});
  await waitUntil(85);

  // =========================================================================
  //  85-90s  STEP 7+8: click Rollback → Confirm
  // =========================================================================
  rmark('STEP 7 — click Rollback');
  const rollbackBtn = page.locator('button:has(svg.lucide-rotate-ccw-icon)').first();
  let rollbackClicked = false;
  if (await rollbackBtn.isVisible().catch(() => false)) {
    await rollbackBtn.click({ timeout: 2_000 }).catch(() => {});
    rollbackClicked = true;
    rmark('STEP 7 — Rollback clicked');
  } else {
    console.warn('  ! Rollback button not visible');
  }

  rmark('STEP 8 — confirm');
  const confirmBtn = page.getByRole('button', { name: /^(confirm|确认|确定)$/i }).last();
  await confirmBtn.waitFor({ state: 'visible', timeout: 3_000 }).catch(() => {});
  if ((await confirmBtn.count()) > 0) {
    await confirmBtn.click({ timeout: 2_000 }).catch(() => {});
    rmark('STEP 8 — confirmed');
  }
  await waitUntil(90);

  // =========================================================================
  //  90-115s  STEP 9: watch status change on /deployments (25s)
  // =========================================================================
  rmark('STEP 9 — watch status 25s');
  // Click refresh to force frontend re-fetch (auto-refresh is 30s; our abort
  // was just seconds ago — without this the UI stays on Paused).
  const refreshBtn = page.locator('button:has(svg.lucide-refresh-cw-icon)').first();
  if (await refreshBtn.isVisible().catch(() => false)) {
    await refreshBtn.click({ timeout: 2_000 }).catch(() => {});
  }
  // Periodically re-refresh so the UI reflects the new Degraded state
  while (elapsed() < 115) {
    if (elapsed() < 100 && await refreshBtn.isVisible().catch(() => false)) {
      await refreshBtn.click({ timeout: 1_000 }).catch(() => {});
    }
    await page.waitForTimeout(3_000);
  }

  // =========================================================================
  //  115-120s  STEP 10: back to /issues — status should have flipped to RCA Done
  // =========================================================================
  rmark('STEP 10 — back to /issues');
  await page.goto(issuesUrl);
  await page.waitForLoadState('networkidle').catch(() => {});
  // Force one more refetch so the status column reflects latest rca_completed_at
  await page.waitForTimeout(1_500);
  await page.reload().catch(() => {});
  await page.waitForLoadState('networkidle').catch(() => {});
  await waitUntil(120);

  rmark('STOP');
  stopRequested = true;
  await writer;
  await client.send('Page.stopScreencast').catch(() => {});

  const count = fs.readdirSync(FRAMES_DIR).length;
  const duration = elapsed().toFixed(1);
  fs.writeFileSync(
    path.resolve(FRAMES_DIR, '../markers.json'),
    JSON.stringify({ duration_sec: parseFloat(duration), frame_count: count, markers, rollbackClicked }, null, 2),
  );
  console.log(`[recording done] ${count} frames, ${duration}s total.`);
});
