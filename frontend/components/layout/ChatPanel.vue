<script setup lang="ts">
import {
  Send, PanelRightClose, RotateCcw, ChevronDown, ChevronRight, Sparkles,
  AlertCircle, Terminal, Maximize2, Minimize2, Square, Pencil, Check, Paperclip,
  FolderOpen, Trash2, Download, FileText, History, Wrench, X,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription,
} from '@/components/ui/dialog'
import { toast } from 'vue-sonner'
import { marked } from 'marked'
import type { ChatMessage, ChatImage } from '@/composables/useChat'
import PdfViewer from '@/components/ui/PdfViewer.vue'

// Custom renderer: mermaid code blocks → <pre class="mermaid">
// + GraphRAG images → clickable with "view source" overlay
const renderer = new marked.Renderer()
const originalCode = renderer.code.bind(renderer)
renderer.code = function (token: { text: string; lang?: string | null }) {
  if (token.lang === 'mermaid') {
    return `<pre class="mermaid">${token.text}</pre>`
  }
  return originalCode(token)
}

// Custom image renderer: detect GraphRAG S3 images and make them clickable
const originalImage = renderer.image.bind(renderer)
renderer.image = function (token: { href: string; title: string | null; text: string }) {
  const { href, text } = token
  // Detect GraphRAG S3 presigned image URLs
  if (href && href.includes('graphrag-databucket')) {
    // Extract image filename for data attribute (e.g. image_000002.png)
    const imgMatch = href.match(/(image_\d+\.png)/)
    const imgFile = imgMatch ? imgMatch[1] : ''
    return `<span class="graphrag-img-wrap" data-img-file="${imgFile}" data-img-url="${encodeURIComponent(href)}">
      <img src="${href}" alt="${text}" class="graphrag-img" loading="lazy" />
      <span class="graphrag-img-badge">📄 View in PDF</span>
    </span>`
  }
  return originalImage(token)
}

marked.setOptions({ breaks: true, gfm: true, renderer })

const { t } = useI18n()
const chatOpen = useState('chatPanelOpen', () => true)
const chatFullscreen = useState('chatFullscreen', () => false)
const { messages, isStreaming, sendMessage, editAndResend, abortStream, startNewChat, clearMessages, selectedProviderId, selectedMcpServerIds, disabledMcpTools, currentSessionId, resumeSession } = useChat()

// Model selector
interface ProviderOption {
  id: string
  name: string
  provider_type: string
  is_default: boolean
}

const availableProviders = ref<ProviderOption[]>([])
const showModelSelector = computed(() => availableProviders.value.length > 0)

const currentProviderName = computed(() => {
  if (!selectedProviderId.value) {
    const def = availableProviders.value.find(p => p.is_default)
    return def?.name || ''
  }
  const p = availableProviders.value.find(p => p.id === selectedProviderId.value)
  return p?.name || ''
})

async function loadProviders() {
  try {
    const data = await api.get<ProviderOption[]>('/api/providers')
    availableProviders.value = data
    // Auto-select default if nothing selected
    if (!selectedProviderId.value && data.length > 0) {
      const def = data.find(p => p.is_default)
      selectedProviderId.value = def?.id || data[0].id
    }
  } catch { /* ignore */ }
}

// MCP tools selector (tools stored per server from test/discovery)
interface McpToolInfo {
  name: string
  description: string
}
interface McpServerOption {
  id: string
  name: string
  enabled: boolean
  transport_type: string
  tools: McpToolInfo[]
}
const mcpServers = ref<McpServerOption[]>([])
const showMcpPicker = ref(false)
const hasMcpTools = computed(() => mcpServers.value.length > 0)
const mcpSelectedCount = computed(() => selectedMcpServerIds.value.length)
const mcpTotalServers = computed(() => mcpServers.value.length)

function toggleMcpServer(id: string) {
  const idx = selectedMcpServerIds.value.indexOf(id)
  if (idx >= 0) {
    selectedMcpServerIds.value.splice(idx, 1)
    // Also remove disabled tools entries for this server
    const srv = mcpServers.value.find(s => s.id === id)
    if (srv) {
      disabledMcpTools.value = disabledMcpTools.value.filter(t => !t.startsWith(`${id}:`))
    }
  } else {
    selectedMcpServerIds.value.push(id)
  }
}

function isToolEnabled(serverId: string, toolName: string) {
  return !disabledMcpTools.value.includes(`${serverId}:${toolName}`)
}

function toggleMcpTool(serverId: string, toolName: string) {
  const key = `${serverId}:${toolName}`
  const idx = disabledMcpTools.value.indexOf(key)
  if (idx >= 0) {
    disabledMcpTools.value.splice(idx, 1)
  } else {
    disabledMcpTools.value.push(key)
  }
}

async function loadMcpServers() {
  try {
    const data = await api.get<McpServerOption[]>('/api/mcp')
    mcpServers.value = data.filter(s => s.enabled)
    // Default: select all enabled servers
    if (selectedMcpServerIds.value.length === 0 && mcpServers.value.length > 0) {
      selectedMcpServerIds.value = mcpServers.value.map(s => s.id)
    }
  } catch { /* ignore */ }
}

onMounted(() => { loadProviders(); loadSessions(); loadMcpServers() })
watch(chatOpen, (open) => { if (open) { loadProviders(); loadSessions(); loadMcpServers() } })

// Reload providers when navigating back from config pages (e.g. /providers)
const route = useRoute()
watch(() => route.path, () => { if (chatOpen.value) loadProviders() })

const inputText = ref('')
const messagesEnd = ref<HTMLElement>()
const inputRef = ref<HTMLTextAreaElement>()

// PDF Viewer state (for GraphRAG source document viewing)
const pdfViewerOpen = ref(false)
const pdfViewerUrl = ref('')
const pdfViewerPage = ref(1)
const pdfViewerBbox = ref<{ x0: number; y0: number; x1: number; y1: number } | undefined>()
const pdfViewerFileName = ref('')

// ── Citation registry ──────────────────────────────────────
interface CitationMeta {
  chunkId: string
  bboxId: string
  filePath?: string
  page?: number
  bbox?: { x0: number; y0: number; x1: number; y1: number }
}

const citationRegistry = ref<Map<number, CitationMeta>>(new Map())

function buildCitationRegistry() {
  const registry = new Map<number, CitationMeta>()
  let citationNum = 1

  for (const msg of messages.value) {
    // Search all message types — chunk metadata can appear in tool_result, tool_use, or text
    const content = msg.content || ''

    const chunkMatch = content.match(/"chunk_id"\s*:\s*"([^"]+)"/)
      || content.match(/chunk_id[=:]\s*"?([a-f0-9-]+)"?/i)
      || content.match(/(chunk-[a-f0-9]+)/)
    const chunkId = chunkMatch?.[1] || ''

    const bboxPattern = /<BBOX:(bbox_\d+)>/g
    let match: RegExpExecArray | null
    let hasBbox = false

    while ((match = bboxPattern.exec(content)) !== null) {
      hasBbox = true
      const bboxId = match[1]
      const alreadyExists = Array.from(registry.values())
        .some(c => c.chunkId === chunkId && c.bboxId === bboxId)
      if (!alreadyExists && chunkId) {
        registry.set(citationNum++, { chunkId, bboxId })
      }
    }

    // Text-only citation (chunk without BBOX)
    if (chunkId && !hasBbox) {
      registry.set(citationNum++, { chunkId, bboxId: '' })
    }
  }

  citationRegistry.value = registry
}

watch(() => messages.value.length, buildCitationRegistry)
watch(() => messages.value.map(m => m.content).join('').length, buildCitationRegistry)

async function openPdfFromCitation(n: number) {
  const cite = citationRegistry.value.get(n)
  if (!cite) {
    toast.error(`Citation [${n}] not found`)
    return
  }

  // Reuse cached data
  if (cite.page && cite.filePath) {
    try {
      const contextId = 'ops'
      const pdfResp = await api.post<{ url: string }>('/api/graphrag/pdf-url', {
        context_id: contextId,
        file_path: cite.filePath,
      })
      if (!pdfResp.url) { toast.error('Could not get PDF URL'); return }
      pdfViewerUrl.value = pdfResp.url
      pdfViewerPage.value = cite.page
      pdfViewerBbox.value = cite.bbox
      pdfViewerFileName.value = cite.filePath
      pdfViewerOpen.value = true
    } catch (e: any) {
      toast.error(`Failed to open PDF: ${e.message || e}`)
    }
    return
  }

  if (!cite.bboxId) {
    toast.info(`Citation [${n}] — no page location available`)
    return
  }

  try {
    const contextId = 'ops'
    const bboxResp = await api.post<{
      bboxes: Array<{ page: number; bbox: { x0: number; y0: number; x1: number; y1: number }; file_path: string }>
    }>('/api/graphrag/bbox', {
      context_id: contextId,
      requests: [{ chunk_id: cite.chunkId, bbox_id: cite.bboxId }],
    })

    if (!bboxResp.bboxes?.length) {
      toast.error('Could not locate source in PDF')
      return
    }

    const info = bboxResp.bboxes[0]
    cite.page = info.page
    cite.filePath = info.file_path
    cite.bbox = info.bbox

    const pdfResp = await api.post<{ url: string }>('/api/graphrag/pdf-url', {
      context_id: contextId,
      file_path: info.file_path,
    })

    if (!pdfResp.url) { toast.error('Could not get PDF URL'); return }
    pdfViewerUrl.value = pdfResp.url
    pdfViewerPage.value = info.page
    pdfViewerBbox.value = info.bbox
    pdfViewerFileName.value = info.file_path
    pdfViewerOpen.value = true
  } catch (e: any) {
    toast.error(`Failed to open citation [${n}]: ${e.message || e}`)
  }
}

