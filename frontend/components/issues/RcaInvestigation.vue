<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { Loader2, Check, Brain, FileText, Activity, Square, Play } from 'lucide-vue-next'
import { marked } from 'marked'
import { Button } from '@/components/ui/button'

interface ToolCall {
  id: string
  name: string
  label: string
  done: boolean
  durationMs: number
  startedAt: number
  inputPreview: string
  outputPreview: string
}

interface ThinkingStep {
  at: number
  content: string
}

const props = defineProps<{
  rcaText: string
  thinkingText: string
  thinkingSteps: ThinkingStep[]
  toolCalls: ToolCall[]
  isStreaming: boolean
  isComplete: boolean
  elapsedMs: number
  storedRcaText?: string
  hasStoredResult?: boolean
  rcaError?: string
  canStart?: boolean
  canRerun?: boolean
}>()

const emit = defineEmits<{
  abort: []
  start: []
}>()

const { t } = useI18n()

const rcaOutputRef = ref<HTMLElement | null>(null)

watch(() => props.rcaText, async () => {
  if (props.isStreaming && rcaOutputRef.value) {
    await nextTick()
    rcaOutputRef.value.scrollTop = rcaOutputRef.value.scrollHeight
  }
})

function formatElapsed(ms: number): string {
  const secs = Math.floor(ms / 1000)
  if (secs < 60) return `${secs}s`
  return `${Math.floor(secs / 60)}m${secs % 60}s`
}

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
  html = html.replace(/>([^<]+)</g, (_, text) => {
    return '>' + (text as string).replace(RCA_KEYWORD_RE, '<mark class="rca-flash">$1</mark>') + '<'
  })
  return html
}

function toolIcon(name: string): string {
  const n = (name || '').toLowerCase()
  if (n.includes('graphrag') || n.includes('rag_tool')) return '📖'
  if (n === 'bash') return '⚡'
  if (n === 'read') return '📄'
  if (n === 'webfetch') return '🌐'
  return '🔧'
}

function toolColor(label: string): string {
  if (label.includes('kubectl')) return 'text-info'
  if (label.includes('Mimir')) return 'text-ai'
  if (label.includes('Loki')) return 'text-warning'
  if (label.includes('Tempo')) return 'text-destructive'
  if (label.includes('Runbook') || label.includes('知识库')) return 'text-success'
  if (label.includes('Rollout 状态')) return 'text-primary'
  if (label.includes('Argo')) return 'text-info'
  return 'text-zinc-400'
}

