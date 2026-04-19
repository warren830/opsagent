<script setup lang="ts">
import { ref, onMounted, computed, watch, nextTick } from 'vue'
import { Search, Play, Square, Loader2, Wrench, Check } from 'lucide-vue-next'
import { marked } from 'marked'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from '@/components/ui/dialog'
import DataTable from '@/components/shared/DataTable.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const { rcaText, thinkingText, toolCalls, isStreaming, isComplete, error: rcaError, elapsedMs, startRca, abort, reset } = useRcaStream()

interface Issue {
  id: string
  title: string
  description: string | null
  source: string
  severity: 'critical' | 'high' | 'medium' | 'low'
  status: 'open' | 'investigating' | 'rca_done' | 'resolved'
  issue_type: 'incident' | 'prediction'
  rca_result: { analysis?: string; error?: string } | string | null
  rca_started_at: string | null
  rca_completed_at: string | null
  timeline: { time: string; event: string }[] | null
  created_at: string
}

const issues = ref<Issue[]>([])
const loading = ref(true)
const activeFilter = ref<string>('')
const activeTypeFilter = ref<string>('')

const showDetailDialog = ref(false)
const selectedIssue = ref<Issue | null>(null)
const rcaOutputRef = ref<HTMLElement | null>(null)

const filters = [
  { value: '', label: () => t('issue.all') },
  { value: 'open', label: () => t('issue.open') },
  { value: 'investigating', label: () => t('issue.investigating') },
  { value: 'rca_done', label: () => t('issue.rcaDone') },
  { value: 'resolved', label: () => t('issue.resolved') },
]

const typeFilters = [
  { value: '', label: () => t('issue.allTypes') },
  { value: 'incident', label: () => t('issue.incident') },
  { value: 'prediction', label: () => t('issue.prediction') },
]

const filteredIssues = computed(() => {
  let result = issues.value
  if (!activeFilter.value) {
    result = result.filter((i) => i.status !== 'resolved')
  } else {
    result = result.filter((i) => i.status === activeFilter.value)
  }
  if (activeTypeFilter.value) {
    result = result.filter((i) => i.issue_type === activeTypeFilter.value)
  }
  return result
})

const columns = computed(() => [
  { key: 'title', label: t('issue.title') },
  { key: 'issue_type', label: t('issue.issueType') },
  { key: 'source', label: t('issue.source') },
  { key: 'severity', label: t('issue.severity') },
  { key: 'status', label: t('cluster.status') },
  { key: 'created_at', label: t('tenant.createdAt') },
])

// --- Helpers ---

function severityVariant(severity: string): 'destructive' | 'warning' | 'info' | 'secondary' {
  switch (severity) {
    case 'critical': return 'destructive'
    case 'high': return 'warning'
    case 'medium': return 'info'
    case 'low': return 'secondary'
    default: return 'secondary'
  }
}

function statusVariant(status: string): 'warning' | 'info' | 'success' | 'secondary' {
  switch (status) {
    case 'open': return 'warning'
    case 'investigating': return 'info'
    case 'rca_done': return 'success'
    case 'resolved': return 'secondary'
    default: return 'secondary'
  }
}

function statusLabel(status: string): string {
  const map: Record<string, string> = {
    open: t('issue.open'),
    investigating: t('issue.investigating'),
    rca_done: t('issue.rcaDone'),
    resolved: t('issue.resolved'),
  }
  return map[status] || status
}

function formatDate(dateStr: string): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

function formatElapsed(ms: number): string {
  const secs = Math.floor(ms / 1000)
  if (secs < 60) return `${secs}s`
  return `${Math.floor(secs / 60)}m${secs % 60}s`
}

// Keywords to flash-highlight in the RCA markdown — draws the viewer's eye
// to the "smoking gun" evidence as it streams in.
const RCA_KEYWORDS = [
  'OOMKilled', 'CrashLoopBackOff', 'ImagePullBackOff', 'Evicted',
  'BUGGY=true', 'memory leak', '内存泄漏',
  'Root Cause', '根因', '根本原因', '直接原因', '深层原因',
  'v2-buggy', 'rollback', '回滚',
  'canary', 'Paused', 'Degraded',
]

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

