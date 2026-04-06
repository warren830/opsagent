/**
 * SSE streaming proxy for POST /api/chat
 * Nuxt routeRules proxy buffers the entire response, breaking SSE.
 * This nitro handler streams chunks directly from the Rust backend.
 */
export default defineEventHandler(async (event) => {
  const body = await readBody(event)
  const backendUrl = 'http://localhost:3080/api/chat'

  // Forward cookies for auth
  const cookie = getHeader(event, 'cookie') || ''

  const response = await fetch(backendUrl, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Cookie': cookie,
    },
    body: JSON.stringify(body),
  })

  if (!response.ok) {
    const err = await response.text().catch(() => 'Backend error')
    throw createError({
      statusCode: response.status,
      statusMessage: err,
    })
  }

  // Set SSE headers
  setResponseHeaders(event, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache',
    'Connection': 'keep-alive',
    'X-Accel-Buffering': 'no', // Disable nginx buffering
  })

  // Stream the response body directly
  if (response.body) {
    const reader = response.body.getReader()

    const stream = new ReadableStream({
      async pull(controller) {
        const { done, value } = await reader.read()
        if (done) {
          controller.close()
          return
        }
        controller.enqueue(value)
      },
      cancel() {
        reader.cancel()
      },
    })

    return sendStream(event, stream)
  }

  return ''
})
