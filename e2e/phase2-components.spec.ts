// e2e/phase2-components.spec.ts
import { test } from '@playwright/test'

/**
 * Phase ② baseline/after visual snapshots of the style-demo reference page.
 *
 * Assumes frontend dev server is running at http://localhost:9999.
 * The style-demo page lives at /style-demo (no auth, standalone layout).
 *
 * Run ONCE before starting Phase ② (dir: baseline-phase2/)
 * Run ONCE at the end of Phase ② (dir: after-phase2/)
 * Compare the two folders side-by-side.
 *
 * Usage:
 *   PHASE2_LABEL=baseline npx playwright test phase2-components.spec.ts --project=chromium
 *   PHASE2_LABEL=after    npx playwright test phase2-components.spec.ts --project=chromium
 */
const LABEL = process.env.PHASE2_LABEL || 'baseline'
const OUT = `screenshots/${LABEL}-phase2`

test.describe('Phase 2 Components snapshots', () => {
  test.use({ viewport: { width: 1280, height: 2400 } })

  test('capture style-demo full page', async ({ page }) => {
    await page.goto('http://localhost:9999/style-demo')
    await page.waitForLoadState('networkidle')
    await page.waitForTimeout(800)
    await page.screenshot({ path: `${OUT}/style-demo.png`, fullPage: true })
  })
})