const RCA_KEYWORD_RE = new RegExp(`(${RCA_KEYWORDS.map(escapeRegex).join('|')})`, 'gi')

function renderMarkdown(md: string): string {
  if (!md) return ''
  let html = marked.parse(md, { async: false }) as string
  // Post-process: wrap keywords in <mark class="rca-flash"> for highlight.
  // Protect tags + existing code/pre by only matching inside text segments.
  html = html.replace(/>([^<]+)</g, (_, text) => {
    return '>' + (text as string).replace(RCA_KEYWORD_RE, '<mark class="rca-flash">$1</mark>') + '<'
  })
  return html
}

/** Extract RCA analysis text from the stored rca_result field */
function getStoredRcaText(issue: Issue): string {
  if (!issue.rca_result) return ''
  if (typeof issue.rca_result === 'string') return issue.rca_result
  return issue.rca_result.analysis || issue.rca_result.error || ''
}

/** Whether the issue has a running RCA (started but not completed) */
function isRcaRunning(issue: Issue): boolean {
  return !!issue.rca_started_at && !issue.rca_completed_at
}

/** Whether the issue has completed RCA */
function hasRcaResult(issue: Issue): boolean {
  return !!issue.rca_completed_at && !!issue.rca_result
}

/** Whether the issue can start RCA */
function canStartRca(issue: Issue): boolean {
  return issue.status !== 'resolved' && !isRcaRunning(issue)
}

// --- API ---

