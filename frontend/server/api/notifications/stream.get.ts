/**
 * SSE streaming proxy for GET /api/notifications/stream
 *
 * Nitro's generic routeRules proxy buffers responses, which kills SSE.
 * This dedicated server route uses raw res.write() for unbuffered streaming.
 */
export default defineEventHandler(async (event) => {
  const backendUrl = 'http://localhost:8003/api/notifications/stream'
  const cookie = getHeader(event, 'cookie') || ''

  const response = await fetch(backendUrl, {
    method: 'GET',
    headers: { 'Cookie': cookie },
  })

  if (!response.ok) {
    const body = await response.text().catch(() => '{"error":"Backend error"}')
    setResponseStatus(event, response.status)
    setResponseHeader(event, 'content-type', 'application/json')
    return body
  }

  await proxySseResponse(event, response)
})
