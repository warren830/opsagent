import type { H3Event } from 'h3'

/**
 * Pipe an SSE response from the backend directly to the client,
 * bypassing all h3/Nitro buffering via raw Node.js res.write().
 */
export async function proxySseResponse(event: H3Event, response: Response) {
  const res = event.node.res
  res.writeHead(200, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache, no-transform',
    'Connection': 'keep-alive',
    'X-Accel-Buffering': 'no',
  })
  res.flushHeaders()

  if (response.body) {
    const reader = response.body.getReader()
    try {
      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        res.write(value)
      }
    } catch {
      // Client disconnected — ignore
    } finally {
      reader.cancel().catch(() => {})
    }
  }

  res.end()
  event._handled = true
}
