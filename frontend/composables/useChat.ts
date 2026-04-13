/**
 * Chat composable — manages SSE connection to Claude CLI backend.
 * Handles streaming with thinking/text/tool_use/done types.
 * Supports abort, edit+resend, session management, image attachments.
 */

export interface ChatImage {
  data: string       // base64 encoded
  mediaType: string  // image/png, image/jpeg, image/gif, image/webp
  name?: string
}

export interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  type: 'text' | 'thinking' | 'tool_use' | 'tool_result' | 'error'
  toolName?: string
  images?: ChatImage[]
  timestamp: Date
  sessionId?: string
  durationMs?: number
}

interface StreamChunk {
  type: 'init' | 'thinking' | 'text' | 'tool_use' | 'tool_result' | 'done' | 'error'
  content?: string
  session_id?: string
  tool_name?: string
  message?: string
  duration_ms?: number
}

export function useChat() {
  const config = useRuntimeConfig()
  const baseURL = config.public.apiBase || ''

  const messages = ref<ChatMessage[]>([])
  const isStreaming = ref(false)
  const currentSessionId = ref<string | null>(null)
  const error = ref<string | null>(null)
  const selectedProviderId = ref<string | null>(null)
  const selectedMcpServerIds = ref<string[]>([])
  const disabledMcpTools = ref<string[]>([])  // "serverId:toolName" format

  let currentAssistantText = ''
  let currentAssistantId = ''
  let abortController: AbortController | null = null
  let forceNewSession = false

  // Typewriter effect state
  let typewriterDisplayed = 0    // chars already revealed
  let typewriterRafId: number | null = null
  const TYPEWRITER_CHARS_PER_FRAME = 4  // base speed: ~240 chars/sec at 60fps

  function startTypewriter() {
    if (typewriterRafId !== null) return
    const tick = () => {
      const msg = messages.value.find(m => m.id === currentAssistantId && m.type === 'text')
      if (!msg) { stopTypewriter(); return }

      const remaining = currentAssistantText.length - typewriterDisplayed
      if (remaining <= 0) {
        // Stop looping — startTypewriter() will restart when new chunks arrive
        typewriterRafId = null
        return
      }

      // Adaptive speed: catch up faster when buffer grows large
      const speed = Math.max(TYPEWRITER_CHARS_PER_FRAME, Math.floor(remaining / 10))
      typewriterDisplayed = Math.min(typewriterDisplayed + speed, currentAssistantText.length)
      msg.content = currentAssistantText.slice(0, typewriterDisplayed)

      typewriterRafId = requestAnimationFrame(tick)
    }
    typewriterRafId = requestAnimationFrame(tick)
  }

  function stopTypewriter() {
    if (typewriterRafId !== null) {
      cancelAnimationFrame(typewriterRafId)
      typewriterRafId = null
    }
  }

  function flushTypewriter() {
    stopTypewriter()
    const msg = messages.value.find(m => m.id === currentAssistantId && m.type === 'text')
    if (msg) msg.content = currentAssistantText
    typewriterDisplayed = currentAssistantText.length
  }

  function addUserMessage(text: string, images?: ChatImage[]): ChatMessage {
    const msg: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: text,
      type: 'text',
      images: images?.length ? images : undefined,
      timestamp: new Date(),
    }
    messages.value.push(msg)
    return msg
  }

  function findOrCreateAssistantMessage(type: ChatMessage['type'], toolName?: string): ChatMessage {
    if (type === 'text') {
      const existing = messages.value.find(m => m.id === currentAssistantId && m.type === 'text')
      if (existing) return existing

      const msg: ChatMessage = {
        id: crypto.randomUUID(),
        role: 'assistant',
        content: '',
        type: 'text',
        timestamp: new Date(),
      }
      currentAssistantId = msg.id
      messages.value.push(msg)
      return msg
    }

    const msg: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'assistant',
      content: '',
      type,
      toolName,
      timestamp: new Date(),
    }
    messages.value.push(msg)
    return msg
  }

  async function sendMessage(text: string, images?: ChatImage[]) {
    if ((!text.trim() && !images?.length) || isStreaming.value) return

    error.value = null
    addUserMessage(text, images)
    await streamResponse(text, images)
  }

  /** Edit a user message and resend — truncates everything after that message */
  async function editAndResend(messageId: string, newText: string) {
    if (isStreaming.value) return

    const idx = messages.value.findIndex(m => m.id === messageId)
    if (idx === -1) return

    // Truncate: keep messages up to (but not including) the edited one
    messages.value = messages.value.slice(0, idx)
    error.value = null

    addUserMessage(newText)
    await streamResponse(newText)
  }

  /** Abort the current stream */
  function abortStream() {
    if (abortController) {
      abortController.abort()
      abortController = null
    }
    isStreaming.value = false
  }

  async function streamResponse(text: string, images?: ChatImage[]) {
    isStreaming.value = true
    currentAssistantText = ''
    currentAssistantId = ''
    typewriterDisplayed = 0
    stopTypewriter()

    abortController = new AbortController()

    try {
      const payload: Record<string, unknown> = {
        message: text,
        session_id: currentSessionId.value,
        new_session: forceNewSession || undefined,
        provider_id: selectedProviderId.value || undefined,
        mcp_server_ids: selectedMcpServerIds.value.length ? selectedMcpServerIds.value : undefined,
        disabled_mcp_tools: disabledMcpTools.value.length ? disabledMcpTools.value : undefined,
      }
      // Reset the flag after first send
      forceNewSession = false
      if (images?.length) {
        payload.images = images.map(img => ({
          data: img.data,
          media_type: img.mediaType,
          name: img.name,
        }))
      }

      const response = await fetch(`${baseURL}/api/chat`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
        signal: abortController.signal,
      })

      if (!response.ok) {
        const err = await response.json().catch(() => ({ error: response.statusText }))
        throw new Error(err.error || 'Chat request failed')
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
              // ignore
            }
          }
        }
      }
    } catch (err: unknown) {
      if (err instanceof DOMException && err.name === 'AbortError') {
        // User aborted — not an error
        return
      }
      const msg = err instanceof Error ? err.message : 'Unknown error'
      error.value = msg
      messages.value.push({
        id: crypto.randomUUID(),
        role: 'assistant',
        content: msg,
        type: 'error',
        timestamp: new Date(),
      })
    } finally {
      flushTypewriter()
      isStreaming.value = false
      abortController = null
    }
  }

  function handleChunk(chunk: StreamChunk) {
    switch (chunk.type) {
      case 'init':
        if (chunk.session_id) currentSessionId.value = chunk.session_id
        break

      case 'thinking': {
        // Reuse existing thinking message instead of creating new ones
        const existing = messages.value.find(m => m.type === 'thinking' && m.role === 'assistant')
        if (existing) {
          existing.content = chunk.content || ''
        } else {
          const msg = findOrCreateAssistantMessage('thinking')
          msg.content = chunk.content || ''
        }
        break
      }

      case 'text': {
        findOrCreateAssistantMessage('text')
        currentAssistantText += chunk.content || ''
        startTypewriter()
        break
      }

      case 'tool_use': {
        const msg = findOrCreateAssistantMessage('tool_use', chunk.tool_name)
        msg.content = chunk.content || ''
        msg.toolName = chunk.tool_name
        break
      }

      case 'tool_result': {
        // Merge into last tool_use message instead of creating new one
        const lastToolUse = [...messages.value].reverse().find(m => m.type === 'tool_use')
        if (lastToolUse) {
          const result = (chunk.content || '').slice(0, 200)
          if (result) {
            lastToolUse.content += '\n--- Result ---\n' + result
          }
        }
        break
      }

      case 'done':
        if (chunk.session_id) currentSessionId.value = chunk.session_id
        if (!currentAssistantText && chunk.content) {
          const msg = findOrCreateAssistantMessage('text')
          msg.content = chunk.content
        }
        flushTypewriter()
        break

      case 'error':
        error.value = chunk.message || 'Unknown error'
        messages.value.push({
          id: crypto.randomUUID(),
          role: 'assistant',
          content: chunk.message || 'Unknown error',
          type: 'error',
          timestamp: new Date(),
        })
        break
    }
  }

  function clearMessages() {
    stopTypewriter()
    messages.value = []
    currentSessionId.value = null
    currentAssistantText = ''
    currentAssistantId = ''
    typewriterDisplayed = 0
    error.value = null
  }

  function startNewChat() {
    clearMessages()
    forceNewSession = true
  }

  /** Resume an existing session by ID — loads message history from backend */
  async function resumeSession(sessionId: string) {
    currentAssistantText = ''
    currentAssistantId = ''
    error.value = null
    currentSessionId.value = sessionId
    forceNewSession = false

    try {
      const resp = await fetch(`${baseURL}/api/chat/sessions/${encodeURIComponent(sessionId)}/messages`, {
        credentials: 'include',
      })
      if (resp.ok) {
        const rows: Array<{
          id: string
          role: string
          content: string
          msg_type: string
          tool_name: string | null
          images: ChatImage[] | null
          duration_ms: number | null
          created_at: string
        }> = await resp.json()
        messages.value = rows.map(r => ({
          id: r.id,
          role: r.role as 'user' | 'assistant',
          content: r.content,
          type: r.msg_type as ChatMessage['type'],
          toolName: r.tool_name ?? undefined,
          images: r.images ?? undefined,
          timestamp: new Date(r.created_at),
          durationMs: r.duration_ms ?? undefined,
        }))
      } else {
        messages.value = []
      }
    } catch {
      messages.value = []
    }
  }

  return {
    messages: readonly(messages),
    isStreaming: readonly(isStreaming),
    currentSessionId: readonly(currentSessionId),
    error: readonly(error),
    selectedProviderId,
    selectedMcpServerIds,
    disabledMcpTools,
    sendMessage,
    editAndResend,
    abortStream,
    clearMessages,
    startNewChat,
    resumeSession,
  }
}
