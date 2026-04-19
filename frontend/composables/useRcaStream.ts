/**
 * Composable for streaming RCA (Root Cause Analysis) results via SSE.
 * Tracks thinking steps, tool calls (with input/output for Evidence panel),
 * and markdown text — designed for the 3-column Incident Response view.
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
  name: string           // raw tool name (e.g. 'Bash', 'mcp__graphrag__rag_tool')
  label: string          // human-readable Chinese label (auto-refined from input)
  input: string          // JSON stringified tool input (for Evidence panel)
  inputPreview: string   // short preview line for timeline
  output: string         // raw tool_result text
  outputPreview: string  // smart-extracted preview (first few meaningful lines)
  startedAt: number
  durationMs: number
  done: boolean
}

export interface ThinkingStep {
  id: number
  content: string
  at: number
}

/**
 * Best-effort Chinese label based on tool name + input.
 * Falls back to generic "执行工具" for unknown tools.
 */
function deriveLabel(name: string, input: string): string {
  const n = (name || '').toLowerCase()
  const lower = (input || '').toLowerCase()

  if (n.includes('graphrag') && n.includes('rag_tool')) return '查询 Runbook 知识库'
  if (n.includes('graphrag') && n.includes('list')) return '列出知识库'
  if (n.startsWith('mcp__')) return `MCP 工具 · ${name.replace(/^mcp__/, '').replace(/__/g, ' / ')}`

  if (n === 'read') return '读取文件'
  if (n === 'webfetch') return '抓取网页'
  if (n === 'grep') return '搜索代码'
  if (n === 'glob') return '查找文件'

  if (n === 'bash') {
    // Inspect the command to derive a specific label
    // Note: "调用 Argo Rollouts" is reserved for the actual rollback/abort action
    // (via the UI button in Deployments page), not for read-only queries here.
    if (/rollouts?[ \t]+(?:abort|pause|promote|undo|restart|set)/.test(lower)) return '调用 Argo Rollouts'
    if (/kubectl[ \t].*rollouts?\b/.test(lower) || /rollouts?[ \t]+(?:get|list|status)/.test(lower)) return '查询 Rollout 状态'
    if (/kubectl/.test(lower)) return '执行 kubectl'
    if (/curl.*mimir|promql|\/api\/v1\/query/.test(lower)) return '查询 Mimir 指标'
    if (/curl.*loki|logql|\/loki\/api/.test(lower)) return '查询 Loki 日志'
    if (/curl.*tempo|traceql/.test(lower)) return '查询 Tempo 链路'
    if (/curl/.test(lower)) return '调用 HTTP 接口'
    if (/aws[ \t]/.test(lower)) return '执行 AWS CLI'
    return '执行 Shell 命令'
  }
  return name || '工具调用'
}

/** Extract a short preview line from tool input (JSON) for timeline/list display. */
function inputPreview(name: string, input: string): string {
  if (!input) return ''
  try {
    const parsed = JSON.parse(input)
    // Common fields across tools
    if (parsed.command) return String(parsed.command).trim().split('\n')[0].slice(0, 120)
    if (parsed.query) return String(parsed.query).trim().split('\n')[0].slice(0, 120)
    if (parsed.url) return String(parsed.url).slice(0, 120)
    if (parsed.file_path) return String(parsed.file_path).slice(0, 120)
    if (parsed.pattern) return String(parsed.pattern).slice(0, 120)
    // graphrag rag_tool: {context_id, query}
    if (parsed.context_id && parsed.query) return `${parsed.context_id}: ${String(parsed.query).slice(0, 100)}`
    return JSON.stringify(parsed).slice(0, 120)
  } catch {
    return String(input).slice(0, 120)
  }
}

/**
 * Tool-specific smart output extraction.
 * kubectl → find Status / LastState / Reason / Events lines
 * PromQL/curl → extract "result":... value
 * LogQL → first 3 log lines with timestamps
 * rag_tool → first paragraph / first heading
 * default → first 4 non-empty lines
 */
