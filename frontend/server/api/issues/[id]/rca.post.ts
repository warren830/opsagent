/**
 * SSE streaming proxy for POST /api/issues/:id/rca
 * Same pattern as /api/chat — bypasses Nuxt routeRules buffering.
 */
export default defineEventHandler(async (event) => {
  const id = getRouterParam(event, 'id')
  const backendUrl = `http://localhost:3080/api/issues/${id}/rca`

  const cookie = getHeader(event, 'cookie') || ''

  const response = await fetch(backendUrl, {
    method: 'POST',
    headers: { 'Cookie': cookie },
  })

  if (!response.ok) {
    const err = await response.text().catch(() => 'Backend error')
    throw createError({
      statusCode: response.status,
      statusMessage: err,
    })
  }

  setResponseHeaders(event, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache',
    'Connection': 'keep-alive',
    'X-Accel-Buffering': 'no',
  })

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
