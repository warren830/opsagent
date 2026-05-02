<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, watch } from 'vue'
import {
  Rocket, RefreshCw, ChevronDown, ChevronRight, Play, RotateCcw,
  CheckCircle2, Loader2, AlertTriangle, Pause, Box, BarChart3,
  ChevronsDownUp, ChevronsUpDown, Server, Settings2, Info, Check, Minus,
} from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Switch } from '@/components/ui/switch'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { Separator } from '@/components/ui/separator'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
} from '@/components/ui/dialog'
import ConfirmDialog from '@/components/shared/ConfirmDialog.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

// ─── Types ─────────────────────────────────────────────────────────────────

interface Cluster {
  id: string
  name: string
  cloud: string
  cluster_type: string
  region: string | null
  status: string
}

interface RolloutSummary {
  name: string
  namespace: string
  strategy: string
  status: string
  desired_replicas: number
  ready_replicas: number
  updated_replicas: number
  canary_weight: number | null
  current_step: number | null
  total_steps: number | null
  active_service: string | null
  preview_service: string | null
  created_at: string | null
  updated_at: string | null
}

interface CanaryStep {
  index: number
  action: string
  value: unknown
  completed: boolean
}

interface ContainerInfo {
  name: string
  image: string
}

interface AnalysisMetric {
  name: string
  phase: string
  value: string | null
  message: string | null
}

interface AnalysisRunSummary {
  name: string
  phase: string
  metrics: AnalysisMetric[]
  created_at: string | null
}

interface RolloutDetail {
  summary: RolloutSummary
  canary_steps: CanaryStep[]
  containers: ContainerInfo[]
}

interface ClusterRollout {
  clusterId: string
  clusterName: string
  rollout: RolloutSummary
}

// ─── State ─────────────────────────────────────────────────────────────────

const clusters = ref<Cluster[]>([])
const clusterRollouts = ref<ClusterRollout[]>([])
const selectedClusterIds = ref<string[]>([])
const loading = ref(false)
const refreshing = ref(false)
const pollTimer = ref<ReturnType<typeof setInterval> | null>(null)
const clusterPopoverOpen = ref(false)

// Track rollouts that were just rolled back, so we can show a visual progress bar
// while Argo Rollouts terminates the canary pods. Keyed by rowKey(cr).
const rollingBack = ref<Set<string>>(new Set())

// Faster poll cadence for demo mode (VITE_DEMO_MODE) to surface rollback effects quickly.
const POLL_INTERVAL_MS = (import.meta.env?.VITE_DEMO_MODE === 'true' || import.meta.env?.VITE_DEMO_MODE === '1') ? 2000 : 30000

// Expanded row details
const expandedRows = ref<Set<string>>(new Set())
const rowDetails = ref<Map<string, { detail: RolloutDetail; analysis: AnalysisRunSummary[] }>>(new Map())

// Confirm dialog
const confirmOpen = ref(false)
const confirmTitle = ref('')
const confirmMessage = ref('')
const confirmAction = ref<(() => Promise<void>) | null>(null)

// Strategy switch dialog
const strategyDialogOpen = ref(false)
const strategyTarget = ref<ClusterRollout | null>(null)
const strategyType = ref<'canary' | 'blueGreen' | 'rollingUpdate'>('canary')
const canaryTemplate = ref<'conservative' | 'aggressive' | 'custom'>('conservative')
const bgActiveService = ref('')
const bgPreviewService = ref('')
const bgAutoPromotion = ref(false)
const strategySubmitting = ref(false)

// ─── Computed ──────────────────────────────────────────────────────────────

const hasCluster = computed(() => selectedClusterIds.value.length > 0)
const allClustersSelected = computed(() => clusters.value.length > 0 && selectedClusterIds.value.length === clusters.value.length)
const someClustersSelected = computed(() => selectedClusterIds.value.length > 0 && selectedClusterIds.value.length < clusters.value.length)
const allExpanded = computed(() => clusterRollouts.value.length > 0 && expandedRows.value.size === clusterRollouts.value.length)

const clusterSelectorLabel = computed(() => {
  const n = selectedClusterIds.value.length
  if (n === 0) return t('deployment.selectCluster')
  if (n === 1) {
    const c = clusters.value.find(c => c.id === selectedClusterIds.value[0])
    return c ? c.name : t('deployment.selectCluster')
  }
  return `${n} clusters`
})