async function openPdfFromGraphRag(imgFile: string, imgUrl?: string) {
  // imgFile = e.g. "image_000002.png"
  // imgUrl = full S3 presigned URL of the image (artifacts/<hash>/image_N.png)
  //
  // Strategy:
  // 1. Find chunk_id+bbox_id from message content → bbox API → pdf-url API
  // 2. If no chunk found, extract artifact hash from S3 URL → search for any
  //    chunk_id in messages → use bbox API to get the real file_path
  // 3. Final fallback: use GraphRAG proxy to list documents

  try {
    const contextId = 'ops' // TODO: make configurable per MCP server

    // Helper: given bbox response, open PDF viewer
    const openFromBbox = async (bboxInfo: { page: number; bbox: { x0: number; y0: number; x1: number; y1: number }; file_path: string }) => {
      const pdfResp = await api.post<{ url: string }>('/api/graphrag/pdf-url', {
        context_id: contextId,
        file_path: bboxInfo.file_path,
      })
      if (pdfResp.url) {
        pdfViewerUrl.value = pdfResp.url
        pdfViewerPage.value = bboxInfo.page
        pdfViewerBbox.value = bboxInfo.bbox
        pdfViewerFileName.value = bboxInfo.file_path
        pdfViewerOpen.value = true
        return true
      }
      return false
    }

    // Strategy 1: Find chunk_id + bbox_id for this specific image
    const imageChunks = findImageChunkInfo(imgFile)
    if (imageChunks) {
      console.log(`[GraphRAG] Found chunk info: chunk=${imageChunks.chunkId}, bbox=${imageChunks.bboxId}`)
      try {
        const bboxResp = await api.post<{ bboxes: Array<{ page: number; bbox: { x0: number; y0: number; x1: number; y1: number }; file_path: string }> }>(
          '/api/graphrag/bbox',
          { context_id: contextId, requests: [{ chunk_id: imageChunks.chunkId, bbox_id: imageChunks.bboxId }] }
        )
        if (bboxResp.bboxes?.length) {
          if (await openFromBbox(bboxResp.bboxes[0])) return
        }
      } catch (e) {
        console.warn('[GraphRAG] Strategy 1 bbox call failed:', e)
      }
    }

    // Strategy 2: Use ANY chunk_id from citation registry or messages to get file_path via bbox API
    // Then use that file_path for pdf-url (image won't have bbox highlight, but PDF opens correctly)
    console.log('[GraphRAG] Strategy 2: finding any chunk_id from messages...')
    const anyChunk = findAnyChunkId()
    if (anyChunk) {
      console.log(`[GraphRAG] Using chunk_id=${anyChunk.chunkId}, bbox_id=${anyChunk.bboxId} to discover file_path`)
      try {
        const bboxResp = await api.post<{ bboxes: Array<{ page: number; bbox: any; file_path: string }> }>(
          '/api/graphrag/bbox',
          { context_id: contextId, requests: [{ chunk_id: anyChunk.chunkId, bbox_id: anyChunk.bboxId }] }
        )
        if (bboxResp.bboxes?.length) {
          const filePath = bboxResp.bboxes[0].file_path
          console.log(`[GraphRAG] Discovered file_path="${filePath}" via bbox API`)
          const pdfResp = await api.post<{ url: string }>('/api/graphrag/pdf-url', {
            context_id: contextId,
            file_path: filePath,
          })
          if (pdfResp.url) {
            pdfViewerUrl.value = pdfResp.url
            pdfViewerPage.value = 1
            pdfViewerBbox.value = undefined
            pdfViewerFileName.value = filePath
            pdfViewerOpen.value = true
            return
          }
        }
      } catch (e) {
        console.warn('[GraphRAG] Strategy 2 failed:', e)
      }
    }

    // Strategy 3: List documents from GraphRAG, match artifact hash from S3 URL to find the exact PDF
    // S3 URL: artifacts/<hash>/image_N.png → document s3_key: uploads/<ctx>/<date>/<hash>/<filename>
    console.log('[GraphRAG] Strategy 3: listing documents from GraphRAG...')
    try {
      const docs = await api.get<Array<{ file_name: string; s3_key: string; status: string }>>(`/api/graphrag/documents/${contextId}`)
      const pdfDocs = (docs || []).filter(d => d.file_name?.endsWith('.pdf') && (d.status === 'SUCCEEDED' || d.status === 'COMPLETED'))
      console.log(`[GraphRAG] Found ${pdfDocs.length} PDF documents`)

      // Extract artifact hash from image S3 URL to match against document s3_key
      let matchedDoc: { file_name: string; s3_key: string } | undefined
      if (imgUrl) {
        const artifactMatch = new URL(imgUrl).pathname.match(/\/artifacts\/([a-f0-9]+)\//)
        if (artifactMatch) {
          const artifactHash = artifactMatch[1]
          console.log(`[GraphRAG] Matching artifact hash "${artifactHash}" against ${pdfDocs.length} documents`)
          matchedDoc = pdfDocs.find(d => d.s3_key?.includes(artifactHash))
          if (matchedDoc) {
            console.log(`[GraphRAG] Matched: "${matchedDoc.file_name}" (s3_key contains ${artifactHash})`)
          }
        }
      }

      // If no match by hash, fallback to first/only PDF
      if (!matchedDoc && pdfDocs.length > 0) {
        matchedDoc = pdfDocs[0]
        console.log(`[GraphRAG] No hash match, using first PDF: "${matchedDoc.file_name}"`)
      }

      if (matchedDoc) {
        // Try s3_key first (direct path), then file_name (DynamoDB lookup + fallback)
        for (const filePath of [matchedDoc.s3_key, matchedDoc.file_name]) {
          if (!filePath) continue
          try {
            const pdfResp = await api.post<{ url: string }>('/api/graphrag/pdf-url', {
              context_id: contextId,
              file_path: filePath,
            })
            if (pdfResp.url) {
              console.log(`[GraphRAG] Strategy 3 success: file_path="${filePath}"`)
              pdfViewerUrl.value = pdfResp.url
              pdfViewerPage.value = 1
              pdfViewerBbox.value = undefined
              pdfViewerFileName.value = matchedDoc.file_name
              pdfViewerOpen.value = true
              return
            }
          } catch {
            // try next path variant
          }
        }
      }
    } catch (e) {
      console.warn('[GraphRAG] Strategy 3 documents API failed:', e)
    }

    console.warn('[GraphRAG] All strategies exhausted for image:', imgFile, 'url:', imgUrl)
    toast.error('Could not locate source document')
  } catch (e: any) {
    toast.error(`Failed to open PDF: ${e.message || e}`)
  }
}

// Find ANY chunk_id + bbox_id from messages or citation registry (not image-specific)
// Used as fallback to discover file_path via bbox API
function findAnyChunkId(): { chunkId: string; bboxId: string } | null {
  // First try citation registry — it's already parsed
  if (citationRegistry.value.size > 0) {
    const first = citationRegistry.value.values().next().value
    if (first?.chunkId) {
      return { chunkId: first.chunkId, bboxId: first.bboxId || '' }
    }
  }

  // Search all messages for chunk_id patterns
  for (const msg of messages.value) {
    const content = msg.content || ''

    // Pattern 1: JSON "chunk_id": "value"
    const jsonMatch = content.match(/"chunk_id"\s*:\s*"([^"]+)"/)
    if (jsonMatch) {
      const bboxMatch = content.match(/<BBOX:(bbox_\w+)>/)
      return { chunkId: jsonMatch[1], bboxId: bboxMatch?.[1] || '' }
    }

    // Pattern 2: [[CITE:chunk_id:bbox_id]] citation format in Claude's text output
    const citeMatch = content.match(/\[\[CITE:([^:\]]+):(bbox_\w+)\]\]/)
    if (citeMatch) {
      return { chunkId: citeMatch[1], bboxId: citeMatch[2] }
    }

    // Pattern 3: chunk-<hex> anywhere
    const chunkHexMatch = content.match(/\b(chunk-[a-f0-9]{8,})\b/)
    if (chunkHexMatch) {
      const bboxMatch = content.match(/<BBOX:(bbox_\w+)>/)
      return { chunkId: chunkHexMatch[1], bboxId: bboxMatch?.[1] || '' }
    }
  }

  return null
}

