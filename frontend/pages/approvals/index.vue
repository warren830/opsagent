<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, computed } from 'vue'
import { Check, X, Loader2, ChevronDown, ChevronRight, CheckCircle2, XCircle, Eye, EyeOff, Undo2, Info } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const authStore = useAuthStore()

interface Approval {
  id: string
  command: string
  reason: string | null
  requested_by: string
  status: 'pending' | 'approved' | 'rejected' | 'executing' | 'executed' | 'completed' | 'failed' | 'withdrawn'
  jira_key: string | null
  plan_detail: Record<string, unknown> | null
  execution_result: Record<string, unknown> | null
  executed_at: string | null
  created_at: string
}

const approvals = ref<Approval[]>([])
const loading = ref(true)
const activeFilter = ref<string>('pending')
const expandedIds = ref<Record<string, boolean>>({})
const jiraBaseUrl = ref<string | null>(null)
let executionPollTimer: ReturnType<typeof setInterval> | null = null
let baselinePollTimer: ReturnType<typeof setInterval> | null = null

const filters = ['pending', 'approved', 'executing', 'executed', 'completed', 'rejected', 'failed', 'withdrawn'] as const

const filteredApprovals = computed(() =>
  approvals.value.filter((a) => a.status === activeFilter.value)
)

const filterCounts = computed(() => {
  const counts: Record<string, number> = {}
  for (const f of filters) counts[f] = 0
  for (const a of approvals.value) {
    if (a.status in counts) counts[a.status]++
  }
  return counts
})

const hasActiveExecution = computed(() =>
  approvals.value.some(a => a.status === 'approved' || a.status === 'executing')
)

const columns = computed(() => [
  { key: 'command', label: t('approval.command') },
  { key: 'jira_key', label: 'Jira' },
  { key: 'status', label: t('user.status') },
  { key: 'executed_at', label: t('approval.executedAt') },
  { key: 'created_at', label: t('approval.requestedAt') },
])

function statusVariant(status: string): string {
  switch (status) {
    case 'pending': return 'warning'
    case 'approved': return 'success'
    case 'executing': return 'info'
    case 'rejected': return 'destructive'
    case 'executed': return 'purple'
    case 'completed': return 'success'
    case 'failed': return 'destructive'
    case 'withdrawn': return 'secondary'
    default: return 'secondary'
  }
}

function statusLabel(status: string): string {
  const map: Record<string, string> = {
    pending: t('approval.pending'),
    approved: t('approval.approved'),
    executing: t('approval.executing'),
    rejected: t('approval.rejected'),
    executed: t('approval.executed'),
    completed: t('approval.completed'),
    failed: t('approval.failed'),
    withdrawn: t('approval.withdrawn'),
  }
  return map[status] || status
}

function formatDate(dateStr: string): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, {
    year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  })
}

function jiraUrl(key: string): string | null {
  if (!jiraBaseUrl.value) return null
  return `${jiraBaseUrl.value}/browse/${key}`
}

async function fetchJiraBaseUrl() {
  try {
    const data = await api.get<{ url: string | null }>('/api/approvals/jira-url')
    if (data.url) jiraBaseUrl.value = data.url
  } catch { /* non-critical */ }
}

function toggleExpand(id: string) {
  expandedIds.value = { ...expandedIds.value, [id]: !expandedIds.value[id] }
}

function canExpand(row: Approval): boolean {
  return row.execution_result != null || ['executing', 'executed', 'completed', 'failed', 'approved'].includes(row.status)
}

function getOutput(row: Approval): string {
  if (!row.execution_result) return ''
  const r = row.execution_result as Record<string, unknown>
  if (typeof r.output === 'string') return r.output
  if (typeof r.error === 'string') return r.error
  return JSON.stringify(r, null, 2)
}