// ─── Helpers ───────────────────────────────────────────────────────────────

function rowKey(cr: ClusterRollout): string {
  return `${cr.clusterId}/${cr.rollout.namespace}/${cr.rollout.name}`
}

function statusBadge(status: string) {
  switch (status) {
    case 'Healthy': return { class: 'bg-success/15 text-success border-success/40/20', icon: CheckCircle2 }
    case 'Progressing': return { class: 'bg-blue-500/15 text-blue-400 border-blue-500/20 animate-pulse', icon: Loader2 }
    case 'Degraded': return { class: 'bg-destructive/15 text-destructive border-destructive/40/20', icon: AlertTriangle }
    case 'Paused': return { class: 'bg-warning/15 text-warning border-warning/40/20', icon: Pause }
    default: return { class: 'bg-panel/15 text-muted-foreground border-border/20', icon: Box }
  }
}

function analysisPhaseColor(phase: string): string {
  switch (phase) {
    case 'Successful': return 'text-success'
    case 'Failed': case 'Error': return 'text-destructive'
    case 'Running': return 'text-blue-400'
    default: return 'text-muted-foreground'
  }
}

/** Replicas label: during canary show updated/desired, otherwise ready/desired */
function replicasLabel(r: RolloutSummary): string {
  if (r.status === 'Paused' || r.status === 'Progressing') {
    return `${r.updated_replicas}+${r.ready_replicas - r.updated_replicas}/${r.desired_replicas}`
  }
  return `${r.ready_replicas}/${r.desired_replicas}`
}

// ─── Cluster multi-select ─────────────────────────────────────────────────

function toggleCluster(id: string) {
  const idx = selectedClusterIds.value.indexOf(id)
  if (idx >= 0) {
    selectedClusterIds.value = selectedClusterIds.value.filter(v => v !== id)
  } else {
    selectedClusterIds.value = [...selectedClusterIds.value, id]
  }
}

function toggleAllClusters() {
  if (allClustersSelected.value) {
    selectedClusterIds.value = []
  } else {
    selectedClusterIds.value = clusters.value.map(c => c.id)
  }
}

// ─── API Calls ─────────────────────────────────────────────────────────────

async function fetchClusters() {
  try {
    clusters.value = await api.get<Cluster[]>('/api/clusters')
  } catch { /* silent */ }
}

async function fetchRollouts() {
  if (!hasCluster.value) return
  loading.value = true
  try {
    const ids = selectedClusterIds.value
    const results = await Promise.allSettled(
      ids.map(async (cid) => {
        const cluster = clusters.value.find(c => c.id === cid)
        const rollouts = await api.get<RolloutSummary[]>(`/api/clusters/${cid}/rollouts`)
        return rollouts.map(r => ({
          clusterId: cid,
          clusterName: cluster?.name ?? cid,
          rollout: r,
        }))
      })
    )
    const all: ClusterRollout[] = []
    for (const r of results) {
      if (r.status === 'fulfilled') all.push(...r.value)
    }
    clusterRollouts.value = all
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : 'Failed to fetch rollouts'
    toast.error(msg)
    clusterRollouts.value = []
  } finally {
    loading.value = false
  }
}

async function refreshRollouts() {
  refreshing.value = true
  await fetchRollouts()
  refreshing.value = false
}

async function fetchRowDetail(cr: ClusterRollout) {
  const key = rowKey(cr)
  const r = cr.rollout
  try {
    const [detail, analysis] = await Promise.all([
      api.get<RolloutDetail>(`/api/clusters/${cr.clusterId}/rollouts/${r.namespace}/${r.name}`),
      api.get<AnalysisRunSummary[]>(`/api/clusters/${cr.clusterId}/rollouts/${r.namespace}/${r.name}/analysis`),
    ])
    rowDetails.value.set(key, { detail, analysis })
  } catch {
    // If detail fetch fails, still show basic info
  }
}

async function toggleRow(cr: ClusterRollout) {
  const key = rowKey(cr)
  const s = new Set(expandedRows.value)
  if (s.has(key)) {
    s.delete(key)
  } else {
    s.add(key)
    if (!rowDetails.value.has(key)) {
      await fetchRowDetail(cr)
    }
  }
  expandedRows.value = s
}