async function fetchIssues() {
  loading.value = true
  try {
    const params = new URLSearchParams()
    if (activeFilter.value) params.set('status', activeFilter.value)
    if (activeTypeFilter.value) params.set('issue_type', activeTypeFilter.value)
    const qs = params.toString()
    issues.value = await api.get<Issue[]>(`/api/issues${qs ? '?' + qs : ''}`)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

async function openDetail(issue: Issue) {
  // Refresh issue to get latest state
  try {
    const fresh = await api.get<Issue>(`/api/issues/${issue.id}`)
    selectedIssue.value = fresh
  } catch {
    selectedIssue.value = issue
  }

  reset()
  showDetailDialog.value = true

  // If RCA is running on the backend, auto-connect to the stream
  await nextTick()
  if (selectedIssue.value && isRcaRunning(selectedIssue.value)) {
    try {
      const status = await api.get<{ running: boolean }>(`/api/issues/${selectedIssue.value.id}/rca/status`)
      if (status.running) {
        startRca(selectedIssue.value.id)
      }
    } catch {
      // Not running — show stored result if any
    }
  }
}

async function handleStartRca() {
  if (!selectedIssue.value) return
  reset()
  startRca(selectedIssue.value.id)
}

// Auto-scroll during streaming
watch(rcaText, async () => {
  if (isStreaming.value && rcaOutputRef.value) {
    await nextTick()
    rcaOutputRef.value.scrollTop = rcaOutputRef.value.scrollHeight
  }
})

// Refresh issue list when RCA completes
watch(isComplete, (done) => {
  if (done) {
    fetchIssues()
  }
})

// Close cleanup
watch(showDetailDialog, (open) => {
  if (!open) {
    if (isStreaming.value) abort()
    reset()
  }
})

onMounted(() => { fetchIssues() })
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('issue.title') }}</h1>
    </div>

    <!-- Filters -->
    <div class="flex items-center gap-3">
      <div class="flex items-center gap-1.5">
        <Button
          v-for="f in filters"
          :key="f.value"
          size="sm"
          :variant="activeFilter === f.value ? 'default' : 'outline'"
          @click="activeFilter = f.value; fetchIssues()"
        >
          {{ f.label() }}
        </Button>
      </div>

      <div class="w-px h-5 bg-border/60" />

      <div class="flex items-center gap-1.5">
        <Button
          v-for="tf in typeFilters"
          :key="tf.value"
          size="sm"
          :variant="activeTypeFilter === tf.value ? 'default' : 'outline'"
          @click="activeTypeFilter = tf.value; fetchIssues()"
        >
          {{ tf.label() }}
        </Button>
      </div>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="filteredIssues" :loading="loading">
      <template #cell-title="{ row }">
        <button class="font-medium text-foreground hover:text-primary transition-colors text-left" @click="openDetail(row as Issue)">
          {{ (row as Issue).title }}
        </button>
      </template>

      <template #cell-issue_type="{ row }">
        <Badge
          :variant="(row as Issue).issue_type === 'prediction' ? 'info' : 'warning'"
          class="text-[10px]"
        >
          {{ (row as Issue).issue_type === 'prediction' ? t('issue.prediction') : t('issue.incident') }}
        </Badge>
      </template>

      <template #cell-source="{ row }">
        <Badge variant="secondary">{{ (row as Issue).source }}</Badge>
      </template>

      <template #cell-severity="{ row }">
        <Badge :variant="severityVariant((row as Issue).severity)">
          {{ (row as Issue).severity }}
        </Badge>
      </template>

      <template #cell-status="{ row }">
        <Badge :variant="statusVariant((row as Issue).status)">
          {{ statusLabel((row as Issue).status) }}
        </Badge>
      </template>

      <template #cell-created_at="{ row }">
        <span class="text-muted-foreground">{{ formatDate((row as Issue).created_at) }}</span>
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" @click="openDetail(row as Issue)">
          <Search class="h-3 w-3" />
        </Button>
      </template>
    </DataTable>

    <!-- Detail Dialog -->
    <Dialog :open="showDetailDialog" @update:open="(val) => { showDetailDialog = val }">
      <DialogContent class="max-w-4xl max-h-[85vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle>{{ t('issue.detail') }}</DialogTitle>
          <DialogDescription class="truncate">{{ selectedIssue?.title }}</DialogDescription>
        </DialogHeader>

        <div v-if="selectedIssue" class="flex-1 overflow-y-auto space-y-3 min-h-0">
          <!-- Meta badges -->
          <div class="flex items-center gap-1.5 flex-wrap">
            <Badge
              :variant="selectedIssue.issue_type === 'prediction' ? 'info' : 'warning'"
              class="text-[10px]"
            >
              {{ selectedIssue.issue_type === 'prediction' ? t('issue.prediction') : t('issue.incident') }}
            </Badge>
            <Badge :variant="severityVariant(selectedIssue.severity)">{{ selectedIssue.severity }}</Badge>
            <Badge :variant="statusVariant(selectedIssue.status)">{{ statusLabel(selectedIssue.status) }}</Badge>
            <Badge variant="secondary">{{ selectedIssue.source }}</Badge>
          </div>

          <!-- Description -->
          <div v-if="selectedIssue.description" class="space-y-1">
            <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('glossary.description') }}</label>
            <p class="text-xs text-foreground whitespace-pre-wrap rounded border border-border/60 bg-secondary/30 p-2">{{ selectedIssue.description }}</p>
          </div>

          <!-- RCA Section -->
          <div class="space-y-2">
            <!-- Streaming RCA output -->
            <template v-if="isStreaming || rcaText">
              <div class="flex items-center justify-between">
                <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
                  <Loader2 v-if="isStreaming" class="h-3 w-3 animate-spin text-orange-400" />
                  {{ isStreaming ? t('issue.rcaStreaming') : t('issue.rcaComplete') }}
                </label>
                <div class="flex items-center gap-2">
                  <span v-if="elapsedMs > 0" class="text-[10px] text-muted-foreground/60">
                    {{ t('issue.rcaElapsed') }} {{ formatElapsed(elapsedMs) }}
                  </span>
                  <Button v-if="isStreaming" variant="ghost" size="icon-sm" class="text-destructive" @click="abort">
                    <Square class="h-3 w-3" />
                  </Button>
                </div>
              </div>

              <!-- Thinking / tool-use indicator (enlarged + colored for demo visibility) -->
              <div v-if="thinkingText && isStreaming" class="text-sm text-orange-400 font-medium px-2 truncate flex items-center gap-2">
                <span class="inline-block h-1.5 w-1.5 rounded-full bg-orange-400 animate-pulse" />
                {{ thinkingText }}
              </div>

              <!-- Split view: markdown (left) + tools-called sidebar (right) -->
              <div class="grid grid-cols-1 md:grid-cols-[1fr_240px] gap-3">
                <!-- Markdown output with typewriter -->
                <div
                  ref="rcaOutputRef"
                  class="rca-markdown rounded border border-border/60 bg-secondary/20 p-3 max-h-[360px] overflow-y-auto text-xs text-foreground/90 will-change-transform"
                  v-html="renderMarkdown(rcaText)"
                />

                <!-- Tools Called sidebar -->
                <div class="rounded border border-border/60 bg-secondary/20 p-2 max-h-[360px] overflow-y-auto">
                  <div class="flex items-center gap-1.5 px-1 pb-1.5 mb-1.5 border-b border-border/40">
                    <Wrench class="h-3 w-3 text-orange-400" />
                    <span class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                      {{ t('issue.toolsCalled') }}
                    </span>
                    <span class="ml-auto text-[10px] text-muted-foreground/60 tabular-nums">{{ toolCalls.length }}</span>
                  </div>
                  <div v-if="toolCalls.length === 0" class="text-[11px] text-muted-foreground/50 px-1 py-2 italic">
                    {{ isStreaming ? t('issue.toolsWaiting') : t('issue.toolsNone') }}
                  </div>
                  <ul class="space-y-1">
                    <li
                      v-for="tc in toolCalls"
                      :key="tc.id"
                      class="tool-item flex items-center gap-1.5 px-1.5 py-1 rounded text-[11px]"
                      :class="tc.done ? 'text-foreground/90' : 'text-orange-400 bg-orange-500/5'"
                    >
                      <Loader2 v-if="!tc.done" class="h-3 w-3 animate-spin shrink-0" />
                      <Check v-else class="h-3 w-3 text-emerald-400 shrink-0" />
                      <span class="truncate flex-1">{{ tc.label }}</span>
                      <span v-if="tc.done && tc.durationMs > 0" class="text-[9px] text-muted-foreground/50 tabular-nums shrink-0">
                        {{ (tc.durationMs / 1000).toFixed(1) }}s
                      </span>
                    </li>
                  </ul>
                </div>
              </div>
            </template>

            <!-- Stored RCA result (not streaming) -->
            <template v-else-if="hasRcaResult(selectedIssue)">
              <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('issue.rcaComplete') }}</label>
              <div
                class="rca-markdown rounded border border-border/60 bg-secondary/20 p-3 max-h-[360px] overflow-y-auto text-xs text-foreground/90"
                v-html="renderMarkdown(getStoredRcaText(selectedIssue))"
              />
            </template>

            <!-- RCA error -->
            <div v-if="rcaError" class="rounded border border-destructive/30 bg-destructive/10 p-2 text-xs text-destructive">
              {{ rcaError }}
            </div>

            <!-- Start RCA button -->
            <div v-if="canStartRca(selectedIssue) && !isStreaming && !rcaText">
              <Button size="sm" @click="handleStartRca">
                <Play class="h-3 w-3" />
                {{ t('issue.rcaStart') }}
              </Button>
            </div>

            <!-- Re-run RCA (after completion) -->
            <div v-if="hasRcaResult(selectedIssue) && !isStreaming && selectedIssue.status !== 'resolved'" class="pt-1">
              <Button variant="outline" size="sm" @click="handleStartRca">
                <Play class="h-3 w-3" />
                {{ t('issue.rcaStart') }}
              </Button>
            </div>
          </div>

          <!-- Timeline -->
          <div v-if="selectedIssue.timeline && selectedIssue.timeline.length > 0" class="space-y-1">
            <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('issue.timeline') }}</label>
            <div class="space-y-1 rounded border border-border/60 bg-secondary/30 p-2">
              <div
                v-for="(entry, idx) in selectedIssue.timeline"
                :key="idx"
                class="flex items-start gap-2 text-xs"
              >
                <span class="text-muted-foreground whitespace-nowrap shrink-0">{{ entry.time }}</span>
                <span class="text-foreground">{{ entry.event }}</span>
              </div>
            </div>
          </div>
        </div>

        <DialogFooter class="gap-1.5 pt-1 shrink-0">
          <Button variant="outline" size="sm" @click="showDetailDialog = false">{{ t('common.close') }}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>

