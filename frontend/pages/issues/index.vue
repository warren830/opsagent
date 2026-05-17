<script setup lang="ts">
import { ref, onMounted, computed, watch, nextTick } from 'vue'
import { Search } from 'lucide-vue-next'
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
import RcaInvestigation from '@/components/issues/RcaInvestigation.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const { rcaText, thinkingText, thinkingSteps, toolCalls, isStreaming, isComplete, error: rcaError, elapsedMs, startRca, abort, reset } = useRcaStream()

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
  /** Catalog Components linked by the alerts webhook. Surfaced on the
   *  detail dialog so the on-call can jump straight to the service entry. */
  affected_component_ids?: string[]
  created_at: string
}

interface CatalogEntityLite {
  id: string
  kind: string
  name: string
  display_name: string | null
}

const issues = ref<Issue[]>([])
const loading = ref(true)
const activeFilter = ref<string>('')
const activeTypeFilter = ref<string>('')

const showDetailDialog = ref(false)
const selectedIssue = ref<Issue | null>(null)

/**
 * Affected catalog components for the currently-open issue, resolved by id.
 * Populated lazily when the detail dialog opens — the issue row only
 * carries UUIDs, and not every issue has any. Empty array means we either
 * haven't fetched yet or there are genuinely no linked components.
 */
const affectedComponents = ref<CatalogEntityLite[]>([])
async function loadAffectedComponents(issue: Issue) {
  affectedComponents.value = []
  const ids = issue.affected_component_ids || []
  if (ids.length === 0) return
  try {
    // One fetch per component — there are rarely more than a handful.
    // Catches individual 404/403s so one missing component doesn't break
    // the whole list.
    const results = await Promise.allSettled(
      ids.map(id => api.get<CatalogEntityLite>(`/api/catalog/entities/${id}`)),
    )
    affectedComponents.value = results
      .filter((r): r is PromiseFulfilledResult<CatalogEntityLite> => r.status === 'fulfilled')
      .map(r => r.value)
  } catch {
    // silent — fall back to empty list, card just won't render
  }
}

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

  // Resolve Catalog components for the "Affected Services" card. Fire-and-
  // forget so the dialog opens even if Catalog is slow or unavailable.
  if (selectedIssue.value) loadAffectedComponents(selectedIssue.value)

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
      <DialogContent class="max-w-6xl max-h-[88vh] overflow-hidden flex flex-col">
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

          <!-- Affected Services (Catalog) — skip when no components linked -->
          <div v-if="affectedComponents.length > 0" class="space-y-1">
            <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('catalog.affectedServices') }}</label>
            <div class="flex flex-wrap gap-1.5">
              <NuxtLink
                v-for="comp in affectedComponents"
                :key="comp.id"
                :to="`/catalog/${comp.id}`"
                class="inline-flex items-center gap-1.5 rounded border border-border/60 bg-secondary/30 px-2 py-1 text-xs text-foreground hover:border-primary/50 hover:text-primary transition-colors"
              >
                <Badge variant="info" class="uppercase text-[9px] px-1 py-0">{{ comp.kind }}</Badge>
                {{ comp.display_name || comp.name }}
              </NuxtLink>
            </div>
          </div>

          <!-- RCA Section -->
          <RcaInvestigation
            :rca-text="rcaText"
            :thinking-text="thinkingText"
            :thinking-steps="thinkingSteps"
            :tool-calls="toolCalls"
            :is-streaming="isStreaming"
            :is-complete="isComplete"
            :elapsed-ms="elapsedMs"
            :stored-rca-text="getStoredRcaText(selectedIssue)"
            :has-stored-result="hasRcaResult(selectedIssue)"
            :rca-error="rcaError ?? undefined"
            :can-start="canStartRca(selectedIssue)"
            :can-rerun="hasRcaResult(selectedIssue) && selectedIssue.status !== 'resolved'"
            @abort="abort"
            @start="handleStartRca"
          />

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