function expandAll() {
  expandedRows.value = new Set(clusterRollouts.value.map(cr => rowKey(cr)))
  for (const cr of clusterRollouts.value) {
    if (!rowDetails.value.has(rowKey(cr))) fetchRowDetail(cr)
  }
}

function collapseAll() {
  expandedRows.value = new Set()
}

// ─── Actions ───────────────────────────────────────────────────────────────

async function promoteRollout(cr: ClusterRollout, full: boolean) {
  const r = cr.rollout
  try {
    await api.post(`/api/clusters/${cr.clusterId}/rollouts/${r.namespace}/${r.name}/promote`, { full })
    toast.success(`${full ? 'Full promotion' : 'Step promotion'}: ${r.namespace}/${r.name}`)
    await refreshRollouts()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : 'Promote failed'
    toast.error(msg)
  }
}

function confirmRollback(cr: ClusterRollout) {
  const r = cr.rollout
  const key = rowKey(cr)
  confirmTitle.value = t('deployment.rollback')
  confirmMessage.value = t('deployment.rollbackConfirm')
  confirmAction.value = async () => {
    try {
      await api.post(`/api/clusters/${cr.clusterId}/rollouts/${r.namespace}/${r.name}/rollback`, {})
      toast.success(`Rolled back: ${r.namespace}/${r.name}`)
      // Flag this rollout as rolling back so the UI shows a live progress bar
      // while Argo Rollouts terminates the bad revision's pods.
      rollingBack.value.add(key)
      rollingBack.value = new Set(rollingBack.value)
      setTimeout(() => {
        rollingBack.value.delete(key)
        rollingBack.value = new Set(rollingBack.value)
      }, 15000)
      await refreshRollouts()
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Rollback failed'
      toast.error(msg)
    }
  }
  confirmOpen.value = true
}

async function handleConfirm() {
  if (confirmAction.value) await confirmAction.value()
  confirmOpen.value = false
}

// ─── Strategy Switch ──────────────────────────────────────────────────────

function openStrategyDialog(cr: ClusterRollout) {
  strategyTarget.value = cr
  // Pre-fill based on current strategy
  const s = cr.rollout.strategy
  if (s === 'BlueGreen') {
    strategyType.value = 'blueGreen'
    bgActiveService.value = cr.rollout.active_service || `${cr.rollout.name}-active`
    bgPreviewService.value = cr.rollout.preview_service || `${cr.rollout.name}-preview`
  } else {
    strategyType.value = 'canary'
    bgActiveService.value = `${cr.rollout.name}-active`
    bgPreviewService.value = `${cr.rollout.name}-preview`
  }
  canaryTemplate.value = 'conservative'
  bgAutoPromotion.value = false
  strategySubmitting.value = false
  strategyDialogOpen.value = true
}

const canaryTemplateSteps = computed(() => {
  switch (canaryTemplate.value) {
    case 'conservative':
      return [
        { setWeight: 20 },
        { pause: {} },
        { setWeight: 50 },
        { pause: { duration: '60s' } },
        { setWeight: 80 },
        { pause: { duration: '30s' } },
      ]
    case 'aggressive':
      return [
        { setWeight: 50 },
        { pause: {} },
      ]
    default:
      return []
  }
})

async function submitStrategy() {
  if (!strategyTarget.value) return
  strategySubmitting.value = true
  const cr = strategyTarget.value
  const r = cr.rollout
  try {
    const body: Record<string, unknown> = { strategy: strategyType.value }
    if (strategyType.value === 'canary' && canaryTemplate.value !== 'custom') {
      body.canarySteps = canaryTemplateSteps.value
    }
    if (strategyType.value === 'blueGreen') {
      body.activeService = bgActiveService.value
      body.previewService = bgPreviewService.value
      body.autoPromotionEnabled = bgAutoPromotion.value
    }
    await api.post(`/api/clusters/${cr.clusterId}/rollouts/${r.namespace}/${r.name}/strategy`, body)
    toast.success(t('deployment.strategyChangeSuccess'))
    strategyDialogOpen.value = false
    await refreshRollouts()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('deployment.strategyChangeFailed')
    toast.error(msg)
  } finally {
    strategySubmitting.value = false
  }
}

// ─── Lifecycle ─────────────────────────────────────────────────────────────

