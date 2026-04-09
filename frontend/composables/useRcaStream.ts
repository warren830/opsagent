/**
 * Composable for streaming RCA (Root Cause Analysis) results via SSE.
 * Mirrors the SSE consumption pattern from useChat.ts but simplified for RCA.
 */

interface StreamChunk {
  type: 'init' | 'thinking' | 'text' | 'tool_use' | 'tool_result' | 'done' | 'error'
  content?: string
  message?: string
  session_id?: string
  tool_name?: string
  duration_ms?: number
}

export function useRcaStream() {
  const rcaText = ref('')
  const thinkingText = ref('')
  const isStreaming = ref(false)
  const isComplete = ref(false)
  const error = ref<string | null>(null)
  const startedAt = ref<number | null>(null)
  const elapsedMs = ref(0)

  let abortController: AbortController | null = null
  let elapsedTimer: ReturnType<typeof setInterval> | null = null

  function startElapsedTimer() {
    startedAt.value = Date.now()
    elapsedTimer = setInterval(() => {
      if (startedAt.value) {
        elapsedMs.value = Date.now() - startedAt.value
      }
    }, 100)
  }

  function stopElapsedTimer() {
    if (elapsedTimer) {
      clearInterval(elapsedTimer)
      elapsedTimer = null
    }
  }

  async function startRca(issueId: string) {
    rcaText.value = ''
    thinkingText.value = ''
    isStreaming.value = true
    isComplete.value = false
    error.value = null
    abortController = new AbortController()

    startElapsedTimer()

    try {
      const response = await fetch(`/api/issues/${issueId}/rca`, {
        method: 'POST',
        credentials: 'include',
        signal: abortController.signal,
      })

      if (!response.ok) {
        throw new Error(await response.text().catch(() => 'RCA request failed'))
      }

      const reader = response.body?.getReader()
      if (!reader) throw new Error('No response stream')

      const decoder = new TextDecoder()
      let buffer = ''

      while (true) {
        const { done, value } = await reader.read()
        if (done) break

        buffer += decoder.decode(value, { stream: true })

        const lines = buffer.split('\n')
        buffer = lines.pop() || ''

        for (const line of lines) {
          const trimmed = line.trim()
          if (!trimmed || trimmed === ':ping') continue

          if (trimmed.startsWith('data: ')) {
            const json = trimmed.slice(6)
            if (!json) continue
            try {
              const chunk: StreamChunk = JSON.parse(json)
              handleChunk(chunk)
            } catch {
              // ignore parse errors
            }
          }
        }
      }

      // Stream ended — mark complete if not already
      if (!isComplete.value && !error.value) {
        isComplete.value = true
      }
    } catch (err: unknown) {
      if (err instanceof DOMException && err.name === 'AbortError') {
        return
      }
      error.value = err instanceof Error ? err.message : 'Unknown error'
    } finally {
      isStreaming.value = false
      abortController = null
      stopElapsedTimer()
    }
  }

  function handleChunk(chunk: StreamChunk) {
    switch (chunk.type) {
      case 'thinking':
        thinkingText.value = chunk.content || ''
        break
      case 'text':
        rcaText.value += chunk.content || ''
        break
      case 'tool_use':
      case 'tool_result':
        // Show tool activity in thinking area
        thinkingText.value = chunk.tool_name ? `Using ${chunk.tool_name}...` : ''
        break
      case 'done':
        isComplete.value = true
        if (!rcaText.value && chunk.content) {
          rcaText.value = chunk.content
        }
        break
      case 'error':
        error.value = chunk.message || 'RCA analysis failed'
        break
    }
  }

  function abort() {
    abortController?.abort()
    isStreaming.value = false
    stopElapsedTimer()
  }

  function reset() {
    rcaText.value = ''
    thinkingText.value = ''
    isStreaming.value = false
    isComplete.value = false
    error.value = null
    startedAt.value = null
    elapsedMs.value = 0
    stopElapsedTimer()
  }

  return {
    rcaText,
    thinkingText,
    isStreaming,
    isComplete,
    error,
    elapsedMs,
    startRca,
    abort,
    reset,
  }
}
