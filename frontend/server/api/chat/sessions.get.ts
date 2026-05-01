/**
 * Proxy for /api/chat/sessions — forward to Rust backend
 */
export default defineEventHandler(async (event) => {
  const backendUrl = 'http://localhost:8003/api/chat/sessions'
  const cookie = getHeader(event, 'cookie') || ''

  const response = await fetch(backendUrl, {
    method: 'GET',
    headers: {
      'Cookie': cookie,
    },
  })

  if (!response.ok) {
    const err = await response.text().catch(() => 'Backend error')
    throw createError({
      statusCode: response.status,
      statusMessage: err,
    })
  }

  return response.json()
})
