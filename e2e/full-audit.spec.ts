// e2e/full-audit.spec.ts
//
// Comprehensive visual + console-error audit across every major route.
// Run with a fresh Nuxt dev server on :9999 (so CSS is clean), backend on :3080.
//
//   cd /Users/ychchen/warren_ws/opsagent/e2e
//   npx playwright test full-audit.spec.ts --project=chromium
//
// Outputs:
//   e2e/screenshots/audit/<slug>.png     full-page screenshots
//   e2e/screenshots/audit/summary.json   { [route]: { errors, warnings, skipped } }

import { test, expect } from '@playwright/test'
import * as fs from 'node:fs'
import * as path from 'node:path'

const BACKEND = 'http://localhost:3080'
const FRONTEND = 'http://localhost:9999'
const OUT_DIR = 'screenshots/audit'

type RouteReport = {
  route: string
  slug: string
  status: 'ok' | 'skipped' | 'error'
  errors: string[]
  warnings: string[]
  loadTimeMs?: number
  finalUrl?: string
  note?: string
}

const routes: Array<{ path: string; auth: boolean; slug?: string }> = [
  { path: '/login', auth: false },
  { path: '/', auth: true, slug: 'home' },
  { path: '/issues', auth: true },
  { path: '/accounts', auth: true },
  { path: '/clusters', auth: true },
  { path: '/topology', auth: true },
  { path: '/resources', auth: true },
  { path: '/telemetry', auth: true },
  { path: '/channels', auth: true },
  { path: '/skills', auth: true },
  { path: '/mcp', auth: true },
  { path: '/knowledge', auth: true },
  { path: '/glossary', auth: true },
  { path: '/deployments', auth: true },
  { path: '/scheduled-jobs', auth: true },
  { path: '/users', auth: true },
  { path: '/tenants', auth: true },
  { path: '/providers', auth: true },
  { path: '/approvals', auth: true },
  { path: '/settings', auth: true },
]

function slugFor(p: string): string {
  if (p === '/') return 'home'
  return p.replace(/^\//, '').replace(/\//g, '-') || 'root'
}

async function loginViaApi(page: import('@playwright/test').Page) {
  const resp = await fetch(`${BACKEND}/api/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username: 'admin', password: 'admin123' }),
  })
  if (!resp.ok) throw new Error(`Login failed: ${resp.status} ${await resp.text()}`)
  const data = await resp.json()
  const token: string = data.token
  await page.context().addCookies([
    {
      name: 'token',
      value: token,
      domain: 'localhost',
      path: '/',
      httpOnly: false,
      secure: false,
      sameSite: 'Lax',
    },
  ])
}

test.describe.configure({ mode: 'serial' })

test.describe('Full frontend audit', () => {
  test.use({ baseURL: FRONTEND })

  test('sweep every route, screenshot + collect console errors', async ({ page }) => {
    test.setTimeout(180_000)

    fs.mkdirSync(OUT_DIR, { recursive: true })
    const reports: RouteReport[] = []

    // Hook the page's console + pageerror streams BEFORE we navigate anywhere.
    let currentErrors: string[] = []
    let currentWarnings: string[] = []
    page.on('pageerror', (err) => {
      currentErrors.push(`[pageerror] ${err.message}`)
    })
    page.on('console', (msg) => {
      const t = msg.type()
      if (t === 'error') {
        currentErrors.push(`[console.error] ${msg.text()}`)
      } else if (t === 'warning') {
        currentWarnings.push(`[console.warn] ${msg.text()}`)
      }
    })
    page.on('requestfailed', (req) => {
      // Ignore HMR/devtools chatter; capture API failures
      const url = req.url()
      if (url.includes('/api/') || url.includes('/ws') || url.startsWith(FRONTEND)) {
        currentErrors.push(`[requestfailed] ${req.method()} ${url} — ${req.failure()?.errorText ?? 'unknown'}`)
      }
    })

    for (const r of routes) {
      const slug = r.slug ?? slugFor(r.path)
      currentErrors = []
      currentWarnings = []

      // Fresh cookie state per route iteration: login only once, but clear stale cookies first.
      if (r.auth) {
        const cookies = await page.context().cookies()
        const hasToken = cookies.find((c) => c.name === 'token')
        if (!hasToken) {
          try {
            await loginViaApi(page)
          } catch (e) {
            reports.push({
              route: r.path,
              slug,
              status: 'error',
              errors: [`Login failed: ${String(e)}`],
              warnings: [],
            })
            continue
          }
        }
      } else {
        // ensure logged-out for /login
        await page.context().clearCookies()
      }

      const started = Date.now()
      let status: RouteReport['status'] = 'ok'
      let note: string | undefined
      let finalUrl: string | undefined

      try {
        const resp = await page.goto(r.path, { waitUntil: 'domcontentloaded', timeout: 20_000 })
        // If the page-level HTTP response was 404 (route doesn't exist), mark skipped.
        if (resp && resp.status() >= 400 && resp.status() !== 401) {
          status = 'skipped'
          note = `HTTP ${resp.status()} on goto`
        }
        try {
          await page.waitForLoadState('networkidle', { timeout: 8_000 })
        } catch {
          // networkidle timeout is not fatal — some pages keep polling.
          currentWarnings.push('[timing] networkidle timeout (8s), continuing')
        }
        await page.waitForTimeout(600)
        finalUrl = page.url()
      } catch (e) {
        status = 'error'
        note = `goto threw: ${String(e)}`
      }

      // Take a screenshot regardless so the human reviewer can see the state.
      const shotPath = path.join(OUT_DIR, `${slug}.png`)
      try {
        await page.screenshot({ path: shotPath, fullPage: true })
      } catch (e) {
        currentErrors.push(`[screenshot] ${String(e)}`)
      }

      reports.push({
        route: r.path,
        slug,
        status,
        errors: [...currentErrors],
        warnings: [...currentWarnings],
        loadTimeMs: Date.now() - started,
        finalUrl,
        note,
      })

      console.log(
        `[audit] ${r.path.padEnd(18)} -> ${status.padEnd(8)} errors=${currentErrors.length} warns=${currentWarnings.length}` +
          (note ? ` note="${note}"` : '')
      )
    }

    // Write summary
    const totals = {
      routes: reports.length,
      withErrors: reports.filter((r) => r.errors.length > 0).length,
      skipped: reports.filter((r) => r.status === 'skipped').length,
      errored: reports.filter((r) => r.status === 'error').length,
    }
    const summary = { generatedAt: new Date().toISOString(), totals, reports }
    fs.writeFileSync(path.join(OUT_DIR, 'summary.json'), JSON.stringify(summary, null, 2))

    console.log('\n[audit] summary:', JSON.stringify(totals))
    console.log('[audit] summary.json ->', path.resolve(OUT_DIR, 'summary.json'))

    // Don't hard-fail the suite based on errors — this is a diagnosis run,
    // not a pass/fail gate. We still assert the sweep produced a summary.
    expect(fs.existsSync(path.join(OUT_DIR, 'summary.json'))).toBeTruthy()
  })
})