// Parse assistant messages to find chunk_id + bbox_id for a given image filename
function findImageChunkInfo(imgFile: string): { chunkId: string; bboxId: string } | null {
  const escapedFile = imgFile.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

  // Search ALL message types — Claude CLI may send chunk metadata in tool_use, tool_result, or text blocks
  // Strategy 1: Find BBOX + chunk_id in the same message that contains the image filename
  for (const msg of messages.value) {
    const content = msg.content || ''
    if (!content.includes(imgFile)) continue

    // Flexible BBOX pattern: <BBOX:bbox_N> anywhere before the image filename
    const bboxPattern = new RegExp(`<BBOX:(bbox_\\w+)>[\\s\\S]{0,500}${escapedFile}`, 'i')
    const bboxMatch = content.match(bboxPattern)

    // Flexible chunk_id pattern: accepts any chunk ID format
    const chunkMatch = content.match(/"chunk_id"\s*:\s*"([^"]+)"/)
      || content.match(/chunk_id[=:]\s*"?([a-f0-9-]+)"?/i)
      || content.match(/(chunk-[a-f0-9]+)/)

    if (bboxMatch && chunkMatch) {
      return { chunkId: chunkMatch[1], bboxId: bboxMatch[1] }
    }

    // Even without BBOX, if we have chunk_id and the image is in this message
    if (chunkMatch) {
      const anyBbox = content.match(/<BBOX:(bbox_\w+)>/)
      return { chunkId: chunkMatch[1], bboxId: anyBbox?.[1] || '' }
    }
  }

  // Strategy 2: Image is in one message, chunk metadata in a nearby previous message
  let lastChunkId = ''
  let lastBboxId = ''
  for (const msg of messages.value) {
    const content = msg.content || ''

    // Collect chunk_id from ANY message type (tool_result, tool_use, text, etc.)
    const chunkMatch = content.match(/"chunk_id"\s*:\s*"([^"]+)"/)
      || content.match(/chunk_id[=:]\s*"?([a-f0-9-]+)"?/i)
      || content.match(/(chunk-[a-f0-9]+)/)
    if (chunkMatch) {
      lastChunkId = chunkMatch[1]
      const bboxMatch = content.match(/<BBOX:(bbox_\w+)>/)
      lastBboxId = bboxMatch?.[1] || ''
    }

    if (content.includes(imgFile) && lastChunkId) {
      return { chunkId: lastChunkId, bboxId: lastBboxId }
    }
  }

  // Strategy 3: Broadest — image in ANY message, chunk_id in ANY other message
  for (const msg of messages.value) {
    const content = msg.content || ''
    if (!content.includes(imgFile)) continue

    for (const other of messages.value) {
      const otherContent = other.content || ''
      const chunkMatch = otherContent.match(/"chunk_id"\s*:\s*"([^"]+)"/)
        || otherContent.match(/chunk_id[=:]\s*"?([a-f0-9-]+)"?/i)
        || otherContent.match(/(chunk-[a-f0-9]+)/)
      if (chunkMatch) {
        const bboxMatch = otherContent.match(/<BBOX:(bbox_\w+)>/)
        console.warn(`[GraphRAG] Fallback match for ${imgFile}: chunk=${chunkMatch[1]}, bbox=${bboxMatch?.[1] || 'none'}, msg_type=${other.type}`)
        return { chunkId: chunkMatch[1], bboxId: bboxMatch?.[1] || '' }
      }
    }
  }

  console.warn(`[GraphRAG] Could not find chunk info for image: ${imgFile}. Messages:`, messages.value.map(m => m.type))
  return null
}

// Accept pre-filled prompts from other pages (e.g., Security Insights → Chat with Agent)
const chatPrefill = useState<string>('chatPrefill', () => '')
watch(chatPrefill, (val) => {
  if (val) {
    inputText.value = val
    chatPrefill.value = ''
    nextTick(() => inputRef.value?.focus())
  }
})

// Expand/collapse state
const expandedThinking = ref<Set<string>>(new Set())
const expandedTools = ref<Set<string>>(new Set())

// Edit state
const editingId = ref<string | null>(null)
const editText = ref('')

// Image attachment state
const pendingImages = ref<ChatImage[]>([])
const fileInputRef = ref<HTMLInputElement>()

const ACCEPTED_IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp']
const MAX_IMAGE_SIZE = 10 * 1024 * 1024 // 10MB

// Slash command state
const showSlashMenu = ref(false)
const slashFilter = ref('')
const slashSelectedIdx = ref(0)

const api = useApi()

// --- Sessions ---
interface ChatSessionInfo {
  id: string
  claude_session_id: string
  title: string | null
  last_active_at: string
}
const recentSessions = ref<ChatSessionInfo[]>([])
const showSessionPicker = ref(false)

async function loadSessions() {
  try {
    recentSessions.value = await api.get<ChatSessionInfo[]>('/api/chat/sessions')
  } catch { recentSessions.value = [] }
}

function selectSession(s: ChatSessionInfo) {
  resumeSession(s.claude_session_id)
  showSessionPicker.value = false
  toast.success(t('chat.sessionResumed'))
}

function formatSessionTime(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return t('chat.justNow')
  if (mins < 60) return `${mins}m`
  return `${Math.floor(mins / 60)}h`
}

interface SlashCommand {
  name: string
  label: string
  description: string
  hasArg?: boolean
}

const baseCommands: SlashCommand[] = [
  { name: 'clear', label: '/clear', description: t('chat.slashClear') },
  { name: 'new', label: '/new', description: t('chat.slashNew') },
  { name: 'model', label: '/model', description: t('chat.slashModel'), hasArg: true },
]

const skillCommands = ref<SlashCommand[]>([])

// Global counter — incremented by skills page on install/delete/update
const skillsVersion = useState('skillsVersion', () => 0)

// Load skills for slash commands
async function loadSkills() {
  try {
    const skills = await api.get<{ name: string; description: string }[]>('/api/skills')
    skillCommands.value = skills.map(s => ({
      name: `skill:${s.name}`,
      label: `/skill ${s.name}`,
      description: s.description || s.name,
    }))
  } catch { /* ignore */ }
}

onMounted(loadSkills)

// Refresh skill commands when chat panel opens or skills change
watch(chatOpen, (open) => { if (open) loadSkills() })
watch(skillsVersion, () => loadSkills())

const filteredCommands = computed(() => {
  const all = [...baseCommands, ...skillCommands.value]
  if (!slashFilter.value) return all
  const q = slashFilter.value.toLowerCase()
  return all.filter(c => c.label.toLowerCase().includes(q) || c.description.toLowerCase().includes(q))
})

const quickHints = [
  'List EKS clusters',
  'Check pending pods',
  'Show S3 buckets',
  'Recent issues',
]

// --- Workspace ---
interface WorkspaceFile {
  name: string
  size: number
  modified: string
  is_dir: boolean
}
interface WorkspaceDir {
  name: string
  files: WorkspaceFile[]
  expanded: boolean
}
const showWorkspace = ref(false)
const workspaceFiles = ref<WorkspaceFile[]>([])
const workspaceLoading = ref(false)
const expandedDirs = ref<Set<string>>(new Set())

const workspaceTree = computed(() => {
  const dirs = new Map<string, WorkspaceFile[]>()
  const rootFiles: WorkspaceFile[] = []
  for (const f of workspaceFiles.value) {
    const slashIdx = f.name.indexOf('/')
    if (slashIdx > 0) {
      const dirName = f.name.substring(0, slashIdx)
      if (!dirs.has(dirName)) dirs.set(dirName, [])
      dirs.get(dirName)!.push(f)
    } else {
      rootFiles.push(f)
    }
  }
  const result: WorkspaceDir[] = []
  for (const [name, files] of dirs) {
    result.push({ name, files: files.sort((a, b) => b.modified.localeCompare(a.modified)), expanded: expandedDirs.value.has(name) })
  }
  result.sort((a, b) => b.name.localeCompare(a.name))
  return { dirs: result, rootFiles: rootFiles.sort((a, b) => b.modified.localeCompare(a.modified)) }
})

function toggleDir(name: string) {
  const s = new Set(expandedDirs.value)
  if (s.has(name)) s.delete(name); else s.add(name)
  expandedDirs.value = s
}

async function openWorkspace() {
  showWorkspace.value = true
  workspaceLoading.value = true
  try {
    workspaceFiles.value = await api.get<WorkspaceFile[]>('/api/chat/workspace')
  } catch { workspaceFiles.value = [] }
  finally { workspaceLoading.value = false }
}

