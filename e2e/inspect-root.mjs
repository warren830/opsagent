import { chromium } from 'playwright'

const browser = await chromium.launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })

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
await page.setExtraHTTPHeaders({ 'Accept-Language': 'zh-CN,zh;q=0.9' })

// Log any navigation events
page.on('framenavigated', f => console.log('NAV →', f.url()))

await page.goto('http://localhost:9999/', { waitUntil: 'domcontentloaded' })
await page.waitForLoadState('networkidle').catch(() => {})
await page.waitForTimeout(2000)

console.log('URL before screenshot:', page.url())
await page.screenshot({ path: 'inspect-root-a.png', fullPage: false })
console.log('URL after screenshot:', page.url())

await page.waitForTimeout(500)
await page.screenshot({ path: 'inspect-root-b.png', fullPage: false })
console.log('URL after 2nd screenshot:', page.url())

await browser.close()