function fmtRelative(startTs: number | null, ts: number): string {
  if (!startTs) return ''
  const s = Math.max(0, Math.floor((ts - startTs) / 1000))
  return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`
}

type TimelineEvt = { kind: 'think' | 'tool'; at: number; label: string; tc?: ToolCall }

const timelineEvents = computed<TimelineEvt[]>(() => {
  const arr: TimelineEvt[] = []
  for (const ts of props.thinkingSteps) arr.push({ kind: 'think', at: ts.at, label: ts.content })
  for (const tc of props.toolCalls) arr.push({ kind: 'tool', at: tc.startedAt, label: tc.label, tc })
  arr.sort((a, b) => a.at - b.at)
  return arr
})

const timelineStart = computed(() => timelineEvents.value[0]?.at ?? null)
</script>

<template>
  <div class="space-y-2">
    <!-- Streaming or active RCA -->
    <template v-if="isStreaming || rcaText">
      <div class="flex items-center justify-between">
        <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
          <Loader2 v-if="isStreaming" class="h-3 w-3 animate-spin text-info" />
          {{ isStreaming ? t('issue.rcaStreaming') : t('issue.rcaComplete') }}
        </label>
        <div class="flex items-center gap-2">
          <span v-if="elapsedMs > 0" class="text-[10px] text-muted-foreground/60">
            {{ t('issue.rcaElapsed') }} {{ formatElapsed(elapsedMs) }}
          </span>
          <Button v-if="isStreaming" variant="ghost" size="icon-sm" class="text-destructive" @click="emit('abort')">
            <Square class="h-3 w-3" />
          </Button>
        </div>
      </div>

      <div v-if="thinkingText && isStreaming" class="text-sm text-info font-medium px-2 truncate flex items-center gap-2">
        <span class="inline-block h-1.5 w-1.5 rounded-full bg-info animate-pulse" />
        {{ thinkingText }}
      </div>

      <div class="grid grid-cols-1 lg:grid-cols-[220px_1fr_300px] gap-3">
        <!-- LEFT: Timeline -->
        <div class="rounded border border-border/60 bg-secondary/20 p-2 max-h-[440px] overflow-y-auto">
          <div class="flex items-center gap-1.5 px-1 pb-1.5 mb-1.5 border-b border-border/40">
            <Activity class="h-3 w-3 text-info" />
            <span class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              {{ t('issue.timeline') }}
            </span>
            <span class="ml-auto text-[10px] text-muted-foreground/60 tabular-nums">{{ timelineEvents.length }}</span>
          </div>
          <div v-if="timelineEvents.length === 0" class="text-[11px] text-muted-foreground/50 px-1 py-2 italic">
            {{ t('issue.timelineWaiting') }}
          </div>
          <ul class="space-y-1.5">
            <li
              v-for="(evt, idx) in timelineEvents"
              :key="`${evt.kind}-${idx}-${evt.at}`"
              class="tool-item flex items-start gap-1.5 text-[11px] leading-snug"
            >
              <span class="shrink-0 text-[9px] text-muted-foreground/60 tabular-nums pt-0.5 w-8">
                {{ fmtRelative(timelineStart, evt.at) }}
              </span>
              <template v-if="evt.kind === 'think'">
                <Brain class="h-3 w-3 text-ai shrink-0 mt-0.5" />
                <span class="text-foreground/80 line-clamp-2">{{ evt.label }}</span>
              </template>
              <template v-else>
                <Loader2 v-if="!evt.tc?.done" class="h-3 w-3 animate-spin text-info shrink-0 mt-0.5" />
                <Check v-else class="h-3 w-3 text-success shrink-0 mt-0.5" />
                <span :class="[toolColor(evt.label), 'font-medium truncate flex-1']">{{ evt.label }}</span>
                <span v-if="evt.tc?.done && evt.tc.durationMs > 0" class="text-[9px] text-muted-foreground/50 tabular-nums shrink-0">
                  {{ (evt.tc.durationMs / 1000).toFixed(1) }}s
                </span>
              </template>
            </li>
          </ul>
        </div>

        <!-- MIDDLE: Markdown report -->
        <div
          ref="rcaOutputRef"
          class="rca-markdown rounded border border-border/60 bg-secondary/20 p-3 max-h-[440px] overflow-y-auto text-xs text-foreground/90 will-change-transform"
          v-html="renderMarkdown(rcaText)"
        />

        <!-- RIGHT: Evidence panel -->
        <div class="rounded border border-border/60 bg-secondary/20 p-2 max-h-[440px] overflow-y-auto">
          <div class="flex items-center gap-1.5 px-1 pb-1.5 mb-1.5 border-b border-border/40">
            <FileText class="h-3 w-3 text-info" />
            <span class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              {{ t('issue.evidence') }}
            </span>
            <span class="ml-auto text-[10px] text-muted-foreground/60 tabular-nums">{{ toolCalls.length }}</span>
          </div>
          <div v-if="toolCalls.length === 0" class="text-[11px] text-muted-foreground/50 px-1 py-2 italic">
            {{ isStreaming ? t('issue.toolsWaiting') : t('issue.toolsNone') }}
          </div>
          <ul class="space-y-2">
            <li
              v-for="tc in toolCalls"
              :key="tc.id"
              class="tool-item rounded border border-border/40 bg-background/40 p-1.5"
              :class="!tc.done && 'border-info/40/40 bg-info/5'"
            >
              <div class="flex items-center gap-1.5 text-[11px] mb-1">
                <span class="shrink-0">{{ toolIcon(tc.name) }}</span>
                <span :class="[toolColor(tc.label), 'font-medium truncate flex-1']">{{ tc.label }}</span>
                <span v-if="tc.done && tc.durationMs > 0" class="text-[9px] text-muted-foreground/50 tabular-nums shrink-0">
                  {{ (tc.durationMs / 1000).toFixed(1) }}s
                </span>
                <Loader2 v-if="!tc.done" class="h-3 w-3 animate-spin text-info shrink-0" />
                <Check v-else class="h-3 w-3 text-success shrink-0" />
              </div>
              <div v-if="tc.inputPreview" class="text-[10px] text-muted-foreground/80 font-mono bg-secondary/40 rounded px-1.5 py-1 mb-1 break-all">
                $ {{ tc.inputPreview }}
              </div>
              <div v-if="tc.outputPreview" class="text-[10px] text-foreground/70 font-mono whitespace-pre-wrap leading-relaxed line-clamp-6">{{ tc.outputPreview }}</div>
            </li>
          </ul>
        </div>
      </div>
    </template>

    <!-- Stored RCA result -->
    <template v-else-if="hasStoredResult && storedRcaText">
      <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('issue.rcaComplete') }}</label>
      <div
        class="rca-markdown rounded border border-border/60 bg-secondary/20 p-3 max-h-[360px] overflow-y-auto text-xs text-foreground/90"
        v-html="renderMarkdown(storedRcaText)"
      />
    </template>

    <!-- Error -->
    <div v-if="rcaError" class="rounded border border-destructive/30 bg-destructive/10 p-2 text-xs text-destructive">
      {{ rcaError }}
    </div>

    <!-- Start button -->
    <div v-if="canStart && !isStreaming && !rcaText">
      <Button size="sm" @click="emit('start')">
        <Play class="h-3 w-3" />
        {{ t('issue.rcaStart') }}
      </Button>
    </div>

    <!-- Re-run button -->
    <div v-if="canRerun && !isStreaming" class="pt-1">
      <Button variant="outline" size="sm" @click="emit('start')">
        <Play class="h-3 w-3" />
        {{ t('issue.rcaStart') }}
      </Button>
    </div>
  </div>
</template>

<style scoped>
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

.tool-item { animation: tool-slide-in 0.35s ease-out; }
@keyframes tool-slide-in {
  from { opacity: 0; transform: translateX(6px); }
  to   { opacity: 1; transform: translateX(0); }
}
</style>
