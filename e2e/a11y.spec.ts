import { test, expect } from '@playwright/test'
import AxeBuilder from '@axe-core/playwright'

/**
 * Accessibility scan — verifies 0 critical/serious WCAG 2.1 AA violations
 * on every primary user-facing page, across all browser projects.
 *
 * To authenticate, we rely on the seeded admin/admin123 from scripts/local-dev.sh.
 * When running against a preview deployment, override E2E_USERNAME / E2E_PASSWORD.
 */

const USERNAME = process.env.E2E_USERNAME || 'admin'
const PASSWORD = process.env.E2E_PASSWORD || 'admin123'

async function login(page: import('@playwright/test').Page) {
  await page.goto('/login')
  await page.getByLabel(/username|用户名/i).fill(USERNAME).catch(() => {})
  await page.locator('input[name="username"], input[type="text"]').first().fill(USERNAME)
  await page.locator('input[type="password"]').first().fill(PASSWORD)
  await page.getByRole('button', { name: /login|sign in|登录/i }).click()
  // Wait for redirect away from /login
  await page.waitForURL((url) => !url.pathname.startsWith('/login'), { timeout: 15_000 }).catch(() => {})
}

/**
 * Run axe on the current page and assert there are no critical/serious violations.
 * Logs violation details in the test report to help triage.
 */
async function expectNoA11yIssues(page: import('@playwright/test').Page, name: string) {
  // Wait until the page is fully loaded and network-idle so axe sees final DOM.
  await page.waitForLoadState('networkidle').catch(() => {})
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .analyze()

  const blockers = results.violations.filter((v) =>
    v.impact === 'critical' || v.impact === 'serious',
  )

  if (blockers.length > 0) {
    console.log(`\n✗ a11y violations on ${name}:`)
    for (const v of blockers) {
      console.log(`  [${v.impact}] ${v.id}: ${v.help} (${v.nodes.length} node(s))`)
    }
  }

  expect(
    blockers,
    `Expected 0 critical/serious a11y violations on ${name}, got ${blockers.length}`,
  ).toEqual([])
}

test.describe('Accessibility (WCAG 2.1 AA — critical/serious only)', () => {
  test('login page', async ({ page }) => {
    await page.goto('/login')
    await expectNoA11yIssues(page, '/login')
  })

  test('dashboard (index) after login', async ({ page }) => {
    await login(page)
    await page.goto('/')
    await expectNoA11yIssues(page, '/')
  })

  test('issues page', async ({ page }) => {
    await login(page)
    await page.goto('/issues')
    await expectNoA11yIssues(page, '/issues')
  })

  test('deployments page', async ({ page }) => {
    await login(page)
    await page.goto('/deployments')
    await expectNoA11yIssues(page, '/deployments')
  })

  test('topology page', async ({ page }) => {
    await login(page)
    await page.goto('/topology')
    await expectNoA11yIssues(page, '/topology')
  })
})
