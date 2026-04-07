<script setup lang="ts">
import {
  Send, PanelRightClose, RotateCcw, ChevronDown, ChevronRight, Sparkles,
  AlertCircle, Terminal, Maximize2, Minimize2, Square, Pencil, Check, Paperclip,
  FolderOpen, Trash2, Download, FileText, History,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription,
} from '@/components/ui/dialog'
import { toast } from 'vue-sonner'
import { marked } from 'marked'
import type { ChatMessage, ChatImage } from '@/composables/useChat'

// Custom renderer: mermaid code blocks → <pre class="mermaid">
const renderer = new marked.Renderer()
const originalCode = renderer.code.bind(renderer)
renderer.code = function (token: { text: string; lang?: string | null }) {
  if (token.lang === 'mermaid') {
    return `<pre class="mermaid">${token.text}</pre>`
  }
  return originalCode(token)
}
marked.setOptions({ breaks: true, gfm: true, renderer })

const { t } = useI18n()
const chatOpen = useState('chatPanelOpen', () => false)
const chatFullscreen = useState('chatFullscreen', () => false)
const { messages, isStreaming, sendMessage, editAndResend, abortStream, startNewChat, clearMessages, selectedProviderId, currentSessionId, resumeSession } = useChat()

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

onMounted(() => { loadProviders(); loadSessions() })
watch(chatOpen, (open) => { if (open) { loadProviders(); loadSessions() } })

const inputText = ref('')
const messagesEnd = ref<HTMLElement>()
const inputRef = ref<HTMLTextAreaElement>()

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

// Close session picker on click outside
function onDocClick(e: MouseEvent) {
  if (showSessionPicker.value && !(e.target as HTMLElement)?.closest('.relative')) {
    showSessionPicker.value = false
  }
}
onMounted(() => document.addEventListener('click', onDocClick))
onUnmounted(() => document.removeEventListener('click', onDocClick))

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