<style scoped>
/* RCA markdown styling — matches ChatPanel prose */
.rca-markdown :deep(h1) { font-size: 0.875rem; font-weight: 700; margin-top: 1rem; margin-bottom: 0.375rem; color: var(--foreground); border-bottom: 1px solid hsl(var(--border) / 0.3); padding-bottom: 0.25rem; }
.rca-markdown :deep(h2) { font-size: 0.8125rem; font-weight: 600; margin-top: 0.75rem; margin-bottom: 0.25rem; color: var(--foreground); }
.rca-markdown :deep(h3) { font-size: 0.75rem; font-weight: 600; margin-top: 0.625rem; margin-bottom: 0.25rem; color: var(--foreground); }
.rca-markdown :deep(h4) { font-size: 0.6875rem; font-weight: 600; margin-top: 0.5rem; margin-bottom: 0.125rem; color: var(--foreground); }
.rca-markdown :deep(p) { margin: 0.25rem 0; line-height: 1.6; }
.rca-markdown :deep(ul),
.rca-markdown :deep(ol) { padding-left: 1.25rem; margin: 0.25rem 0; }
.rca-markdown :deep(li) { margin: 0.125rem 0; line-height: 1.5; }
.rca-markdown :deep(code) { font-size: 0.6875rem; padding: 0.125rem 0.25rem; border-radius: 0.25rem; background: hsl(var(--secondary)); font-family: ui-monospace, monospace; }
.rca-markdown :deep(pre) { margin: 0.375rem 0; padding: 0.5rem; border-radius: 0.375rem; background: hsl(var(--secondary) / 0.8); overflow-x: auto; }
.rca-markdown :deep(pre code) { padding: 0; background: none; font-size: 0.6875rem; line-height: 1.5; }
.rca-markdown :deep(strong) { font-weight: 600; }
.rca-markdown :deep(hr) { border: none; border-top: 1px solid hsl(var(--border) / 0.4); margin: 0.5rem 0; }
.rca-markdown :deep(blockquote) { border-left: 3px solid hsl(var(--border)); padding-left: 0.5rem; margin: 0.375rem 0; color: hsl(var(--muted-foreground)); }
.rca-markdown :deep(table) { width: 100%; border-collapse: collapse; margin: 0.375rem 0; font-size: 0.6875rem; }
.rca-markdown :deep(th) { text-align: left; padding: 0.25rem 0.5rem; border-bottom: 1px solid hsl(var(--border) / 0.6); font-weight: 600; }
.rca-markdown :deep(td) { padding: 0.25rem 0.5rem; border-bottom: 1px solid hsl(var(--border) / 0.3); }

/* Keyword flash-highlight — draws attention to smoking-gun evidence as RCA streams in */
.rca-markdown :deep(mark.rca-flash) {
  background: linear-gradient(90deg, rgba(251, 146, 60, 0.35), rgba(251, 146, 60, 0.15));
  color: rgb(251, 146, 60);
  padding: 0 0.25rem;
  border-radius: 0.1875rem;
  font-weight: 600;
  animation: rca-flash-in 0.6s ease-out;
}
@keyframes rca-flash-in {
  0%   { background: rgba(251, 146, 60, 0.9); color: #fff; }
  60%  { background: rgba(251, 146, 60, 0.5); }
  100% { background: linear-gradient(90deg, rgba(251, 146, 60, 0.35), rgba(251, 146, 60, 0.15)); }
}

/* Tools-called side list — slide-in animation when a new tool is pushed */
.tool-item { animation: tool-slide-in 0.35s ease-out; }
@keyframes tool-slide-in {
  from { opacity: 0; transform: translateX(6px); }
  to   { opacity: 1; transform: translateX(0); }
}
</style>
