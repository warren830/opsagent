import { chromium } from 'playwright'

const browser = await chromium.launch()

async function run(locale, expectedText, step) {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    locale,
    extraHTTPHeaders: { 'Accept-Language': locale },
  })

  const resp = await fetch('http://localhost:9999/api/auth/login', {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username: 'admin', password: 'admin123' }),
  })
  const { token } = await resp.json()
  await context.addCookies([
    { name: 'token', value: token, url: 'http://localhost:9999', httpOnly: true, sameSite: 'Lax' },
  ])
  await context.addInitScript(t => localStorage.setItem('auth-token', t), token)

  const page = await context.newPage()
  await page.goto('http://localhost:9999/', { waitUntil: 'domcontentloaded' })
  await page.waitForLoadState('networkidle').catch(() => {})
  await page.waitForTimeout(1500)

  const bodyText = await page.locator('body').innerText()
  const has = bodyText.includes(expectedText)
  console.log(`[${step}] locale=${locale}, expected text "${expectedText}" present: ${has}`)
  if (!has) console.log('  first 150:', bodyText.slice(0, 150).replace(/\n/g, '|'))

  await page.screenshot({ path: `i18n-${step}.png`, fullPage: false })
  await context.close()
}

await run('en-US', '欢迎回来', 'en-browser')       // should now show Chinese
await run('zh-CN', '欢迎回来', 'zh-browser')       // should still show Chinese
await browser.close()
