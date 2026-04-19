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

export interface ToolCall {
  id: number
  name: string        // raw tool name (e.g. 'Bash', 'mcp__graphrag__rag_tool')
  label: string       // human-readable Chinese label
  startedAt: number
  durationMs: number
  done: boolean
}

/**
 * Translate a Claude tool name into a concise Chinese label + action verb
 * so RCA demo clearly shows what the agent is doing.
 */
function toolLabel(name: string): string {
  const n = (name || '').toLowerCase()
  if (n === 'bash') return '执行 Shell 命令'
  if (n === 'read') return '读取文件'
  if (n === 'webfetch') return '抓取网页'
  if (n === 'grep') return '搜索代码'
  if (n === 'glob') return '查找文件'
  if (n.includes('graphrag') && n.includes('rag_tool')) return '查询 Runbook 知识库'
  if (n.includes('graphrag') && n.includes('list')) return '列出知识库'
  if (n.includes('kubectl')) return '调用 kubectl'
  if (n.includes('prometheus') || n.includes('mimir')) return '查询 Mimir 指标'
  if (n.includes('loki')) return '查询 Loki 日志'
  if (n.includes('tempo')) return '查询 Tempo 链路'
  if (n.startsWith('mcp__')) return `MCP 工具 · ${name.replace(/^mcp__/, '').replace(/__/g, ' / ')}`
  return name || '工具调用'
}

export function useRcaStream() {
  const rcaText = ref('')
  const thinkingText = ref('')
  const toolCalls = ref<ToolCall[]>([])
  const isStreaming = ref(false)
  const isComplete = ref(false)
  const error = ref<string | null>(null)
  const startedAt = ref<number | null>(null)
  const elapsedMs = ref(0)

  let abortController: AbortController | null = null
  let elapsedTimer: ReturnType<typeof setInterval> | null = null
  let nextToolId = 0

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
    toolCalls.value = []
    nextToolId = 0
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

      if (!isComplete.value && !error.value) {
        isComplete.value = true
      }
    } catch (err: unknown) {
      if (err instanceof DOMException && err.name === 'AbortError') {
        return
      }
      error.value = err instanceof Error ? err.message : 'Unknown error'
    } finally {
      // Mark any still-running tool calls as done
      toolCalls.value = toolCalls.value.map((tc) => (tc.done ? tc : { ...tc, done: true }))
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
      case 'tool_use': {
        const name = chunk.tool_name || 'tool'
        const label = toolLabel(name)
        toolCalls.value.push({
          id: nextToolId++,
          name,
          label,
          startedAt: Date.now(),
          durationMs: 0,
          done: false,
        })
        thinkingText.value = `正在 ${label}...`
        break
      }
      case 'tool_result': {
        // Mark the most recent unfinished tool call as done
        const list = toolCalls.value
        for (let i = list.length - 1; i >= 0; i--) {
          if (!list[i].done) {
            list[i] = {
              ...list[i],
              done: true,
              durationMs: chunk.duration_ms ?? (Date.now() - list[i].startedAt),
            }
            break
          }
        }
        thinkingText.value = ''
        break
      }
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
    toolCalls.value = []
    nextToolId = 0
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
    toolCalls,
    isStreaming,
    isComplete,
    error,
    elapsedMs,
    startRca,
    abort,
    reset,
  }
}