async function downloadFile(name: string) {
  try {
    const encodedPath = name.split('/').map(encodeURIComponent).join('/')
    const blob = await api.getBlob(`/api/chat/workspace/${encodedPath}`)
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = name.split('/').pop() || name
    a.click()
    URL.revokeObjectURL(url)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function deleteFile(name: string) {
  try {
    const encodedPath = name.split('/').map(encodeURIComponent).join('/')
    await api.del(`/api/chat/workspace/${encodedPath}`)
    workspaceFiles.value = workspaceFiles.value.filter(f => f.name !== name)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function deleteDir(dirName: string) {
  try {
    await api.del(`/api/chat/workspace/${encodeURIComponent(dirName)}`)
    workspaceFiles.value = workspaceFiles.value.filter(f => !f.name.startsWith(dirName + '/'))
    expandedDirs.value.delete(dirName)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

// --- Actions ---

function close() {
  chatOpen.value = false
  chatFullscreen.value = false
  showSessionPicker.value = false
}

// Close dropdowns on click outside
const mcpPickerRef = ref<HTMLElement>()
const sessionPickerRef = ref<HTMLElement>()

function onDocClick(e: MouseEvent) {
  const target = e.target as HTMLElement

  // Handle citation badge clicks → open PDF viewer at cited location
  const citeBadge = target.closest('.citation-badge') as HTMLElement
  if (citeBadge) {
    e.preventDefault()
    e.stopPropagation()
    const citeN = Number.parseInt(citeBadge.dataset.citeN || '', 10)
    if (citeN) openPdfFromCitation(citeN)
    return
  }

  // Handle GraphRAG image clicks → open PDF viewer
  const imgWrap = target.closest('.graphrag-img-wrap') as HTMLElement
  if (imgWrap) {
    e.preventDefault()
    e.stopPropagation()
    const imgFile = imgWrap.dataset.imgFile
    const imgUrl = imgWrap.dataset.imgUrl ? decodeURIComponent(imgWrap.dataset.imgUrl) : ''
    if (imgFile) openPdfFromGraphRag(imgFile, imgUrl)
    return
  }

  if (showSessionPicker.value && sessionPickerRef.value && !sessionPickerRef.value.contains(target)) {
    showSessionPicker.value = false
  }
  if (showMcpPicker.value && mcpPickerRef.value && !mcpPickerRef.value.contains(target)) {
    showMcpPicker.value = false
  }
  if (showSlashMenu.value) {
    // Close slash menu if click is outside the input area
    const textarea = inputRef.value
    if (textarea && !textarea.contains(target) && !target.closest('.slash-menu')) {
      showSlashMenu.value = false
    }
  }
}
// Cmd+K / Ctrl+K to toggle chat panel
function onGlobalKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
    e.preventDefault()
    chatOpen.value = !chatOpen.value
  }
}

onMounted(() => { document.addEventListener('click', onDocClick); document.addEventListener('keydown', onGlobalKeydown) })
onUnmounted(() => { document.removeEventListener('click', onDocClick); document.removeEventListener('keydown', onGlobalKeydown) })

function toggleFullscreen() {
  chatFullscreen.value = !chatFullscreen.value
}

async function send() {
  const text = inputText.value.trim()
  const images = pendingImages.value.length > 0 ? [...pendingImages.value] : undefined
  if (!text && !images?.length) return
  if (isStreaming.value) return

  // Handle slash commands
  if (text.startsWith('/') && !images?.length) {
    const handled = handleSlashCommand(text)
    if (handled) {
      inputText.value = ''
      showSlashMenu.value = false
      return
    }
  }

  inputText.value = ''
  pendingImages.value = []
  showSlashMenu.value = false
  await sendMessage(text || 'Describe this image.', images)
  scrollToBottom()
}

function handleSlashCommand(text: string): boolean {
  const parts = text.split(/\s+/)
  const cmd = parts[0].toLowerCase()

  if (cmd === '/clear') { clearMessages(); return true }
  if (cmd === '/new') { startNewChat(); return true }
  if (cmd === '/model' && parts[1]) {
    // Prepend model instruction to next message
    inputText.value = `[Use model: ${parts.slice(1).join(' ')}] `
    return true
  }
  if (cmd.startsWith('/skill') && parts[1]) {
    // Skill is auto-injected via --add-dir, just prompt the user
    const skillName = parts.slice(1).join(' ')
    const skill = skillCommands.value.find(s => s.name === `skill:${skillName}`)
    inputText.value = ''
    sendMessage(`Use the ${skillName} skill. ${skill?.description || ''}`)
    return true
  }
  return false
}

function sendHint(hint: string) {
  inputText.value = hint
  send()
}

function startEdit(msg: ChatMessage) {
  editingId.value = msg.id
  editText.value = msg.content
}

function cancelEdit() {
  editingId.value = null
  editText.value = ''
}

async function confirmEdit(msgId: string) {
  const text = editText.value.trim()
  if (!text) return
  editingId.value = null
  editText.value = ''
  await editAndResend(msgId, text)
  scrollToBottom()
}

function handleKeydown(e: KeyboardEvent) {
  // Slash menu navigation
  if (showSlashMenu.value) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      slashSelectedIdx.value = Math.min(slashSelectedIdx.value + 1, filteredCommands.value.length - 1)
      return
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      slashSelectedIdx.value = Math.max(slashSelectedIdx.value - 1, 0)
      return
    }
    if (e.key === 'Tab' || (e.key === 'Enter' && filteredCommands.value.length > 0)) {
      e.preventDefault()
      const cmd = filteredCommands.value[slashSelectedIdx.value]
      if (cmd) {
        inputText.value = cmd.label + (cmd.hasArg ? ' ' : '')
        showSlashMenu.value = false
        if (!cmd.hasArg) send()
      }
      return
    }
    if (e.key === 'Escape') {
      showSlashMenu.value = false
      inputText.value = ''
      return
    }
  }

  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
    e.preventDefault()
    send()
  }
}

function handleInput() {
  const text = inputText.value
  if (text === '/') {
    showSlashMenu.value = true
    slashFilter.value = ''
    slashSelectedIdx.value = 0
  } else if (text.startsWith('/') && !text.includes(' ')) {
    showSlashMenu.value = true
    slashFilter.value = text
    slashSelectedIdx.value = 0
  } else {
    showSlashMenu.value = false
  }
}

function selectSlashCommand(cmd: SlashCommand) {
  inputText.value = cmd.label + (cmd.hasArg ? ' ' : '')
  showSlashMenu.value = false
  if (!cmd.hasArg) {
    send()
  } else {
    inputRef.value?.focus()
  }
}

// --- Image handling ---

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.onerror = reject
    reader.readAsDataURL(file)
  })
}

async function addImageFiles(files: FileList | File[]) {
  for (const file of Array.from(files)) {
    if (!ACCEPTED_IMAGE_TYPES.includes(file.type)) continue
    if (file.size > MAX_IMAGE_SIZE) continue
    const dataUrl = await fileToBase64(file)
    // dataUrl = "data:image/png;base64,xxxxx"
    const base64 = dataUrl.split(',')[1]
    if (!base64) continue
    pendingImages.value.push({
      data: base64,
      mediaType: file.type,
      name: file.name,
    })
  }
}

function openFilePicker() {
  fileInputRef.value?.click()
}

async function handleFileSelect(e: Event) {
  const input = e.target as HTMLInputElement
  if (input.files?.length) {
    await addImageFiles(input.files)
    input.value = '' // reset so same file can be re-selected
  }
}

function removeImage(idx: number) {
  pendingImages.value.splice(idx, 1)
}

async function handlePaste(e: ClipboardEvent) {
  const items = e.clipboardData?.items
  if (!items) return

  const imageFiles: File[] = []
  for (const item of Array.from(items)) {
    if (item.type.startsWith('image/')) {
      const file = item.getAsFile()
      if (file) imageFiles.push(file)
    }
  }
  if (imageFiles.length > 0) {
    e.preventDefault()
    await addImageFiles(imageFiles)
  }
}

function toggleThinking(id: string) {
  if (expandedThinking.value.has(id)) { expandedThinking.value.delete(id) } else { expandedThinking.value.add(id) }
}

function toggleTool(id: string) {
  if (expandedTools.value.has(id)) { expandedTools.value.delete(id) } else { expandedTools.value.add(id) }
}

// Find the tool_result message that follows a given tool_use message
function getToolResult(toolUseMsg: ChatMessage): string | null {
  const idx = messages.value.indexOf(toolUseMsg)
  if (idx < 0) return null
  // Look at the next few messages for a matching tool_result
  for (let i = idx + 1; i < Math.min(idx + 3, messages.value.length); i++) {
    if (messages.value[i].type === 'tool_result') {
      return messages.value[i].content || null
    }
    // Stop if we hit another tool_use or text (result should be right after)
    if (messages.value[i].type === 'tool_use' || messages.value[i].type === 'text') break
  }
  return null
}