function outputPreview(name: string, input: string, output: string): string {
  if (!output) return ''
  const n = (name || '').toLowerCase()
  const inp = (input || '').toLowerCase()
  const out = String(output)

  // Runbook/rag_tool — keep first markdown chunk
  if (n.includes('graphrag') || n.includes('rag_tool')) {
    // Prefer the first heading + following paragraph
    const headingMatch = out.match(/^#{1,4}\s+.+[\s\S]{0,300}/m)
    if (headingMatch) return headingMatch[0].trim().slice(0, 400)
    return out.slice(0, 400)
  }

  // PromQL (Mimir)
  if (/mimir|promql|\/api\/v1\/query/.test(inp)) {
    // Try to extract "value":[ts,"X"] occurrences
    const vals = [...out.matchAll(/"value"\s*:\s*\[\s*[\d.]+\s*,\s*"([^"]+)"/g)].slice(0, 3)
    if (vals.length) return `结果: ${vals.map((v) => v[1]).join(', ')}`
    // Try __name__ + value pairs
    const statusMatch = out.match(/"status"\s*:\s*"(\w+)"/)
    if (statusMatch) return `status=${statusMatch[1]}  (${out.length} bytes)`
  }

  // LogQL (Loki)
  if (/loki|logql/.test(inp)) {
    // Pull first few log lines — Loki returns [ts, "msg"] tuples
    const lines = [...out.matchAll(/\["(\d+)",\s*"([^"]{10,200})"/g)].slice(0, 3)
    if (lines.length) return lines.map((m) => m[2].replace(/\\n/g, ' ').slice(0, 120)).join('\n')
  }

  // kubectl — look for State/Reason/Events
  if (/kubectl/.test(inp)) {
    const interesting = out
      .split('\n')
      .map((l) => l.trim())
      .filter((l) =>
        /^(?:Name|Status|Reason|State|Last State|Message|Restart Count|Events|Type|Phase):/i.test(l) ||
        /OOMKilled|CrashLoopBackOff|ImagePullBackOff|Evicted|Failed/i.test(l)
      )
      .slice(0, 6)
    if (interesting.length) return interesting.join('\n')
  }

  // Fallback: first 4 non-empty lines
  return out
    .split('\n')
    .map((l) => l.trimEnd())
    .filter((l) => l.trim())
    .slice(0, 4)
    .join('\n')
    .slice(0, 400)
}

export function useRcaStream() {
  const rcaText = ref('')
  const thinkingText = ref('')                     // current thinking (overwritten)
  const thinkingSteps = ref<ThinkingStep[]>([])    // NEW: full thinking history
  const toolCalls = ref<ToolCall[]>([])
  const isStreaming = ref(false)
  const isComplete = ref(false)
  const error = ref<string | null>(null)
  const startedAt = ref<number | null>(null)
  const elapsedMs = ref(0)

  let abortController: AbortController | null = null
  let elapsedTimer: ReturnType<typeof setInterval> | null = null
  let nextToolId = 0
  let nextThinkingId = 0

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
    thinkingSteps.value = []
    toolCalls.value = []
    nextToolId = 0
    nextThinkingId = 0
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
      case 'thinking': {
        const c = (chunk.content || '').trim()
        if (!c) break
        thinkingText.value = c
        // Append to history (dedupe: don't push if identical to the last)
        const last = thinkingSteps.value[thinkingSteps.value.length - 1]
        if (!last || last.content !== c) {
          thinkingSteps.value.push({ id: nextThinkingId++, content: c, at: Date.now() })
        }
        break
      }
      case 'text':
        rcaText.value += chunk.content || ''
        break
      case 'tool_use': {
        const name = chunk.tool_name || 'tool'
        const input = chunk.content || ''
        const label = deriveLabel(name, input)
        toolCalls.value.push({
          id: nextToolId++,
          name,
          label,
          input,
          inputPreview: inputPreview(name, input),
          output: '',
          outputPreview: '',
          startedAt: Date.now(),
          durationMs: 0,
          done: false,
        })
        thinkingText.value = `正在 ${label}...`
        break
      }
      case 'tool_result': {
        // Attach output to the most recent unfinished call
        const list = toolCalls.value
        for (let i = list.length - 1; i >= 0; i--) {
          if (!list[i].done) {
            const output = chunk.content || ''
            list[i] = {
              ...list[i],
              done: true,
              output,
              outputPreview: outputPreview(list[i].name, list[i].input, output),
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
    thinkingSteps.value = []
    toolCalls.value = []
    nextToolId = 0
    nextThinkingId = 0
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
    thinkingSteps,
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