function renderMd(text: string): string {
  if (!text) return ''
  return marked.parse(text) as string
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
        themeVariables: { darkMode: true, background: '#181b1f', primaryColor: '#FF6600', primaryTextColor: '#e0e0e0', lineColor: '#555', fontSize: '12px' },
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
          <div class="relative">
            <button
              class="h-6 w-6 rounded flex items-center justify-center transition-colors"
              :class="showSessionPicker ? 'bg-accent text-foreground' : 'text-muted-foreground hover:text-foreground hover:bg-accent'"
              :title="t('chat.sessions')"
              @click="showSessionPicker = !showSessionPicker; if (showSessionPicker) loadSessions()"
            >
              <History class="h-3 w-3" />
            </button>
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
                  <span class="text-[9px] text-muted-foreground/40">1h</span>
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
          <button
            class="h-6 w-6 rounded flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
            :title="t('chat.newChat')"
            @click="startNewChat"
          >
            <RotateCcw class="h-3 w-3" />
          </button>
          <button
            class="h-6 w-6 rounded flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
            :title="t('chat.workspace')"
            @click="openWorkspace"
          >
            <FolderOpen class="h-3 w-3" />
          </button>
          <button
            class="h-6 w-6 rounded flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
            :title="chatFullscreen ? t('chat.minimize') : t('chat.maximize')"
            @click="toggleFullscreen"
          >
            <Minimize2 v-if="chatFullscreen" class="h-3 w-3" />
            <Maximize2 v-else class="h-3 w-3" />
          </button>
          <button
            class="h-6 w-6 rounded flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
            @click="close"
          >
            <PanelRightClose class="h-3.5 w-3.5" />
          </button>
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
        <div v-else class="py-3 space-y-1" :class="chatFullscreen ? 'max-w-3xl mx-auto px-6' : 'px-3'">
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
                  <button class="h-6 px-2 rounded text-[11px] text-muted-foreground hover:bg-accent transition-colors" @click="cancelEdit">
                    {{ t('common.cancel') }}
                  </button>
                  <button class="h-6 px-2 rounded text-[11px] bg-primary text-primary-foreground hover:bg-primary/90 transition-colors" @click="confirmEdit(msg.id)">
                    {{ t('chat.resend') }}
                  </button>
                </div>
              </div>
              <!-- Normal display -->
              <div v-else class="flex items-start gap-1 max-w-[85%]">
                <button
                  class="h-5 w-5 rounded flex items-center justify-center text-muted-foreground/0 group-hover:text-muted-foreground/50 hover:!text-foreground hover:bg-accent transition-all shrink-0 mt-0.5"
                  :title="t('common.edit')"
                  @click="startEdit(msg)"
                >
                  <Pencil class="h-2.5 w-2.5" />
                </button>
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
            <div v-else-if="msg.type === 'text'" class="py-1">
              <div
                class="chat-markdown text-[13px] leading-relaxed text-foreground/90"
                v-html="renderMd(msg.content)"
              />
            </div>

            <!-- ========== Tool use ========== -->
            <div v-else-if="msg.type === 'tool_use'" class="py-0.5">
              <button
                class="inline-flex items-center gap-1.5 text-[11px] text-muted-foreground hover:text-foreground bg-card/80 rounded px-2 py-1 border border-border/50 transition-colors"
                @click="toggleTool(msg.id)"
              >
                <Terminal class="h-3 w-3 shrink-0 text-warning" />
                <span class="font-mono">{{ msg.toolName || 'tool' }}</span>
                <ChevronRight v-if="!expandedTools.has(msg.id)" class="h-2.5 w-2.5 transition-transform" />
                <ChevronDown v-else class="h-2.5 w-2.5 transition-transform" />
              </button>
              <div
                v-if="expandedTools.has(msg.id) && msg.content"
                class="mt-1 ml-4 text-[11px] font-mono text-foreground/80 bg-muted/50 rounded px-2.5 py-2 max-h-40 overflow-y-auto border border-border/50 leading-relaxed whitespace-pre-wrap"
              >{{ msg.content }}</div>
            </div>

            <!-- ========== Tool result ========== -->
            <div v-else-if="msg.type === 'tool_result'" class="py-0.5">
              <button
                class="inline-flex items-center gap-1.5 text-[11px] text-success hover:text-success transition-colors"
                @click="toggleTool(msg.id)"
              >
                <Check class="h-3 w-3 shrink-0" />
                <span class="font-mono">{{ msg.toolName || 'result' }}</span>
                <ChevronRight v-if="!expandedTools.has(msg.id)" class="h-2.5 w-2.5" />
                <ChevronDown v-else class="h-2.5 w-2.5" />
              </button>
              <div
                v-if="expandedTools.has(msg.id) && msg.content"
                class="mt-1 ml-4 text-[11px] font-mono text-foreground/70 bg-muted/40 rounded px-2.5 py-2 max-h-40 overflow-y-auto border border-border/40 leading-relaxed whitespace-pre-wrap"
              >{{ msg.content.length > 1000 ? msg.content.slice(0, 1000) + '\n...' : msg.content }}</div>
            </div>

            <!-- ========== Error ========== -->
            <div v-else-if="msg.type === 'error'" class="py-1">
              <div class="inline-flex items-center gap-1.5 text-[11px] text-destructive/70 bg-destructive/5 rounded px-2 py-1 border border-destructive/10">
                <AlertCircle class="h-3 w-3 shrink-0" />
                <span>{{ msg.content }}</span>
              </div>
            </div>

          </template>

          <!-- Streaming dots -->
          <div v-if="isStreaming" class="py-1">
            <span class="inline-flex gap-0.5">
              <span class="w-1 h-1 rounded-full bg-muted-foreground/40 animate-bounce [animation-delay:0ms]" />
              <span class="w-1 h-1 rounded-full bg-muted-foreground/40 animate-bounce [animation-delay:150ms]" />
              <span class="w-1 h-1 rounded-full bg-muted-foreground/40 animate-bounce [animation-delay:300ms]" />
            </span>
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
            class="absolute bottom-full left-2 right-2 mb-1 bg-card border border-border/60 rounded-lg shadow-lg overflow-hidden z-20 max-h-48 overflow-y-auto"
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
            <button
              class="absolute -top-1 -right-1 h-4 w-4 rounded-full bg-destructive/80 text-white flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
              @click="removeImage(idx)"
            >
              <X class="h-2.5 w-2.5" />
            </button>
          </div>
        </div>

        <!-- Animated gradient border wrapper -->
        <div class="relative rounded-xl p-[1.5px]" :class="[chatFullscreen ? 'max-w-3xl mx-auto' : '', isStreaming ? 'chat-input-glow' : 'chat-input-idle']">
          <div class="flex gap-1.5 items-end rounded-[10px] bg-card/95 backdrop-blur-sm px-1.5 py-1.5">
            <!-- Attach button -->
            <button
              class="h-8 w-8 shrink-0 rounded-lg flex items-center justify-center text-muted-foreground/50 hover:text-foreground hover:bg-accent transition-colors"
              title="Attach image"
              @click="openFilePicker"
            >
              <Paperclip class="h-3.5 w-3.5" />
            </button>
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
                @click.stop="deleteDir(dir.name)"
                :title="t('common.delete')"
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
                    <button class="p-0.5 rounded hover:bg-accent" @click="downloadFile(f.name)" :title="t('chat.download')">
                      <Download class="h-3 w-3 text-muted-foreground/60" />
                    </button>
                    <button class="p-0.5 rounded text-destructive/50 hover:text-destructive hover:bg-destructive/10" @click="deleteFile(f.name)" :title="t('common.delete')">
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
              <button class="p-0.5 rounded hover:bg-accent" @click="downloadFile(f.name)" :title="t('chat.download')">
                <Download class="h-3 w-3 text-muted-foreground/60" />
              </button>
              <button class="p-0.5 rounded text-destructive/50 hover:text-destructive hover:bg-destructive/10" @click="deleteFile(f.name)" :title="t('common.delete')">
                <Trash2 class="h-3 w-3" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </DialogContent>
  </Dialog>
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
.chat-markdown h1, .chat-markdown h2, .chat-markdown h3 { font-weight: 600; margin: 0.5em 0 0.2em; color: hsl(var(--foreground)); }
.chat-markdown h1 { font-size: 1.1em; }
.chat-markdown h2 { font-size: 1em; }
.chat-markdown h3 { font-size: 0.95em; }
.chat-markdown a { color: hsl(var(--primary)); text-decoration: underline; text-underline-offset: 2px; }
.chat-markdown blockquote { border-left: 2px solid hsl(var(--border)); padding-left: 0.6em; margin: 0.3em 0; color: hsl(var(--muted-foreground)); }
.chat-markdown table { border-collapse: collapse; margin: 0.4em 0; font-size: 0.9em; width: 100%; }
.chat-markdown th, .chat-markdown td { border: 1px solid hsl(var(--border) / 0.4); padding: 0.3em 0.6em; text-align: left; }
.chat-markdown th { background: hsl(var(--secondary)); font-weight: 600; }
/* Mermaid */
.chat-markdown pre.mermaid { background: transparent; border: none; padding: 0.5em 0; text-align: center; }
.chat-markdown pre.mermaid svg { max-width: 100%; height: auto; }

/* Chat input border */
.chat-input-idle {
  background: linear-gradient(135deg, #ff6600, #ff8533, #ffad66, #e85d00, #ff6600);
}

/* Streaming: conic-gradient "snake" that chases around the border */
.chat-input-glow {
  background: conic-gradient(
    from var(--glow-angle, 0deg),
    transparent 0%,
    transparent 60%,
    #ff6600 75%,
    #ff8533 85%,
    #ffad66 92%,
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
</style>