function renderMd(text: string): string {
  if (!text) return ''
  let html = marked.parse(text) as string

  // Post-process: convert [n] citation markers to clickable badges
  // Avoid matching markdown links [text](url) or content inside tags/attributes
  if (citationRegistry.value.size > 0) {
    html = html.replace(
      /(?<!["\w/])(\[(\d{1,3})\])(?!\()/g,
      (_full, _bracket, num) => {
        const n = Number.parseInt(num, 10)
        if (citationRegistry.value.has(n)) {
          return `<sup class="citation-badge" data-cite-n="${n}" title="Source [${n}]">${n}</sup>`
        }
        return _full
      },
    )
  }

  return html
}

function truncate(text: string, len: number): string {
  return text.length > len ? text.slice(0, len) + '...' : text
}

function scrollToBottom() {
  nextTick(() => messagesEnd.value?.scrollIntoView({ behavior: 'smooth' }))
}

// Mermaid rendering (lazy load to avoid SSR issues)
let mermaidLoaded = false
async function renderMermaid() {
  await nextTick()
  if (!mermaidLoaded) {
    try {
      const m = await import('mermaid')
      m.default.initialize({
        startOnLoad: false,
        theme: 'dark',
        themeVariables: { darkMode: false, background: '#ffffff', primaryColor: '#3b82f6', primaryTextColor: '#0f172a', lineColor: '#94a3b8', fontSize: '12px' },
      })
      mermaidLoaded = true
    } catch { return }
  }
  try {
    const m = await import('mermaid')
    await m.default.run({ querySelector: '.chat-markdown .mermaid' })
  } catch { /* ignore incomplete mermaid during streaming */ }
}

// Watchers
watch(() => messages.value.length, () => { scrollToBottom(); renderMermaid() })
watch(() => {
  const last = messages.value[messages.value.length - 1]
  return last?.content?.length || 0
}, () => { if (isStreaming.value) scrollToBottom() })
watch(isStreaming, (val) => { if (!val) renderMermaid() })

// Resize
const panelWidth = ref(420)
const isResizing = ref(false)

function startResize(e: MouseEvent) {
  isResizing.value = true
  const startX = e.clientX
  const startWidth = panelWidth.value
  function onMouseMove(e: MouseEvent) {
    panelWidth.value = Math.max(320, Math.min(800, startWidth + (startX - e.clientX)))
  }
  function onMouseUp() {
    isResizing.value = false
    document.removeEventListener('mousemove', onMouseMove)
    document.removeEventListener('mouseup', onMouseUp)
  }
  document.addEventListener('mousemove', onMouseMove)
  document.addEventListener('mouseup', onMouseUp)
}
</script>

<template>
  <Transition
    enter-active-class="transition-all duration-200 ease-out"
    leave-active-class="transition-all duration-150 ease-in"
    enter-from-class="translate-x-full opacity-0"
    enter-to-class="translate-x-0 opacity-100"
    leave-from-class="translate-x-0 opacity-100"
    leave-to-class="translate-x-full opacity-0"
  >
    <aside
      v-if="chatOpen"
      class="flex flex-col border-l border-border/60 bg-background relative"
      :class="chatFullscreen ? 'flex-1' : ''"
      style="height: 100%;"
      :style="chatFullscreen ? {} : { width: `${panelWidth}px` }"
    >
      <!-- Resize handle (side panel mode only) -->
      <div
        v-if="!chatFullscreen"
        class="absolute left-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-primary/40 active:bg-primary/60 transition-colors z-10"
        @mousedown="startResize"
      />

      <!-- Header -->
      <div class="flex items-center justify-between px-3 h-10 border-b border-border/60 shrink-0 bg-card/50">
        <div class="flex items-center gap-1.5">
          <Sparkles class="h-3.5 w-3.5 text-primary" />
          <span class="font-medium text-xs">{{ t('chat.title') }}</span>
          <!-- Model selector -->
          <Select v-if="showModelSelector" v-model="selectedProviderId">
            <SelectTrigger class="ml-1.5 h-5 w-auto gap-1 border-border/40 bg-secondary/30 px-1.5 text-[10px] text-muted-foreground"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem v-for="p in availableProviders" :key="p.id" :value="p.id">{{ p.name }}</SelectItem>
            </SelectContent>
          </Select>
          <span v-else-if="currentProviderName" class="text-[10px] text-muted-foreground/50 ml-1">{{ currentProviderName }}</span>
        </div>
        <div class="flex items-center gap-0.5">
          <!-- MCP Tools picker -->
          <div v-if="hasMcpTools" ref="mcpPickerRef" class="relative">
            <Button
              variant="ghost" size="sm"
              class="h-6 rounded px-1.5 gap-1"
              :class="showMcpPicker ? 'bg-accent text-foreground' : 'text-muted-foreground hover:text-foreground'"
              :title="t('chat.mcpTools')"
              @click.stop="showMcpPicker = !showMcpPicker"
            >
              <Wrench class="h-3 w-3" />
              <span class="text-[10px]">{{ mcpSelectedCount }}/{{ mcpTotalServers }}</span>
            </Button>
            <Transition enter-active-class="transition-all duration-150" enter-from-class="opacity-0 -translate-y-1" leave-active-class="transition-all duration-100" leave-to-class="opacity-0 -translate-y-1">
              <div v-if="showMcpPicker" class="absolute right-0 top-8 w-64 z-50 rounded-lg border border-border/60 bg-card shadow-lg py-1 max-h-72 overflow-y-auto">
                <div class="px-2 py-1 text-[10px] text-muted-foreground/60 font-medium uppercase tracking-wider">{{ t('chat.mcpTools') }}</div>
                <!-- Tools grouped by server -->
                <template v-for="srv in mcpServers" :key="srv.id">
                  <!-- Server toggle row -->
                  <button
                    class="w-full flex items-center gap-2 px-2 py-1.5 text-xs hover:bg-accent/50 transition-colors"
                    @click="toggleMcpServer(srv.id)"
                  >
                    <div
class="h-3.5 w-3.5 rounded border flex items-center justify-center shrink-0"
                      :class="selectedMcpServerIds.includes(srv.id) ? 'bg-primary border-primary' : 'border-border'">
                      <Check v-if="selectedMcpServerIds.includes(srv.id)" class="h-2.5 w-2.5 text-primary-foreground" />
                    </div>
                    <span class="font-medium truncate">{{ srv.name }}</span>
                    <span class="text-[10px] text-muted-foreground/40 ml-auto">{{ srv.tools?.length || 0 }} tools</span>
                  </button>
                  <!-- Tool list under this server -->
                  <div v-if="srv.tools?.length && selectedMcpServerIds.includes(srv.id)" class="pl-5 pr-2 space-y-0">
                    <button
                      v-for="tool in srv.tools"
                      :key="tool.name"
                      class="w-full flex items-center gap-1.5 py-0.5 text-[11px] hover:bg-accent/30 rounded px-1 transition-colors"
                      :class="isToolEnabled(srv.id, tool.name) ? 'text-muted-foreground/70' : 'text-muted-foreground/30'"
                      :title="tool.description"
                      @click.stop="toggleMcpTool(srv.id, tool.name)"
                    >
                      <div
class="h-3 w-3 rounded border flex items-center justify-center shrink-0"
                        :class="isToolEnabled(srv.id, tool.name) ? 'bg-primary/80 border-primary/80' : 'border-border/50'">
                        <Check v-if="isToolEnabled(srv.id, tool.name)" class="h-2 w-2 text-primary-foreground" />
                      </div>
                      <Terminal class="h-2.5 w-2.5 shrink-0 text-muted-foreground/40" />
                      <span class="font-mono truncate">{{ tool.name }}</span>
                    </button>
                  </div>
                </template>
              </div>
            </Transition>
          </div>
          <div ref="sessionPickerRef" class="relative">
            <Button
              variant="ghost" size="sm"
              class="h-6 w-6 p-0 rounded"
              :class="showSessionPicker ? 'bg-accent text-foreground' : 'text-muted-foreground hover:text-foreground'"
              :title="t('chat.sessions')"
              @click.stop="showSessionPicker = !showSessionPicker; if (showSessionPicker) loadSessions()"
            >
              <History class="h-3 w-3" />
            </Button>
            <!-- Session dropdown -->
            <Transition
              enter-active-class="transition-all duration-150 ease-out"
              leave-active-class="transition-all duration-100 ease-in"
              enter-from-class="opacity-0 scale-95"
              enter-to-class="opacity-100 scale-100"
              leave-from-class="opacity-100 scale-100"
              leave-to-class="opacity-0 scale-95"
            >
              <div
                v-if="showSessionPicker"
                class="absolute right-0 top-full mt-1 w-56 bg-card border border-border/60 rounded-lg shadow-lg z-30 overflow-hidden"
              >
                <div class="px-2.5 py-1.5 border-b border-border/40 flex items-center justify-between">
                  <span class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60">{{ t('chat.recentSessions') }}</span>
                  <span class="text-[9px] text-muted-foreground/40">24h</span>
                </div>
                <div class="max-h-48 overflow-y-auto">
                  <div v-if="recentSessions.length === 0" class="px-3 py-4 text-center text-[11px] text-muted-foreground/50">
                    {{ t('chat.noSessions') }}
                  </div>
                  <button
                    v-for="s in recentSessions"
                    :key="s.id"
                    class="w-full flex items-center gap-2 px-2.5 py-1.5 text-left transition-colors hover:bg-accent/50"
                    :class="currentSessionId === s.claude_session_id ? 'bg-primary/10 text-primary' : 'text-foreground'"
                    @click="selectSession(s)"
                  >
                    <div class="min-w-0 flex-1">
                      <div class="text-[11px] truncate">{{ s.title || s.claude_session_id.slice(0, 8) }}</div>
                    </div>
                    <span class="text-[9px] text-muted-foreground/50 shrink-0">{{ formatSessionTime(s.last_active_at) }}</span>
                  </button>
                </div>
              </div>
            </Transition>
          </div>
          <Button variant="ghost" size="sm" class="h-6 w-6 p-0 text-muted-foreground hover:text-foreground" :title="t('chat.newChat')" @click="startNewChat">
            <RotateCcw class="h-3 w-3" />
          </Button>
          <Button variant="ghost" size="sm" class="h-6 w-6 p-0 text-muted-foreground hover:text-foreground" :title="t('chat.workspace')" @click="openWorkspace">
            <FolderOpen class="h-3 w-3" />
          </Button>
          <Button variant="ghost" size="sm" class="h-6 w-6 p-0 text-muted-foreground hover:text-foreground" :title="chatFullscreen ? t('chat.minimize') : t('chat.maximize')" @click="toggleFullscreen">
            <Minimize2 v-if="chatFullscreen" class="h-3 w-3" />
            <Maximize2 v-else class="h-3 w-3" />
          </Button>
          <Button variant="ghost" size="sm" class="h-6 w-6 p-0 text-muted-foreground hover:text-foreground" @click="close">
            <PanelRightClose class="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      <!-- Messages -->
      <div class="flex-1 overflow-y-auto">
        <!-- Welcome -->
        <div v-if="messages.length === 0" class="flex flex-col items-center justify-center min-h-full text-center px-6 py-12">
          <div class="w-9 h-9 rounded-full bg-gradient-to-br from-primary/20 to-primary/5 flex items-center justify-center mb-3 ring-1 ring-primary/10">
            <Sparkles class="h-4 w-4 text-primary" />
          </div>
          <p class="text-xs text-muted-foreground mb-5">{{ t('chat.welcome') }}</p>
          <div class="flex flex-wrap justify-center gap-1.5">
            <button
              v-for="hint in quickHints"
              :key="hint"
              class="text-[11px] px-2.5 py-1 rounded-full border border-border/50 text-muted-foreground hover:text-foreground hover:border-primary/30 hover:bg-primary/5 transition-all"
              @click="sendHint(hint)"
            >
              {{ hint }}
            </button>
          </div>
        </div>

        <!-- Messages list -->
        <div v-else class="py-3 space-y-1" :class="chatFullscreen ? 'max-w-5xl mx-auto px-6' : 'px-3'">
          <template v-for="msg in messages" :key="msg.id">

            <!-- ========== User ========== -->
            <div v-if="msg.role === 'user'" class="flex justify-end py-1 group">
              <!-- Edit mode -->
              <div v-if="editingId === msg.id" class="w-full space-y-1.5">
                <textarea
                  v-model="editText"
                  rows="2"
                  class="w-full resize-none rounded-lg bg-secondary/40 px-3 py-2 text-[13px] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/40 border-0 leading-relaxed"
                  @keydown.enter.prevent="confirmEdit(msg.id)"
                  @keydown.escape="cancelEdit"
                />
                <div class="flex justify-end gap-1">
                  <Button variant="ghost" size="sm" class="h-6 px-2 text-[11px]" @click="cancelEdit">
                    {{ t('common.cancel') }}
                  </Button>
                  <Button size="sm" class="h-6 px-2 text-[11px]" @click="confirmEdit(msg.id)">
                    {{ t('chat.resend') }}
                  </Button>
                </div>
              </div>
              <!-- Normal display -->
              <div v-else class="flex items-start gap-1 max-w-[85%]">
                <Button
                  variant="ghost" size="sm"
                  class="h-5 w-5 p-0 text-muted-foreground/0 group-hover:text-muted-foreground/50 hover:!text-foreground transition-all shrink-0 mt-0.5"
                  :title="t('common.edit')"
                  @click="startEdit(msg)"
                >
                  <Pencil class="h-2.5 w-2.5" />
                </Button>
                <div class="space-y-1.5">
                  <!-- Image thumbnails -->
                  <div v-if="msg.images?.length" class="flex flex-wrap gap-1 justify-end">
                    <img
                      v-for="(img, imgIdx) in msg.images"
                      :key="imgIdx"
                      :src="`data:${img.mediaType};base64,${img.data}`"
                      :alt="img.name || 'image'"
                      class="h-20 max-w-[160px] rounded-lg object-cover border border-border/30 cursor-pointer hover:opacity-80 transition-opacity"
                    />
                  </div>
                  <div class="rounded-2xl rounded-br-sm bg-primary/15 px-3 py-1.5 text-[13px] leading-relaxed text-foreground">
                    {{ msg.content }}
                  </div>
                </div>
              </div>
            </div>

            <!-- ========== Thinking ========== -->
            <div v-else-if="msg.type === 'thinking'" class="py-0.5">
              <button
                class="inline-flex items-center gap-1 text-[11px] text-muted-foreground/80 hover:text-foreground transition-colors"
                @click="toggleThinking(msg.id)"
              >
                <ChevronRight v-if="!expandedThinking.has(msg.id)" class="h-3 w-3 transition-transform" />
                <ChevronDown v-else class="h-3 w-3 transition-transform" />
                <span class="italic">{{ t('chat.thinking') }}</span>
                <span v-if="!expandedThinking.has(msg.id)" class="text-muted-foreground/50 ml-1 max-w-[200px] truncate">{{ truncate(msg.content, 60) }}</span>
              </button>
              <Transition
                enter-active-class="transition-all duration-200 ease-out"
                leave-active-class="transition-all duration-150 ease-in"
                enter-from-class="opacity-0 max-h-0"
                enter-to-class="opacity-100 max-h-[200px]"
                leave-from-class="opacity-100 max-h-[200px]"
                leave-to-class="opacity-0 max-h-0"
              >
                <div
                  v-if="expandedThinking.has(msg.id)"
                  class="mt-1 ml-4 text-[11px] text-muted-foreground leading-relaxed border-l-2 border-border/50 pl-2.5 max-h-[200px] overflow-y-auto"
                >
                  {{ msg.content }}
                </div>
              </Transition>
            </div>

            <!-- ========== Text (markdown) ========== -->
            <div v-else-if="msg.type === 'text'" class="py-1 overflow-x-auto">
              <div
                class="chat-markdown text-[13px] leading-relaxed text-foreground/90"
                v-html="renderMd(msg.content)"
              />
            </div>

            <!-- ========== Tool use (with merged result) ========== -->
            <div v-else-if="msg.type === 'tool_use'" class="py-0.5">
              <button
                class="inline-flex items-center gap-1.5 text-[11px] transition-colors"
                :class="getToolResult(msg) ? 'text-success/70 hover:text-success' : 'text-muted-foreground hover:text-foreground'"
                @click="toggleTool(msg.id)"
              >
                <Check v-if="getToolResult(msg)" class="h-3 w-3 shrink-0" />
                <Terminal v-else class="h-3 w-3 shrink-0 text-warning animate-pulse" />
                <span class="font-mono">{{ msg.toolName || 'tool' }}</span>
                <ChevronRight v-if="!expandedTools.has(msg.id)" class="h-2.5 w-2.5 transition-transform" />
                <ChevronDown v-else class="h-2.5 w-2.5 transition-transform" />
              </button>
              <div v-if="expandedTools.has(msg.id)" class="mt-1 ml-4 space-y-1">
                <div
                  v-if="msg.content"
                  class="text-[11px] font-mono text-foreground/80 bg-muted/20 rounded px-2.5 py-2 max-h-40 overflow-y-auto border border-border/30 leading-relaxed whitespace-pre-wrap"
                >{{ msg.content }}</div>
                <div
                  v-if="getToolResult(msg)"
                  class="text-[11px] font-mono text-foreground/60 bg-muted/10 rounded px-2.5 py-2 max-h-40 overflow-y-auto border border-border/20 leading-relaxed whitespace-pre-wrap"
                >{{ getToolResult(msg)!.length > 1000 ? getToolResult(msg)!.slice(0, 1000) + '\n...' : getToolResult(msg) }}</div>
              </div>
            </div>

            <!-- ========== Tool result (hidden — merged into tool_use above) ========== -->
            <template v-else-if="msg.type === 'tool_result'" />

            <!-- ========== Error ========== -->
            <div v-else-if="msg.type === 'error'" class="py-1">
              <div class="inline-flex items-center gap-1.5 text-[11px] text-destructive/70 bg-destructive/5 rounded px-2 py-1 border border-destructive/10">
                <AlertCircle class="h-3 w-3 shrink-0" />
                <span>{{ msg.content }}</span>
              </div>
            </div>

          </template>

          <!-- Streaming indicator — neko cat -->
          <div v-if="isStreaming" class="py-2 pl-1">
            <div class="neko-thinking inline-flex items-end gap-2">
              <svg class="neko-cat" width="24" height="24" viewBox="0 0 64 64" fill="none">
                <!-- ears -->
                <path d="M14 28 L10 8 L24 20 Z" fill="currentColor" class="text-primary/70" />
                <path d="M50 28 L54 8 L40 20 Z" fill="currentColor" class="text-primary/70" />
                <path d="M16 26 L13 12 L23 21 Z" fill="currentColor" class="text-primary/20" />
                <path d="M48 26 L51 12 L41 21 Z" fill="currentColor" class="text-primary/20" />
                <!-- head -->
                <ellipse cx="32" cy="36" rx="20" ry="18" fill="currentColor" class="text-primary/70" />
                <!-- eyes -->
                <ellipse class="neko-eye-l" cx="24" cy="34" rx="2.5" ry="3" fill="currentColor" />
                <ellipse class="neko-eye-r" cx="40" cy="34" rx="2.5" ry="3" fill="currentColor" />
                <!-- nose -->
                <ellipse cx="32" cy="40" rx="1.5" ry="1" fill="currentColor" class="text-primary/40" />
                <!-- mouth -->
                <path d="M28 42 Q32 46 36 42" stroke="currentColor" class="text-primary/30" stroke-width="1.2" fill="none" />
                <!-- whiskers -->
                <line x1="6" y1="36" x2="20" y2="38" stroke="currentColor" class="text-primary/25" stroke-width="0.8" />
                <line x1="6" y1="42" x2="20" y2="41" stroke="currentColor" class="text-primary/25" stroke-width="0.8" />
                <line x1="44" y1="38" x2="58" y2="36" stroke="currentColor" class="text-primary/25" stroke-width="0.8" />
                <line x1="44" y1="41" x2="58" y2="42" stroke="currentColor" class="text-primary/25" stroke-width="0.8" />
              </svg>
              <span class="text-[10px] text-muted-foreground/50 font-mono neko-text">thinking...</span>
            </div>
          </div>
          <div ref="messagesEnd" />
        </div>
      </div>

      <!-- Input area -->
      <div class="border-t border-border/60 p-2 shrink-0 bg-card/30 relative">
        <!-- Slash command menu -->
        <Transition
          enter-active-class="transition-all duration-150 ease-out"
          leave-active-class="transition-all duration-100 ease-in"
          enter-from-class="opacity-0 translate-y-1"
          enter-to-class="opacity-100 translate-y-0"
          leave-from-class="opacity-100 translate-y-0"
          leave-to-class="opacity-0 translate-y-1"
        >
          <div
            v-if="showSlashMenu && filteredCommands.length > 0"
            class="slash-menu absolute bottom-full left-2 right-2 mb-1 bg-card border border-border/60 rounded-lg shadow-lg overflow-hidden z-20 max-h-48 overflow-y-auto"
          >
            <button
              v-for="(cmd, idx) in filteredCommands"
              :key="cmd.name"
              class="w-full flex items-center gap-2 px-3 py-1.5 text-left transition-colors"
              :class="idx === slashSelectedIdx ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/50'"
              @click="selectSlashCommand(cmd)"
              @mouseenter="slashSelectedIdx = idx"
            >
              <span class="text-xs font-mono text-primary/60">{{ cmd.label }}</span>
              <span class="text-[10px] text-muted-foreground/50 ml-auto truncate max-w-[140px]">{{ cmd.description }}</span>
            </button>
          </div>
        </Transition>

        <!-- Hidden file input -->
        <input
          ref="fileInputRef"
          type="file"
          accept="image/png,image/jpeg,image/gif,image/webp"
          multiple
          class="hidden"
          @change="handleFileSelect"
        />

        <!-- Image preview strip -->
        <div v-if="pendingImages.length > 0" class="flex gap-1.5 mb-1.5 overflow-x-auto" :class="chatFullscreen ? 'max-w-3xl mx-auto' : ''">
          <div
            v-for="(img, idx) in pendingImages"
            :key="idx"
            class="relative group shrink-0"
          >
            <img
              :src="`data:${img.mediaType};base64,${img.data}`"
              :alt="img.name || 'image'"
              class="h-14 w-14 rounded-md object-cover border border-border/40"
            />
            <Button
              variant="destructive" size="sm"
              class="absolute -top-1 -right-1 h-4 w-4 p-0 rounded-full opacity-0 group-hover:opacity-100 transition-opacity"
              @click="removeImage(idx)"
            >
              <X class="h-2.5 w-2.5" />
            </Button>
          </div>
        </div>

        <!-- Animated gradient border wrapper -->
        <div class="relative rounded-xl p-[1.5px]" :class="[chatFullscreen ? 'max-w-3xl mx-auto' : '', isStreaming ? 'chat-input-glow' : 'chat-input-idle']">
          <div class="flex gap-1.5 items-end rounded-[10px] bg-card/95 backdrop-blur-sm px-1.5 py-1.5">
            <!-- Attach button -->
            <Button
              variant="ghost" size="sm"
              class="h-8 w-8 p-0 shrink-0 rounded-lg text-muted-foreground/50 hover:text-foreground"
              title="Attach image"
              @click="openFilePicker"
            >
              <Paperclip class="h-3.5 w-3.5" />
            </Button>
            <textarea
              ref="inputRef"
              v-model="inputText"
              :placeholder="t('chat.placeholder')"
              rows="1"
              class="flex-1 resize-none rounded-lg bg-transparent px-3 py-2 text-[13px] placeholder:text-muted-foreground/40 focus-visible:outline-none border-0 transition-colors leading-relaxed"
              @keydown="handleKeydown"
              @input="handleInput"
              @paste="handlePaste"
            />
            <!-- Send or Stop button -->
            <Button
              v-if="!isStreaming"
              size="icon"
              class="h-8 w-8 shrink-0 rounded-lg"
              :disabled="!inputText.trim() && pendingImages.length === 0"
              @click="send"
            >
              <Send class="h-3.5 w-3.5" />
            </Button>
            <Button
              v-else
              size="icon"
              variant="outline"
              class="h-8 w-8 shrink-0 rounded-lg border-destructive/30 text-destructive hover:bg-destructive/10"
              @click="abortStream"
            >
              <Square class="h-3 w-3" />
            </Button>
          </div>
        </div>
        <div class="text-center mt-1">
          <span class="text-[10px] text-muted-foreground/30">{{ t('chat.slashHint') }}</span>
        </div>
      </div>
    </aside>
  </Transition>

  <!-- Workspace Dialog -->
  <Dialog :open="showWorkspace" @update:open="(v) => showWorkspace = v">
    <DialogContent class="max-w-md">
      <DialogHeader>
        <DialogTitle class="flex items-center gap-1.5 text-sm">
          <FolderOpen class="h-3.5 w-3.5" />
          {{ t('chat.workspace') }}
        </DialogTitle>
        <DialogDescription class="text-[11px]">
          {{ t('chat.workspaceDesc') }}
        </DialogDescription>
      </DialogHeader>
      <div class="max-h-[400px] overflow-y-auto">
        <div v-if="workspaceLoading" class="text-center py-6 text-muted-foreground text-xs">
          {{ t('common.loading') }}
        </div>
        <div v-else-if="workspaceFiles.length === 0" class="text-center py-6 text-muted-foreground/60 text-xs">
          {{ t('common.noData') }}
        </div>
        <div v-else class="space-y-0.5">
          <!-- Directories (collapsible) -->
          <div v-for="dir in workspaceTree.dirs" :key="dir.name">
            <!-- Dir header -->
            <div
              class="flex items-center justify-between px-2.5 py-1.5 rounded hover:bg-accent/50 transition-colors cursor-pointer group"
              @click="toggleDir(dir.name)"
            >
              <div class="flex items-center gap-1.5 min-w-0 flex-1">
                <ChevronRight class="h-3 w-3 text-muted-foreground/60 shrink-0 transition-transform" :class="dir.expanded ? 'rotate-90' : ''" />
                <FolderOpen class="h-3 w-3 text-primary/70 shrink-0" />
                <span class="text-xs font-mono text-foreground truncate">{{ dir.name }}</span>
                <span class="text-[10px] text-muted-foreground/50">{{ dir.files.length }}</span>
              </div>
              <button
                class="p-0.5 rounded text-destructive/50 hover:text-destructive hover:bg-destructive/10 opacity-0 group-hover:opacity-100 transition-all shrink-0"
                :title="t('common.delete')"
                @click.stop="deleteDir(dir.name)"
              >
                <Trash2 class="h-3 w-3" />
              </button>
            </div>
            <!-- Dir children -->
            <Transition name="collapse">
              <div v-if="dir.expanded" class="pl-5 space-y-0.5">
                <div
                  v-for="f in dir.files"
                  :key="f.name"
                  class="flex items-center justify-between px-2.5 py-1 rounded hover:bg-accent/50 transition-colors group"
                >
                  <div class="flex items-center gap-1.5 min-w-0 flex-1 cursor-pointer" @click="downloadFile(f.name)">
                    <FileText class="h-3 w-3 text-muted-foreground/50 shrink-0" />
                    <span class="text-[11px] font-mono text-foreground truncate">{{ f.name.split('/').pop() }}</span>
                    <span class="text-[10px] text-muted-foreground/40 shrink-0">{{ formatSize(f.size) }}</span>
                  </div>
                  <div class="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
                    <button class="p-0.5 rounded hover:bg-accent" :title="t('chat.download')" @click="downloadFile(f.name)">
                      <Download class="h-3 w-3 text-muted-foreground/60" />
                    </button>
                    <button class="p-0.5 rounded text-destructive/50 hover:text-destructive hover:bg-destructive/10" :title="t('common.delete')" @click="deleteFile(f.name)">
                      <Trash2 class="h-3 w-3" />
                    </button>
                  </div>
                </div>
              </div>
            </Transition>
          </div>
          <!-- Root files (not in a folder) -->
          <div
            v-for="f in workspaceTree.rootFiles"
            :key="f.name"
            class="flex items-center justify-between px-2.5 py-1.5 rounded hover:bg-accent/50 transition-colors group"
          >
            <div class="flex items-center gap-1.5 min-w-0 flex-1 cursor-pointer" @click="downloadFile(f.name)">
              <FileText class="h-3 w-3 text-muted-foreground/50 shrink-0" />
              <span class="text-xs font-mono text-foreground truncate">{{ f.name }}</span>
              <span class="text-[10px] text-muted-foreground/40 shrink-0">{{ formatSize(f.size) }}</span>
            </div>
            <div class="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
              <button class="p-0.5 rounded hover:bg-accent" :title="t('chat.download')" @click="downloadFile(f.name)">
                <Download class="h-3 w-3 text-muted-foreground/60" />
              </button>
              <button class="p-0.5 rounded text-destructive/50 hover:text-destructive hover:bg-destructive/10" :title="t('common.delete')" @click="deleteFile(f.name)">
                <Trash2 class="h-3 w-3" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </DialogContent>
  </Dialog>

  <!-- PDF Viewer (GraphRAG source document) -->
  <PdfViewer
    :open="pdfViewerOpen"
    :pdf-url="pdfViewerUrl"
    :page="pdfViewerPage"
    :bbox="pdfViewerBbox"
    :file-name="pdfViewerFileName"
    @update:open="pdfViewerOpen = $event"
  />
</template>

<style>
/* Markdown */
.chat-markdown p { margin: 0 0 0.4em; }
.chat-markdown p:last-child { margin-bottom: 0; }
.chat-markdown strong { font-weight: 600; color: hsl(var(--foreground)); }
.chat-markdown code {
  font-size: 0.8em;
  background: hsl(var(--secondary));
  padding: 0.1em 0.35em;
  border-radius: 3px;
  font-family: ui-monospace, 'Cascadia Code', 'Source Code Pro', Menlo, monospace;
}
.chat-markdown pre {
  background: hsl(var(--secondary));
  border: 1px solid hsl(var(--border) / 0.4);
  border-radius: 6px;
  padding: 0.5em 0.75em;
  margin: 0.4em 0;
  overflow-x: auto;
  font-size: 0.78em;
  line-height: 1.5;
}
.chat-markdown pre code { background: none; padding: 0; font-size: inherit; }
.chat-markdown ul, .chat-markdown ol { margin: 0.3em 0; padding-left: 1.4em; }
.chat-markdown li { margin: 0.15em 0; }
.chat-markdown h1, .chat-markdown h2, .chat-markdown h3 { font-weight: 600; margin: 0.7em 0 0.25em; color: hsl(var(--foreground)); }
.chat-markdown h1 { font-size: 1.15em; padding-bottom: 0.2em; border-bottom: 1px solid hsl(var(--border) / 0.3); }
.chat-markdown h2 { font-size: 1.02em; padding-left: 0.5em; border-left: 3px solid hsl(var(--primary) / 0.7); }
.chat-markdown h3 { font-size: 0.95em; color: hsl(var(--foreground) / 0.85); }
.chat-markdown a { color: hsl(var(--primary)); text-decoration: underline; text-underline-offset: 2px; }
.chat-markdown blockquote { border-left: 2px solid hsl(var(--border)); padding-left: 0.6em; margin: 0.3em 0; color: hsl(var(--muted-foreground)); }
.chat-markdown table {
  border-collapse: collapse;
  margin: 0.5em 0;
  font-size: 0.85em;
  width: 100%;
  border: 1px solid hsl(var(--border) / 0.3);
  border-radius: 6px;
  overflow: hidden;
}
.chat-markdown thead { background: hsl(var(--secondary) / 0.8); }
.chat-markdown th {
  padding: 0.4em 0.65em;
  text-align: left;
  font-weight: 600;
  font-size: 0.9em;
  color: hsl(var(--foreground) / 0.9);
  border-bottom: 1px solid hsl(var(--border) / 0.5);
  white-space: nowrap;
}
.chat-markdown td {
  padding: 0.35em 0.65em;
  border-bottom: 1px solid hsl(var(--border) / 0.2);
  color: hsl(var(--foreground) / 0.8);
}
.chat-markdown tbody tr:nth-child(even) { background: hsl(var(--secondary) / 0.25); }
.chat-markdown tbody tr:hover { background: hsl(var(--primary) / 0.06); }
.chat-markdown th + th,
.chat-markdown td + td { border-left: 1px solid hsl(var(--border) / 0.15); }
/* GraphRAG clickable images */
.graphrag-img-wrap {
  display: inline-block;
  position: relative;
  cursor: pointer;
  border-radius: 6px;
  overflow: hidden;
  transition: all 0.2s;
  border: 1px solid transparent;
}
.graphrag-img-wrap:hover {
  border-color: hsl(var(--primary) / 0.5);
  box-shadow: 0 0 12px hsl(var(--primary) / 0.2);
}
.graphrag-img {
  max-width: 100%;
  max-height: 400px;
  border-radius: 5px;
  display: block;
}
.graphrag-img-badge {
  position: absolute;
  bottom: 6px;
  right: 6px;
  background: hsl(var(--card) / 0.9);
  backdrop-filter: blur(4px);
  border: 1px solid hsl(var(--border) / 0.5);
  color: hsl(var(--primary));
  font-size: 10px;
  font-weight: 500;
  padding: 2px 8px;
  border-radius: 4px;
  opacity: 0;
  transition: opacity 0.15s;
  pointer-events: none;
}
.graphrag-img-wrap:hover .graphrag-img-badge {
  opacity: 1;
}

/* ── Citation badges ── */
.citation-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  margin: 0 1px;
  font-size: 10px;
  font-weight: 600;
  line-height: 1;
  color: hsl(var(--primary));
  background: hsl(var(--primary) / 0.12);
  border: 1px solid hsl(var(--primary) / 0.25);
  border-radius: 3px;
  cursor: pointer;
  vertical-align: super;
  transition: all 0.15s ease;
  font-family: ui-monospace, 'Cascadia Code', monospace;
  user-select: none;
}
.citation-badge:hover {
  background: hsl(var(--primary) / 0.25);
  border-color: hsl(var(--primary) / 0.5);
  box-shadow: 0 0 6px hsl(var(--primary) / 0.2);
  transform: translateY(-1px);
}
.citation-badge:active {
  transform: translateY(0);
  background: hsl(var(--primary) / 0.3);
}

/* Mermaid — scrollable for large diagrams */
.chat-markdown pre.mermaid {
  background: hsl(var(--secondary) / 0.3);
  border: 1px solid hsl(var(--border) / 0.2);
  border-radius: 8px;
  padding: 0.75em;
  margin: 0.5em 0;
  text-align: left;
  overflow-x: auto;
}
.chat-markdown pre.mermaid svg { height: auto; min-width: min-content; }
.chat-markdown pre.mermaid svg text { fill: hsl(var(--foreground) / 0.9) !important; }

/* Images in markdown — scrollable */
.chat-markdown img { max-height: 600px; border-radius: 6px; }

/* Chat input border */
.chat-input-idle {
  background: linear-gradient(135deg, #3b82f6, #60a5fa, #c4b5fd, #8b5cf6, #3b82f6);
}

/* Streaming: conic-gradient "snake" that chases around the border */
.chat-input-glow {
  background: conic-gradient(
    from var(--glow-angle, 0deg),
    transparent 0%,
    transparent 60%,
    #3b82f6 75%,
    #8b5cf6 85%,
    #c4b5fd 92%,
    transparent 100%
  );
  animation: glow-spin 3s linear infinite;
}

@keyframes glow-spin {
  to { --glow-angle: 360deg; }
}

@property --glow-angle {
  syntax: "<angle>";
  initial-value: 0deg;
  inherits: false;
}

/* Neko thinking animation */
.neko-thinking {
  animation: neko-fade-in 0.3s ease-out;
}

.neko-cat {
  animation: neko-bob 1.8s ease-in-out infinite;
  filter: drop-shadow(0 0 4px hsl(var(--primary) / 0.3));
}

/* Eyes blink */
.neko-eye-l, .neko-eye-r {
  fill: currentColor;
  animation: neko-blink 3s ease-in-out infinite;
}
.neko-eye-r { animation-delay: 0.1s; }

/* "thinking..." text pulse */
.neko-text {
  animation: neko-pulse 2s ease-in-out infinite;
}

@keyframes neko-bob {
  0%, 100% { transform: translateY(0) rotate(0deg); }
  25% { transform: translateY(-2px) rotate(-2deg); }
  75% { transform: translateY(-1px) rotate(2deg); }
}

@keyframes neko-blink {
  0%, 42%, 48%, 100% { ry: 3; }
  45% { ry: 0.3; }
}

@keyframes neko-pulse {
  0%, 100% { opacity: 0.5; }
  50% { opacity: 0.9; }
}

@keyframes neko-fade-in {
  from { opacity: 0; transform: translateY(4px) scale(0.95); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}
</style>