async function fetchApprovals() {
  try {
    const data = await api.get<Approval[]>('/api/approvals')
    if (
      data.length === approvals.value.length &&
      data.every((a, i) =>
        a.id === approvals.value[i].id &&
        a.status === approvals.value[i].status &&
        a.executed_at === approvals.value[i].executed_at
      )
    ) {
      return
    }
    const oldById = new Map(approvals.value.map(a => [a.id, a]))
    const newExpands: Record<string, boolean> = {}
    for (const newA of data) {
      const old = oldById.get(newA.id)
      if (old?.status === 'executing' && newA.status === 'executed') {
        toast.success(t('approval.executionSuccess'), { description: newA.command })
        newExpands[newA.id] = true
      } else if (old?.status === 'executing' && newA.status === 'failed') {
        toast.error(t('approval.executionFailed'), { description: newA.command })
        newExpands[newA.id] = true
      }
    }
    if (Object.keys(newExpands).length) {
      expandedIds.value = { ...expandedIds.value, ...newExpands }
    }
    approvals.value = data
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

function startExecutionPolling() {
  if (executionPollTimer) return
  executionPollTimer = setInterval(() => {
    if (hasActiveExecution.value) fetchApprovals()
    else stopExecutionPolling()
  }, 4000)
}

function stopExecutionPolling() {
  if (executionPollTimer) { clearInterval(executionPollTimer); executionPollTimer = null }
}

function startBaselinePolling() {
  if (baselinePollTimer) return
  baselinePollTimer = setInterval(fetchApprovals, 15_000)
}

function stopBaselinePolling() {
  if (baselinePollTimer) { clearInterval(baselinePollTimer); baselinePollTimer = null }
}

async function approve(id: string) {
  try {
    await api.post(`/api/approvals/${id}/approve`)
    toast.success(t('common.success'))
    await fetchApprovals()
    activeFilter.value = 'executing'
    startExecutionPolling()
  } catch (err: unknown) {
    toast.error(err instanceof Error ? err.message : t('common.error'))
  }
}

async function reject(id: string) {
  try {
    await api.post(`/api/approvals/${id}/reject`)
    toast.success(t('common.success'))
    await fetchApprovals()
  } catch (err: unknown) {
    toast.error(err instanceof Error ? err.message : t('common.error'))
  }
}

async function markResult(id: string, success: boolean) {
  try {
    await api.post(`/api/approvals/${id}/mark`, { success })
    toast.success(success ? t('approval.markedSuccess') : t('approval.markedFailed'))
    await fetchApprovals()
  } catch (err: unknown) {
    toast.error(err instanceof Error ? err.message : t('common.error'))
  }
}

async function withdraw(id: string) {
  try {
    await api.post(`/api/approvals/${id}/withdraw`)
    toast.success(t('approval.withdrawSuccess'))
    await fetchApprovals()
  } catch (err: unknown) {
    toast.error(err instanceof Error ? err.message : t('common.error'))
  }
}

function isOwnRequest(row: Approval): boolean {
  return authStore.user?.id === row.requested_by
}

watch(hasActiveExecution, (active) => {
  if (active) { stopBaselinePolling(); startExecutionPolling() }
  else { stopExecutionPolling(); startBaselinePolling() }
})

onMounted(() => {
  loading.value = true
  fetchApprovals()
  fetchJiraBaseUrl()
  startBaselinePolling()
})

onUnmounted(() => {
  stopExecutionPolling()
  stopBaselinePolling()
})
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('approval.title') }}</h1>
    </div>

    <!-- Status filter tabs -->
    <div class="flex flex-wrap items-center gap-1.5">
      <Button
        v-for="f in filters"
        :key="f"
        size="sm"
        :variant="activeFilter === f ? 'default' : 'outline'"
        @click="activeFilter = f"
      >
        {{ statusLabel(f) }}
        <Badge variant="secondary" class="ml-1.5">{{ filterCounts[f] }}</Badge>
      </Button>
    </div>

    <!-- Table with expandable rows -->
    <div class="rounded border border-border/60 bg-card overflow-hidden">
      <div class="overflow-x-auto">
        <table class="w-full text-xs">
          <thead>
            <tr class="border-b border-border/60 bg-secondary/30">
              <th class="h-9 w-8 px-2" />
              <th
                v-for="col in columns"
                :key="col.key"
                class="h-9 px-3 text-left align-middle font-medium text-muted-foreground whitespace-nowrap uppercase tracking-wider text-[11px]"
              >
                {{ col.label }}
              </th>
              <th class="h-9 px-3 text-right align-middle font-medium text-muted-foreground whitespace-nowrap uppercase tracking-wider text-[11px]">
                {{ t('common.actions') }}
              </th>
            </tr>
          </thead>
          <tbody>
            <template v-if="loading">
              <tr v-for="i in 5" :key="`sk-${i}`" class="border-b border-border/40">
                <td class="px-2 py-2" />
                <td v-for="col in columns" :key="`sk-${i}-${col.key}`" class="px-3 py-2">
                  <div class="h-4 w-3/4 rounded-sm bg-secondary animate-pulse" />
                </td>
                <td class="px-3 py-2" />
              </tr>
            </template>

            <template v-else-if="filteredApprovals.length > 0">
              <template v-for="row in filteredApprovals" :key="row.id">
                <tr
                  class="border-b border-border/40 transition-colors hover:bg-accent/50"
                  :class="expandedIds[row.id] ? 'bg-accent/30' : ''"
                >
                  <!-- Expand toggle -->
                  <td class="px-2 py-2 align-middle">
                    <Button
                      v-if="canExpand(row)"
                      variant="ghost" size="sm"
                      class="h-5 w-5 p-0 text-muted-foreground hover:text-foreground"
                      @click="toggleExpand(row.id)"
                    >
                      <ChevronDown v-if="expandedIds[row.id]" class="h-3 w-3" />
                      <ChevronRight v-else class="h-3 w-3" />
                    </Button>
                  </td>

                  <!-- Command + reason -->
                  <td class="px-3 py-2 align-middle">
                    <div class="space-y-0.5">
                      <code class="rounded-sm bg-secondary px-1.5 py-0.5 text-[11px] font-mono text-foreground">{{ row.command }}</code>
                      <p v-if="row.reason" class="text-[11px] text-muted-foreground">{{ row.reason }}</p>
                    </div>
                  </td>

                  <!-- Jira -->
                  <td class="px-3 py-2 align-middle">
                    <a
                      v-if="row.jira_key && jiraUrl(row.jira_key)"
                      :href="jiraUrl(row.jira_key)!"
                      target="_blank" rel="noopener"
                      class="text-[11px] font-mono text-primary hover:underline"
                    >
                      {{ row.jira_key }}
                    </a>
                    <span v-else-if="row.jira_key" class="text-[11px] font-mono text-primary">{{ row.jira_key }}</span>
                    <span v-else class="text-muted-foreground">-</span>
                  </td>

                  <!-- Status -->
                  <td class="px-3 py-2 align-middle">
                    <div class="flex items-center gap-1.5">
                      <Loader2 v-if="row.status === 'executing'" class="h-3 w-3 animate-spin text-primary" />
                      <Badge :variant="statusVariant(row.status)">{{ statusLabel(row.status) }}</Badge>
                    </div>
                  </td>

                  <!-- Executed At -->
                  <td class="px-3 py-2 align-middle">
                    <span v-if="row.executed_at" class="text-muted-foreground">{{ formatDate(row.executed_at!) }}</span>
                    <span v-else class="text-muted-foreground">-</span>
                  </td>

                  <!-- Created At -->
                  <td class="px-3 py-2 align-middle">
                    <span class="text-muted-foreground">{{ formatDate(row.created_at) }}</span>
                  </td>

                  <!-- Actions -->
                  <td class="px-3 py-2 text-right align-middle">
                    <div class="flex items-center justify-end gap-0.5">
                      <template v-if="row.status === 'pending'">
                        <template v-if="authStore.isSuperAdmin">
                          <Button variant="ghost" size="icon-sm" class="text-success hover:text-success" @click="approve(row.id)">
                            <Check class="h-3 w-3" />
                          </Button>
                          <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="reject(row.id)">
                            <X class="h-3 w-3" />
                          </Button>
                        </template>
                        <Button
                          v-if="isOwnRequest(row)"
                          variant="ghost" size="sm"
                          class="h-6 text-[10px] text-muted-foreground hover:text-foreground gap-1"
                          @click="withdraw(row.id)"
                        >
                          <Undo2 class="h-3 w-3" />
                          {{ t('approval.withdraw') }}
                        </Button>
                      </template>
                      <template v-else-if="canExpand(row) && row.status !== 'executing'">
                        <Button
                          variant="ghost" size="sm"
                          class="h-6 text-[10px] text-muted-foreground hover:text-foreground gap-1"
                          @click="toggleExpand(row.id)"
                        >
                          <EyeOff v-if="expandedIds[row.id]" class="h-3 w-3" />
                          <Eye v-else class="h-3 w-3" />
                          {{ expandedIds[row.id] ? t('approval.hideResult') : t('approval.viewResult') }}
                        </Button>
                      </template>
                    </div>
                  </td>
                </tr>

                <!-- Expanded detail row -->
                <tr v-if="expandedIds[row.id]" :key="`${row.id}-detail`" class="border-b border-border/40 bg-secondary/20">
                  <td :colspan="columns.length + 2" class="px-4 py-3">
                    <div v-if="row.status === 'executing'" class="flex items-center gap-2 text-primary">
                      <Loader2 class="h-4 w-4 animate-spin" />
                      <span class="text-xs">{{ t('approval.executing') }}...</span>
                    </div>

                    <div v-else-if="row.execution_result || row.status === 'approved'" class="space-y-2">
                      <div v-if="row.execution_result" class="flex items-center justify-between">
                        <div class="flex items-center gap-1.5">
                          <CheckCircle2 v-if="row.status === 'completed'" class="h-3.5 w-3.5 text-emerald-500" />
                          <XCircle v-else-if="row.status === 'failed'" class="h-3.5 w-3.5 text-destructive" />
                          <Info v-else class="h-3.5 w-3.5 text-primary" />
                          <span
                            class="text-[11px] font-medium"
                            :class="row.status === 'completed' ? 'text-emerald-500' : row.status === 'failed' ? 'text-destructive' : 'text-primary'"
                          >
                            {{ row.status === 'completed' ? t('approval.markedSuccess') : row.status === 'failed' ? t('approval.markedFailed') : t('approval.executionDone') }}
                          </span>
                        </div>
                      </div>
                      <!-- Admin mark buttons (hidden for terminal states) -->
                      <div v-if="authStore.isSuperAdmin && row.status !== 'completed' && row.status !== 'failed'" class="flex items-center gap-1">
                        <span class="text-[10px] text-muted-foreground mr-1">{{ t('approval.markAs') }}:</span>
                        <Button
                          variant="outline" size="sm"
                          class="h-5 text-[10px] px-2 text-emerald-500 border-emerald-500/30 hover:bg-emerald-500/10"
                          @click="markResult(row.id, true)"
                        >
                          <CheckCircle2 class="h-3 w-3 mr-0.5" />
                          {{ t('approval.success') }}
                        </Button>
                        <Button
                          variant="outline" size="sm"
                          class="h-5 text-[10px] px-2 text-destructive border-destructive/30 hover:bg-destructive/10"
                          @click="markResult(row.id, false)"
                        >
                          <XCircle class="h-3 w-3 mr-0.5" />
                          {{ t('approval.failure') }}
                        </Button>
                      </div>
                      <pre v-if="row.execution_result" class="max-h-80 overflow-y-auto rounded bg-[#0A0B10] border border-border/40 p-3 text-[11px] font-mono text-foreground/90 whitespace-pre-wrap break-words leading-relaxed">{{ getOutput(row) }}</pre>
                    </div>

                    <div v-else class="text-xs text-muted-foreground">
                      {{ t('approval.noOutput') }}
                    </div>
                  </td>
                </tr>
              </template>
            </template>

            <template v-else>
              <tr>
                <td :colspan="columns.length + 2" class="h-24 text-center">
                  <div class="flex flex-col items-center justify-center gap-1.5 text-muted-foreground">
                    <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8 opacity-30" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" />
                    </svg>
                    <span class="text-xs">{{ t('common.noData') }}</span>
                  </div>
                </td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>
