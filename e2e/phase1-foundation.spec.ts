// e2e/phase1-foundation.spec.ts
import { test } from '@playwright/test'

/**
 * Phase ① baseline/after visual snapshots (local dev server).
 *
 * Assumes frontend dev server is running at http://localhost:3000
 * and backend is reachable at http://localhost:3080.
 *
 * Run ONCE before starting Phase ① (dir: baseline-phase1/)
 * Run ONCE at the end of Phase ① (dir: after-phase1/)
 * Compare the two folders side-by-side — this is the phase's "test".
 *
 * Usage:
 *   PHASE1_LABEL=baseline npx playwright test phase1-foundation.spec.ts --project=chromium
 *   PHASE1_LABEL=after    npx playwright test phase1-foundation.spec.ts --project=chromium
 *
 * Note: We inject the JWT cookie directly via API (bypassing the Secure cookie
 * restriction on http:// in local dev) rather than going through the login form.
 */
const LABEL = process.env.PHASE1_LABEL || 'baseline'
const OUT = `screenshots/${LABEL}-phase1`
const BACKEND = 'http://localhost:3080'
const FRONTEND = 'http://localhost:3000'

/**
 * Fetch a JWT token from the backend directly, then inject it as a cookie
 * into the browser context. This avoids the Secure-cookie issue on http://.
 */
async function loginViaApi(page: import('@playwright/test').Page) {
  // Call the backend directly to get the token
  const resp = await fetch(`${BACKEND}/api/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username: 'admin', password: 'admin123' }),
  })
  if (!resp.ok) throw new Error(`Login failed: ${resp.status} ${await resp.text()}`)
  const data = await resp.json()
  const token: string = data.token

  // Inject the token as a non-Secure cookie so the frontend can read it on http://
  await page.context().addCookies([
    {
      name: 'token',
      value: token,
      domain: 'localhost',
      path: '/',
      httpOnly: false, // must be readable by JS in dev
      secure: false,   // http:// local dev
      sameSite: 'Lax',
    },
  ])
}

test.describe('Phase 1 Foundation snapshots', () => {
  test.use({ baseURL: FRONTEND })

  test('capture login page (unauthenticated)', async ({ page }) => {
    await page.goto(`${FRONTEND}/login`)
    await page.waitForLoadState('networkidle')
    await page.waitForTimeout(500)
    await page.screenshot({ path: `${OUT}/login.png`, fullPage: true })
  })

  test('capture dashboard, issues, settings (authenticated)', async ({ page }) => {
    await loginViaApi(page)

    for (const path of ['/', '/issues', '/settings']) {
      await page.goto(path)
      await page.waitForLoadState('networkidle')
      await page.waitForTimeout(800) // wait for skeleton loaders / gradient transitions to settle
      const slug = path === '/' ? 'home' : path.replace(/\//g, '-').replace(/^-/, '')
      await page.screenshot({ path: `${OUT}/${slug}.png`, fullPage: true })
    }
  })
})