watch(selectedClusterIds, () => {
  clusterRollouts.value = []
  expandedRows.value = new Set()
  rowDetails.value = new Map()
  fetchRollouts()
})

onMounted(async () => {
  await fetchClusters()
  pollTimer.value = setInterval(() => {
    if (hasCluster.value) fetchRollouts()
  }, POLL_INTERVAL_MS)
})

onUnmounted(() => {
  if (pollTimer.value) clearInterval(pollTimer.value)
})
</script>

<template>
  <div class="p-6 space-y-6 max-w-[1400px] mx-auto">
    <!-- Header -->
    <div class="flex items-center justify-between flex-wrap gap-3">
      <div class="flex items-center gap-3">
        <div class="p-2 rounded-lg bg-info/10 border border-info/40/20">
          <Rocket class="h-5 w-5 text-info" />
        </div>
        <div>
          <h1 class="text-lg font-semibold text-foreground">{{ t('deployment.title') }}</h1>
          <p class="text-[11px] text-muted-foreground">Argo Rollouts &middot; {{ t('deployment.autoRefresh') }}</p>
        </div>
      </div>

      <div class="flex items-center gap-2">
        <!-- Cluster multi-select (shadcn Popover + Checkbox) -->
        <Popover v-model:open="clusterPopoverOpen">
          <PopoverTrigger as-child>
            <Button variant="outline" size="sm" class="h-8 px-3 text-xs bg-background border-border text-foreground hover:bg-muted hover:border-border">
              <Server class="h-3.5 w-3.5 mr-2 text-muted-foreground" />
              <span class="max-w-[180px] truncate">{{ clusterSelectorLabel }}</span>
              <ChevronDown class="h-3.5 w-3.5 ml-2 text-muted-foreground transition-transform" :class="{ 'rotate-180': clusterPopoverOpen }" />
            </Button>
          </PopoverTrigger>
          <PopoverContent class="w-[300px] p-0 bg-background border-border" align="end">
            <!-- Select all row -->
            <div class="flex items-center gap-2.5 px-3 py-2 cursor-pointer hover:bg-muted transition-colors" @click="toggleAllClusters">
              <div
                class="h-4 w-4 rounded border flex items-center justify-center shrink-0 transition-colors"
                :class="allClustersSelected
                  ? 'bg-info border-info/40 text-foreground'
                  : someClustersSelected
                    ? 'bg-info/50 border-info/40 text-foreground'
                    : 'border-border'"
              >
                <Minus v-if="someClustersSelected && !allClustersSelected" class="h-3 w-3" />
                <Check v-else-if="allClustersSelected" class="h-3 w-3" />
              </div>
              <span class="text-xs text-muted-foreground font-medium">{{ t('deployment.selectCluster') }}</span>
              <span class="text-[10px] text-muted-foreground ml-auto">{{ selectedClusterIds.length }}/{{ clusters.length }}</span>
            </div>
            <Separator class="bg-muted" />
            <!-- Cluster list -->
            <div class="py-1 max-h-[240px] overflow-y-auto">
              <div
                v-for="c in clusters"
                :key="c.id"
                class="flex items-center gap-2.5 w-full px-3 py-1.5 cursor-pointer hover:bg-muted transition-colors"
                @click="toggleCluster(c.id)"
              >
                <div
                  class="h-4 w-4 rounded border flex items-center justify-center shrink-0 transition-colors"
                  :class="selectedClusterIds.includes(c.id)
                    ? 'bg-info border-info/40 text-foreground'
                    : 'border-border'"
                >
                  <Check v-if="selectedClusterIds.includes(c.id)" class="h-3 w-3" />
                </div>
                <span class="text-xs text-foreground truncate flex-1">{{ c.name }}</span>
                <span class="text-[11px] text-muted-foreground shrink-0">{{ c.region || c.cloud }}</span>
              </div>
            </div>
          </PopoverContent>
        </Popover>

        <Tooltip v-if="hasCluster">
          <TooltipTrigger as-child>
            <Button
              variant="ghost" size="sm"
              class="h-8 w-8 p-0"
              :disabled="refreshing"
              @click="refreshRollouts"
            >
              <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': refreshing }" />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom"><p class="text-xs">Refresh</p></TooltipContent>
        </Tooltip>

        <Button
          v-if="clusterRollouts.length > 0"
          variant="ghost" size="sm"
          class="h-8 px-2 text-xs"
          @click="allExpanded ? collapseAll() : expandAll()"
        >
          <component :is="allExpanded ? ChevronsDownUp : ChevronsUpDown" class="h-3.5 w-3.5 mr-1" />
          {{ allExpanded ? t('nav.collapseAll') : t('nav.expandAll') }}
        </Button>
      </div>
    </div>

    <!-- Empty state -->
    <div v-if="!hasCluster" class="flex flex-col items-center justify-center py-20 text-muted-foreground">
      <Rocket class="h-10 w-10 mb-3 opacity-30" />
      <p class="text-sm">{{ t('deployment.noCluster') }}</p>
    </div>

    <!-- Loading -->
    <div v-else-if="loading && clusterRollouts.length === 0" class="flex items-center justify-center py-20">
      <Loader2 class="h-6 w-6 animate-spin text-info" />
    </div>

    <!-- No rollouts -->
    <div v-else-if="clusterRollouts.length === 0 && !loading" class="flex flex-col items-center justify-center py-20 text-muted-foreground">
      <Box class="h-10 w-10 mb-3 opacity-30" />
      <p class="text-sm">{{ t('deployment.noRollouts') }}</p>
    </div>

    <!-- Rollouts list -->
    <div v-else class="space-y-1.5">
      <template v-for="cr in clusterRollouts" :key="rowKey(cr)">
        <Collapsible :open="expandedRows.has(rowKey(cr))" @update:open="toggleRow(cr)">
          <!-- Main row -->
          <CollapsibleTrigger as-child>
            <div class="group relative flex items-center gap-3 px-4 py-2.5 rounded-lg bg-panel/60 backdrop-blur-sm border border-border hover:border-border transition-colors cursor-pointer overflow-hidden">
              <!-- Rollback progress bar (appears for ~15s after rollback is triggered) -->
              <div
                v-if="rollingBack.has(rowKey(cr))"
                class="absolute inset-x-0 top-0 h-[3px] bg-gradient-to-r from-rose-500 via-amber-500 to-rose-500 bg-[length:200%_100%] rollback-bar-anim"
              />
              <!-- Expand icon -->
              <component
                :is="expandedRows.has(rowKey(cr)) ? ChevronDown : ChevronRight"
                class="h-3.5 w-3.5 text-muted-foreground shrink-0"
              />

              <!-- Name + namespace + cluster -->
              <div class="min-w-0 flex-1 flex items-center gap-1.5 overflow-hidden">
                <span class="text-sm font-medium text-foreground truncate">{{ cr.rollout.name }}</span>
                <span class="text-xs text-muted-foreground shrink-0">{{ cr.rollout.namespace }}</span>
                <span v-if="selectedClusterIds.length > 1" class="text-[11px] text-muted-foreground truncate shrink-0">@ {{ cr.clusterName }}</span>
              </div>

              <!-- Strategy -->
              <Badge variant="outline" class="text-xs px-2 py-0.5 border-border text-muted-foreground shrink-0 hidden sm:inline-flex">
                {{ cr.rollout.strategy }}
              </Badge>

              <!-- Rolling-back indicator -->
              <Badge
                v-if="rollingBack.has(rowKey(cr))"
                class="text-[10px] px-2 py-0.5 bg-destructive/10 text-destructive border border-destructive/40/20 shrink-0"
              >
                <Loader2 class="h-3 w-3 mr-1 animate-spin" />
                {{ t('deployment.rollingBack') }}
              </Badge>

              <!-- Status -->
              <Badge :class="['text-xs px-2 py-0.5 border shrink-0', statusBadge(cr.rollout.status).class]">
                <component :is="statusBadge(cr.rollout.status).icon" class="h-3.5 w-3.5 mr-1" />
                {{ cr.rollout.status }}
              </Badge>

              <!-- Replicas -->
              <Tooltip>
                <TooltipTrigger as-child>
                  <div class="text-xs text-muted-foreground shrink-0 tabular-nums">
                    {{ replicasLabel(cr.rollout) }}
                  </div>
                </TooltipTrigger>
                <TooltipContent side="bottom">
                  <p class="text-xs">{{ cr.rollout.ready_replicas }} ready / {{ cr.rollout.desired_replicas }} desired</p>
                  <p v-if="cr.rollout.updated_replicas !== cr.rollout.ready_replicas" class="text-xs text-muted-foreground">{{ cr.rollout.updated_replicas }} updated (canary)</p>
                </TooltipContent>
              </Tooltip>

              <!-- Steps -->
              <div v-if="cr.rollout.total_steps" class="text-xs text-muted-foreground shrink-0 tabular-nums">
                {{ (cr.rollout.current_step ?? 0) + 1 }}/{{ cr.rollout.total_steps }}
              </div>

              <!-- Actions -->
              <div
                :class="['flex items-center gap-1 shrink-0', cr.rollout.status === 'Paused' ? '' : 'opacity-0 group-hover:opacity-100 transition-opacity']"
                @click.stop
              >
                <template v-if="cr.rollout.status === 'Paused'">
                  <Tooltip>
                    <TooltipTrigger as-child>
                      <Button variant="ghost" size="sm" class="h-7 px-2.5 text-xs text-blue-400 hover:text-blue-300 hover:bg-blue-500/10" @click="promoteRollout(cr, false)">
                        <Play class="h-3.5 w-3.5 mr-1" /> Step
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent side="bottom"><p class="text-xs">Promote to next step</p></TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger as-child>
                      <Button variant="ghost" size="sm" class="h-7 px-2.5 text-xs text-success hover:text-success hover:bg-success/10" @click="promoteRollout(cr, true)">
                        <Play class="h-3.5 w-3.5 mr-1" /> Full
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent side="bottom"><p class="text-xs">Promote to 100%</p></TooltipContent>
                  </Tooltip>
                </template>
                <Tooltip v-if="cr.rollout.status === 'Progressing' || cr.rollout.status === 'Paused'">
                  <TooltipTrigger as-child>
                    <Button variant="ghost" size="sm" class="h-7 w-7 p-0 text-destructive hover:text-destructive hover:bg-destructive/10" @click="confirmRollback(cr)">
                      <RotateCcw class="h-3.5 w-3.5" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side="bottom"><p class="text-xs">Rollback</p></TooltipContent>
                </Tooltip>
                <Tooltip>
                  <TooltipTrigger as-child>
                    <Button
                      variant="ghost" size="sm"
                      class="h-7 w-7 p-0 text-muted-foreground hover:text-foreground hover:bg-panel/10"
                      :disabled="cr.rollout.status === 'Progressing' || cr.rollout.status === 'Paused'"
                      @click="openStrategyDialog(cr)"
                    >
                      <Settings2 class="h-3.5 w-3.5" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side="bottom">
                    <p class="text-xs">
                      {{ (cr.rollout.status === 'Progressing' || cr.rollout.status === 'Paused')
                        ? t('deployment.strategyDisabledDuringRollout')
                        : t('deployment.changeStrategy') }}
                    </p>
                  </TooltipContent>
                </Tooltip>
              </div>
            </div>
          </CollapsibleTrigger>

          <!-- Expanded detail -->
          <CollapsibleContent>
            <div class="ml-6 mr-2 mb-2 mt-1 p-4 rounded-lg bg-muted border border-border space-y-4">
              <template v-if="rowDetails.has(rowKey(cr))">
                <!-- Canary Steps -->
                <div v-if="rowDetails.get(rowKey(cr))!.detail.canary_steps.length > 0">
                  <p class="text-xs text-muted-foreground font-medium mb-2.5">{{ t('deployment.canarySteps') }}</p>
                  <div class="flex items-center gap-1.5 flex-wrap">
                    <template v-for="step in rowDetails.get(rowKey(cr))!.detail.canary_steps" :key="step.index">
                      <div
                        :class="[
                          'px-2.5 py-1 rounded text-xs font-medium border transition-all flex items-center gap-1',
                          step.completed
                            ? 'bg-success/15 text-success border-success/40/20'
                            : step.index === (cr.rollout.current_step ?? -1)
                              ? 'bg-blue-500/20 text-blue-300 border-blue-500/30 animate-pulse'
                              : 'bg-muted text-muted-foreground border-border'
                        ]"
                      >
                        <Pause v-if="step.action === 'pause'" class="h-3 w-3" />
                        {{ step.action === 'setWeight' ? `${step.value}%` : step.action === 'pause' ? (String(step.value) === '"indefinite"' || step.value === 'indefinite' ? 'Pause' : `Pause ${step.value}`) : step.action }}
                      </div>
                      <span v-if="step.index < rowDetails.get(rowKey(cr))!.detail.canary_steps.length - 1" class="text-muted-foreground/80">→</span>
                    </template>
                  </div>
                </div>

                <!-- Containers -->
                <div v-if="rowDetails.get(rowKey(cr))!.detail.containers.length > 0">
                  <p class="text-xs text-muted-foreground font-medium mb-1.5">{{ t('deployment.containers') }}</p>
                  <div v-for="c in rowDetails.get(rowKey(cr))!.detail.containers" :key="c.name" class="text-xs text-muted-foreground py-0.5">
                    <span class="text-foreground font-medium">{{ c.name }}</span>
                    <span class="text-muted-foreground mx-1.5">→</span>
                    <code class="text-muted-foreground text-[11px]">{{ c.image }}</code>
                  </div>
                </div>

                <!-- Analysis Runs -->
                <div v-if="rowDetails.get(rowKey(cr))!.analysis.length > 0">
                  <p class="text-xs text-muted-foreground font-medium mb-1.5">{{ t('deployment.analysisRuns') }}</p>
                  <div v-for="ar in rowDetails.get(rowKey(cr))!.analysis" :key="ar.name" class="flex items-center gap-2 text-xs">
                    <BarChart3 class="h-3.5 w-3.5 text-muted-foreground" />
                    <span class="text-foreground">{{ ar.name }}</span>
                    <Badge :class="['text-[10px] px-1.5 py-0', analysisPhaseColor(ar.phase)]">{{ ar.phase }}</Badge>
                    <span v-for="m in ar.metrics" :key="m.name" class="text-muted-foreground">
                      {{ m.name }}: <span :class="analysisPhaseColor(m.phase)">{{ m.value || m.phase }}</span>
                    </span>
                  </div>
                </div>
              </template>

              <div v-else class="flex items-center justify-center py-4">
                <Loader2 class="h-5 w-5 animate-spin text-muted-foreground" />
              </div>
            </div>
          </CollapsibleContent>
        </Collapsible>
      </template>
    </div>

    <!-- Confirm dialog -->
    <ConfirmDialog
      :open="confirmOpen"
      :title="confirmTitle"
      :description="confirmMessage"
      @confirm="handleConfirm"
      @cancel="confirmOpen = false"
    />

    <!-- Strategy Switch Dialog -->
    <Dialog :open="strategyDialogOpen" @update:open="(v) => { if (!v) strategyDialogOpen = false }">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle class="flex items-center gap-2">
            <Settings2 class="h-4 w-4 text-info" />
            {{ t('deployment.changeStrategy') }}
          </DialogTitle>
          <DialogDescription v-if="strategyTarget">
            {{ strategyTarget.rollout.namespace }}/{{ strategyTarget.rollout.name }}
            <Badge variant="outline" class="ml-2 text-[10px] px-1.5 py-0 border-border text-muted-foreground">
              {{ t('deployment.currentStrategy') }}: {{ strategyTarget.rollout.strategy }}
            </Badge>
          </DialogDescription>
        </DialogHeader>

        <div class="space-y-4 py-2">
          <!-- Strategy type selector -->
          <div class="space-y-1.5">
            <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('deployment.strategyType') }}</label>
            <Select v-model="strategyType">
              <SelectTrigger class="h-9 bg-background border-border text-foreground">
                <SelectValue />
              </SelectTrigger>
              <SelectContent class="bg-background border-border">
                <SelectItem value="canary" class="text-foreground">
                  <span class="flex items-center gap-2">🐤 {{ t('deployment.canary') }}</span>
                </SelectItem>
                <SelectItem value="blueGreen" class="text-foreground">
                  <span class="flex items-center gap-2">🔵🟢 {{ t('deployment.blueGreen') }}</span>
                </SelectItem>
                <SelectItem value="rollingUpdate" class="text-foreground">
                  <span class="flex items-center gap-2">🔄 {{ t('deployment.rollingUpdate') }}</span>
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          <Separator class="bg-muted" />

          <!-- Canary options -->
          <div v-if="strategyType === 'canary'" class="space-y-3">
            <div class="space-y-1.5">
              <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('deployment.canaryTemplate') }}</label>
              <Select v-model="canaryTemplate">
                <SelectTrigger class="h-9 bg-background border-border text-foreground">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent class="bg-background border-border">
                  <SelectItem value="conservative" class="text-foreground">
                    {{ t('deployment.conservative') }}
                  </SelectItem>
                  <SelectItem value="aggressive" class="text-foreground">
                    {{ t('deployment.aggressive') }}
                  </SelectItem>
                  <SelectItem value="custom" class="text-foreground">
                    {{ t('deployment.custom') }}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
            <!-- Steps preview -->
            <div v-if="canaryTemplate !== 'custom'" class="flex items-center gap-1.5 flex-wrap px-1">
              <template v-for="(step, i) in canaryTemplateSteps" :key="i">
                <div class="px-2 py-1 rounded text-xs font-medium border bg-muted text-muted-foreground border-border flex items-center gap-1">
                  <Pause v-if="step.pause !== undefined" class="h-3 w-3" />
                  {{ step.setWeight ? `${step.setWeight}%` : step.pause?.duration ? `Pause ${step.pause.duration}` : 'Pause ∞' }}
                </div>
                <span v-if="i < canaryTemplateSteps.length - 1" class="text-muted-foreground/80">→</span>
              </template>
              <div class="px-2 py-1 rounded text-xs font-medium border bg-success/10 text-success border-success/40/20">100%</div>
            </div>
            <p v-else class="text-[11px] text-muted-foreground px-1">
              No predefined steps — the rollout will proceed immediately to 100%.
            </p>
          </div>

          <!-- Blue-Green options -->
          <div v-else-if="strategyType === 'blueGreen'" class="space-y-3">
            <!-- Warning banner -->
            <div class="flex gap-2.5 p-3 rounded-lg bg-warning/5 border border-warning/40/15">
              <Info class="h-4 w-4 text-warning shrink-0 mt-0.5" />
              <p class="text-[11px] leading-relaxed text-warning/90">
                {{ t('deployment.blueGreenWarning') }}
              </p>
            </div>

            <div class="space-y-1.5">
              <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('deployment.activeService') }}</label>
              <Input
                v-model="bgActiveService"
                class="h-9 bg-background border-border text-foreground text-sm"
                placeholder="my-app-active"
              />
            </div>
            <div class="space-y-1.5">
              <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('deployment.previewService') }}</label>
              <Input
                v-model="bgPreviewService"
                class="h-9 bg-background border-border text-foreground text-sm"
                placeholder="my-app-preview"
              />
              <p class="text-[10px] text-muted-foreground">Auto-created if not existing</p>
            </div>
            <div class="flex items-center justify-between">
              <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('deployment.autoPromotion') }}</label>
              <Switch
                :checked="bgAutoPromotion"
                class="data-[state=checked]:bg-info"
                @update:checked="(v: boolean) => bgAutoPromotion = v"
              />
            </div>
          </div>

          <!-- Rolling Update info -->
          <div v-else-if="strategyType === 'rollingUpdate'" class="px-1">
            <div class="flex gap-2.5 p-3 rounded-lg bg-blue-500/5 border border-blue-500/15">
              <Info class="h-4 w-4 text-blue-400 shrink-0 mt-0.5" />
              <p class="text-[11px] leading-relaxed text-blue-300/90">
                {{ t('deployment.rollingUpdateDesc') }}
              </p>
            </div>
          </div>
        </div>

        <DialogFooter class="gap-1.5">
          <Button variant="outline" size="sm" @click="strategyDialogOpen = false">
            {{ t('common.cancel') }}
          </Button>
          <Button
            size="sm"
            :disabled="strategySubmitting || (strategyType === 'blueGreen' && (!bgActiveService || !bgPreviewService))"
            class="bg-gradient-to-r from-sky-500 to-violet-500 text-foreground hover:brightness-110"
            @click="submitStrategy"
          >
            <Loader2 v-if="strategySubmitting" class="h-3.5 w-3.5 mr-1.5 animate-spin" />
            {{ t('deployment.applyStrategy') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>

<style scoped>
/* Rollback progress bar — sweeping gradient to convey "work in progress" */
.rollback-bar-anim {
  animation: rollback-sweep 1.2s linear infinite;
}
@keyframes rollback-sweep {
  0%   { background-position: 100% 0; }
  100% { background-position: -100% 0; }
}
</style>
