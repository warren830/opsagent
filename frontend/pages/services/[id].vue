<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { toast } from 'vue-sonner'
import {
  ArrowLeft,
  ChevronRight,
  ExternalLink,
  FileText,
  Layers,
  Loader2,
  Pencil,
  Rocket,
  RotateCcw,
  Settings2,
  Flag,
  BookOpen,
  Activity,
  AlertOctagon,
  Target,
  Users2,
  Server,
  Hash,
  Clock3,
  Link2,
  Trash2,
} from 'lucide-vue-next'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import BudgetBar from '@/components/slos/BudgetBar.vue'
import BurnRateBadge from '@/components/slos/BurnRateBadge.vue'
import ConfirmDialog from '@/components/shared/ConfirmDialog.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const route = useRoute()
const router = useRouter()

// ───────────────────────── Types ─────────────────────────

interface CatalogEntity {
  id: string
  tenant_id: string
  kind: string
  name: string
  display_name: string | null
  description: string | null
  lifecycle: string
  owner_group_id: string | null
  system_id: string | null
  tags: string[]
  annotations: Record<string, unknown>
  source_url: string | null
  source_ref: string | null
  spec: Record<string, unknown>
  created_at: string
  updated_at: string
}

interface CatalogRelation {
  id: string
  from_id: string
  to_id: string
  relation_type: string
  created_at: string
}

interface Slo {
  id: string
  component_id: string | null
  name: string
  description: string | null
  sli_type: string
  objective_pct: number
  window_days: number
  enabled: boolean
}

interface BudgetSnapshot {
  id: string
  slo_id: string
  captured_at: string
  sli_achieved_pct: number
  budget_total_minutes: number
  budget_consumed_minutes: number
  budget_remaining_pct: number
  burn_rate_1h: number | null
  burn_rate_6h: number | null
}

interface Incident {
  id: string
  number: number
  title: string
  severity: 'sev1' | 'sev2' | 'sev3' | 'sev4'
  status: string
  impact_summary: string | null
  affected_component_ids: string[]
  detection_source: string
  started_at: string
  detected_at: string
  resolved_at: string | null
  closed_at: string | null
}

interface ChangeEvent {
  id: string
  kind: string
  title: string
  description: string | null
  service_id: string | null
  actor: string | null
  occurred_at: string
  metadata?: Record<string, unknown>
}

// ───────────────────────── State ─────────────────────────

const entityId = computed(() => route.params.id as string)
const entity = ref<CatalogEntity | null>(null)
const loading = ref(true)

const relations = ref<CatalogRelation[]>([])
const relatedIndex = ref<Map<string, CatalogEntity>>(new Map())

const slos = ref<Slo[]>([])
const budgets = ref<Record<string, BudgetSnapshot | null>>({})
const slosLoading = ref(false)

const incidents = ref<Incident[]>([])
const incidentsLoading = ref(false)

const changes = ref<ChangeEvent[]>([])
const changesLoading = ref(false)
const changesUnavailable = ref(false)

type TabId = 'overview' | 'slos' | 'incidents' | 'changes' | 'runbooks' | 'docs'
const activeTab = ref<TabId>('overview')

const showDelete = ref(false)

// ───────────────────────── Helpers ───────────────────────

function lifecycleVariant(lc: string): 'success' | 'info' | 'secondary' | 'destructive' {
  switch (lc) {
    case 'production': return 'success'
    case 'experimental': return 'info'
    case 'deprecated': return 'secondary'
    case 'retired': return 'destructive'
    default: return 'secondary'
  }
}

function kindVariant(kind: string): 'default' | 'info' | 'success' | 'warning' | 'secondary' {
  switch (kind) {
    case 'system': return 'default'
    case 'component': return 'info'
    case 'api': return 'warning'
    case 'resource': return 'success'
    case 'group': return 'secondary'
    default: return 'secondary'
  }
}

const SEVERITY_STYLE: Record<string, { variant: 'destructive' | 'warning' | 'info' | 'secondary'; label: string }> = {
  sev1: { variant: 'destructive', label: 'SEV1' },
  sev2: { variant: 'warning', label: 'SEV2' },
  sev3: { variant: 'info', label: 'SEV3' },
  sev4: { variant: 'secondary', label: 'SEV4' },
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  })
}

function changeKindMeta(kind: string): {
  variant: 'success' | 'warning' | 'destructive' | 'info' | 'secondary'
  icon: typeof Rocket
  label: string
} {
  switch (kind) {
    case 'deploy':
      return { variant: 'success', icon: Rocket, label: t('services.changes.kinds.deploy') }
    case 'rollback':
      return { variant: 'warning', icon: RotateCcw, label: t('services.changes.kinds.rollback') }
    case 'slo_burn':
      return { variant: 'destructive', icon: Flag, label: t('services.changes.kinds.slo_burn') }
    case 'config':
      return { variant: 'info', icon: Settings2, label: t('services.changes.kinds.config') }
    case 'feature_flag':
      return { variant: 'info', icon: Flag, label: t('services.changes.kinds.feature_flag') }
    case 'manual':
      return { variant: 'secondary', icon: Pencil, label: t('services.changes.kinds.manual') }
    case 'catalog_import':
      return { variant: 'secondary', icon: Layers, label: t('services.changes.kinds.catalog_import') }
    default:
      return { variant: 'secondary', icon: Activity, label: kind }
  }
}

// Runtime info extracted from spec (components set runtime in their spec).
const runtime = computed(() => {
  const spec = entity.value?.spec as Record<string, unknown> | undefined
  const rt = spec?.runtime as Record<string, unknown> | undefined
  if (!rt) return null
  return {
    kind: typeof rt.kind === 'string' ? rt.kind : null,
    cluster_id: typeof rt.cluster_id === 'string' ? rt.cluster_id : null,
    namespace: typeof rt.namespace === 'string' ? rt.namespace : null,
    workload_name: typeof rt.workload_name === 'string' ? rt.workload_name : null,
    aws_arn: typeof rt.aws_arn === 'string' ? rt.aws_arn : null,
  }
})

const consumesApis = computed<string[]>(() => {
  const spec = entity.value?.spec as Record<string, unknown> | undefined
  const arr = spec?.consumes_apis as unknown
  return Array.isArray(arr) ? arr.filter((x): x is string => typeof x === 'string') : []
})

const providesApis = computed<string[]>(() => {
  const spec = entity.value?.spec as Record<string, unknown> | undefined
  const arr = spec?.provides_apis as unknown
  return Array.isArray(arr) ? arr.filter((x): x is string => typeof x === 'string') : []
})

const dependsOnResources = computed<string[]>(() => {
  const spec = entity.value?.spec as Record<string, unknown> | undefined
  const arr = spec?.depends_on_resources as unknown
  return Array.isArray(arr) ? arr.filter((x): x is string => typeof x === 'string') : []
})

interface RelatedEntry {
  relation: CatalogRelation
  other: CatalogEntity | null
}

const outgoingDeps = computed<RelatedEntry[]>(() => {
  if (!entity.value) return []
  const me = entity.value.id
  return relations.value
    .filter(r => r.from_id === me && (r.relation_type === 'consumes' || r.relation_type === 'depends_on'))
    .map(r => ({ relation: r, other: relatedIndex.value.get(r.to_id) ?? null }))
})

const incomingConsumers = computed<RelatedEntry[]>(() => {
  if (!entity.value) return []
  const me = entity.value.id
  return relations.value
    .filter(r => r.to_id === me && (r.relation_type === 'consumes' || r.relation_type === 'depends_on' || r.relation_type === 'provides'))
    .map(r => ({ relation: r, other: relatedIndex.value.get(r.from_id) ?? null }))
})

const activeSloCount = computed(() => slos.value.filter(s => s.enabled).length)

const activeIncidentCount = computed(() =>
  incidents.value.filter(i => !i.resolved_at && !i.closed_at).length,
)

const deployCount30d = computed(() => {
  const cutoff = Date.now() - 30 * 24 * 3600 * 1000
  return changes.value.filter(
    c => (c.kind === 'deploy' || c.kind === 'rollback')
      && new Date(c.occurred_at).getTime() >= cutoff,
  ).length
})

const worstBurnRate = computed<number | null>(() => {
  let worst: number | null = null
  for (const s of slos.value) {
    const snap = budgets.value[s.id]
    if (!snap) continue
    const candidates = [snap.burn_rate_1h, snap.burn_rate_6h].filter(
      (x): x is number => typeof x === 'number' && Number.isFinite(x),
    )
    for (const v of candidates) {
      if (worst === null || v > worst) worst = v
    }
  }
  return worst
})

const runbookUrl = computed<string | null>(() => {
  const ann = entity.value?.annotations as Record<string, unknown> | undefined
  if (!ann) return null
  const val = ann['runbook.url'] ?? ann['runbook/url']
  return typeof val === 'string' ? val : null
})

const docsUrl = computed<string | null>(() => {
  const ann = entity.value?.annotations as Record<string, unknown> | undefined
  const byAnn = ann ? (ann['docs.url'] ?? ann['docs/url']) : null
  if (typeof byAnn === 'string') return byAnn
  return entity.value?.source_url ?? null
})

const breadcrumb = computed(() => {
  if (!entity.value) return ''
  return entity.value.display_name || entity.value.name
})

// ───────────────────────── Loaders ───────────────────────

async function fetchEntity() {
  loading.value = true
  try {
    entity.value = await api.get<CatalogEntity>(`/api/catalog/entities/${entityId.value}`)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
    entity.value = null
  } finally {
    loading.value = false
  }
}

async function fetchRelations() {
  try {
    relations.value = await api.get<CatalogRelation[]>(
      `/api/catalog/entities/${entityId.value}/relations`,
    )
    if (relations.value.length > 0) {
      const all = await api.get<CatalogEntity[]>('/api/catalog/entities')
      const m = new Map<string, CatalogEntity>()
      for (const e of all) m.set(e.id, e)
      relatedIndex.value = m
    }
  } catch {
    // silent — empty relations are a valid state
  }
}

async function fetchSlos() {
  slosLoading.value = true
  try {
    // Backend list endpoint does not filter by component_id yet; filter in-browser.
    const all = await api.get<Slo[]>('/api/slos?include_disabled=true')
    slos.value = all.filter(s => s.component_id === entityId.value)

    const pairs = await Promise.all(
      slos.value.map(async (s) => {
        try {
          const snap = await api.get<BudgetSnapshot>(`/api/slos/${s.id}/budget`)
          return [s.id, snap] as const
        } catch {
          return [s.id, null] as const
        }
      }),
    )
    budgets.value = Object.fromEntries(pairs)
  } catch {
    slos.value = []
  } finally {
    slosLoading.value = false
  }
}

async function fetchIncidents() {
  incidentsLoading.value = true
  try {
    incidents.value = await api.get<Incident[]>(
      `/api/incidents?component_id=${encodeURIComponent(entityId.value)}`,
    )
  } catch {
    incidents.value = []
  } finally {
    incidentsLoading.value = false
  }
}

async function fetchChanges() {
  changesLoading.value = true
  changesUnavailable.value = false
  try {
    changes.value = await api.get<ChangeEvent[]>(
      `/api/change-events?service_id=${encodeURIComponent(entityId.value)}&limit=50`,
    )
  } catch {
    // Endpoint is not shipped yet (another agent is building it).
    // Treat as "no data" without surfacing a scary error.
    changes.value = []
    changesUnavailable.value = true
  } finally {
    changesLoading.value = false
  }
}

async function handleDelete() {
  try {
    await api.del(`/api/catalog/entities/${entityId.value}`)
    toast.success(t('common.success'))
    showDelete.value = false
    router.push('/services')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function loadAll() {
  await fetchEntity()
  if (!entity.value) return
  await Promise.all([fetchRelations(), fetchSlos(), fetchIncidents(), fetchChanges()])
}

onMounted(loadAll)

watch(entityId, async (newId, oldId) => {
  if (newId && newId !== oldId) {
    relatedIndex.value = new Map()
    activeTab.value = 'overview'
    await loadAll()
  }
})

const TABS: { id: TabId; key: string; icon: typeof Activity }[] = [
  { id: 'overview', key: 'services.tabs.overview', icon: Layers },
  { id: 'slos', key: 'services.tabs.slos', icon: Target },
  { id: 'incidents', key: 'services.tabs.incidents', icon: AlertOctagon },
  { id: 'changes', key: 'services.tabs.changes', icon: Activity },
  { id: 'runbooks', key: 'services.tabs.runbooks', icon: FileText },
  { id: 'docs', key: 'services.tabs.docs', icon: BookOpen },
]

const tabCounts = computed<Partial<Record<TabId, number | null>>>(() => ({
  slos: slos.value.length,
  incidents: activeIncidentCount.value || incidents.value.length,
  changes: changesUnavailable.value ? null : changes.value.length,
}))
</script>

<template>
  <div class="space-y-4">
    <!-- Top nav row -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2 text-[11px] text-muted-foreground/80">
        <Button variant="ghost" size="sm" class="h-7" @click="router.push('/services')">
          <ArrowLeft class="h-3.5 w-3.5" />
          {{ t('services.title') }}
        </Button>
        <ChevronRight class="h-3 w-3 opacity-50" />
        <span class="truncate max-w-[280px] font-medium text-foreground">{{ breadcrumb }}</span>
      </div>

      <div v-if="entity" class="flex items-center gap-1.5">
        <NuxtLink :to="`/catalog/${entity.id}`">
          <Button variant="outline" size="sm">
            <Pencil class="h-3.5 w-3.5" />
            {{ t('catalog.editEntity') }}
          </Button>
        </NuxtLink>
        <Button
          variant="outline"
          size="sm"
          class="text-destructive hover:text-destructive"
          @click="showDelete = true"
        >
          <Trash2 class="h-3.5 w-3.5" />
          {{ t('common.delete') }}
        </Button>
      </div>
    </div>

    <!-- Loading / not found -->
    <div v-if="loading" class="flex items-center justify-center py-16">
      <Loader2 class="h-6 w-6 animate-spin text-primary" />
    </div>

    <div v-else-if="!entity" class="rounded-lg border border-dashed border-border/50 py-12 text-center">
      <Layers class="h-10 w-10 mx-auto text-muted-foreground/40" />
      <p class="mt-2 text-sm text-muted-foreground">{{ t('catalog.notFound') }}</p>
    </div>

    <template v-else>
      <!-- Header card -->
      <header class="rounded-lg bg-panel/60 p-5 space-y-3">
        <div class="flex items-start justify-between gap-4">
          <div class="min-w-0 space-y-1.5">
            <h1 class="text-xl font-semibold text-foreground truncate">
              {{ entity.display_name || entity.name }}
            </h1>
            <div v-if="entity.display_name" class="text-xs text-muted-foreground/70 font-mono truncate">
              {{ entity.name }}
            </div>
            <p v-if="entity.description" class="text-xs text-muted-foreground leading-relaxed max-w-2xl">
              {{ entity.description }}
            </p>
          </div>
        </div>

        <div class="flex flex-wrap items-center gap-1.5">
          <Badge :variant="lifecycleVariant(entity.lifecycle)" class="text-[10px]">
            {{ entity.lifecycle }}
          </Badge>
          <Badge v-if="entity.system_id" variant="secondary" class="text-[10px] gap-1">
            <Layers class="h-3 w-3" />
            {{ relatedIndex.get(entity.system_id)?.name ?? t('services.systemLabel') }}
          </Badge>
          <Badge v-if="entity.owner_group_id" variant="secondary" class="text-[10px] gap-1">
            <Users2 class="h-3 w-3" />
            {{ relatedIndex.get(entity.owner_group_id)?.name ?? t('services.ownerLabel') }}
          </Badge>
          <Badge
            v-for="tag in entity.tags"
            :key="tag"
            variant="secondary"
            class="text-[9px]"
          >
            {{ tag }}
          </Badge>
        </div>
      </header>

      <!-- Tab bar -->
      <div class="flex items-center gap-1 border-b border-border/40 overflow-x-auto">
        <button
          v-for="tab in TABS"
          :key="tab.id"
          class="relative px-3 h-9 text-xs font-medium transition-colors inline-flex items-center gap-1.5 whitespace-nowrap"
          :class="activeTab === tab.id
            ? 'text-primary'
            : 'text-muted-foreground hover:text-foreground'"
          @click="activeTab = tab.id"
        >
          <component :is="tab.icon" class="h-3.5 w-3.5" />
          {{ t(tab.key) }}
          <Badge
            v-if="tabCounts[tab.id] != null && tabCounts[tab.id]! > 0"
            variant="secondary"
            class="text-[9px] h-4 min-w-4 px-1"
          >
            {{ tabCounts[tab.id] }}
          </Badge>
          <span
            v-if="activeTab === tab.id"
            class="absolute bottom-0 left-0 right-0 h-0.5 bg-primary rounded-t"
          />
        </button>
      </div>

      <!-- ─────── OVERVIEW ─────── -->
      <section v-if="activeTab === 'overview'" class="grid grid-cols-1 lg:grid-cols-[1fr_300px] gap-4">
        <!-- Left: runtime + dependencies -->
        <div class="space-y-3">
          <!-- Runtime -->
          <div class="rounded-lg bg-panel/60 p-4 space-y-2">
            <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              <Server class="h-3 w-3" />
              {{ t('services.runtime') }}
            </div>
            <div v-if="runtime" class="grid grid-cols-1 sm:grid-cols-2 gap-x-4 gap-y-1.5 text-xs">
              <div class="flex items-center justify-between gap-2">
                <span class="text-muted-foreground">{{ t('services.runtimeKind') }}</span>
                <span class="font-mono text-[11px]">{{ runtime.kind ?? '—' }}</span>
              </div>
              <div v-if="runtime.cluster_id" class="flex items-center justify-between gap-2">
                <span class="text-muted-foreground">{{ t('services.cluster') }}</span>
                <NuxtLink
                  :to="`/clusters/${runtime.cluster_id}`"
                  class="font-mono text-[11px] text-primary hover:underline truncate max-w-[200px]"
                >
                  {{ runtime.cluster_id.slice(0, 12) }}
                </NuxtLink>
              </div>
              <div v-if="runtime.namespace" class="flex items-center justify-between gap-2">
                <span class="text-muted-foreground">{{ t('services.namespace') }}</span>
                <span class="font-mono text-[11px]">{{ runtime.namespace }}</span>
              </div>
              <div v-if="runtime.workload_name" class="flex items-center justify-between gap-2">
                <span class="text-muted-foreground">{{ t('services.workload') }}</span>
                <span class="font-mono text-[11px] truncate max-w-[200px]">{{ runtime.workload_name }}</span>
              </div>
              <div v-if="runtime.aws_arn" class="col-span-full flex items-center justify-between gap-2">
                <span class="text-muted-foreground">ARN</span>
                <span class="font-mono text-[10px] truncate max-w-[320px]">{{ runtime.aws_arn }}</span>
              </div>
            </div>
            <div v-else class="text-[11px] text-muted-foreground/70">
              {{ t('services.noRuntime') }}
            </div>
          </div>

          <!-- Dependencies -->
          <div class="rounded-lg bg-panel/60 p-4 space-y-3">
            <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              <Link2 class="h-3 w-3" />
              {{ t('services.dependencies') }}
            </div>

            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
              <!-- Outgoing -->
              <div>
                <div class="text-[10px] uppercase tracking-wider text-muted-foreground/70 mb-1.5">
                  {{ t('services.depsOut') }}
                </div>
                <ul v-if="outgoingDeps.length || consumesApis.length || dependsOnResources.length" class="space-y-1">
                  <li
                    v-for="entry in outgoingDeps"
                    :key="entry.relation.id"
                    class="flex items-center justify-between gap-2 text-xs"
                  >
                    <NuxtLink
                      v-if="entry.other"
                      :to="entry.other.kind === 'component' ? `/services/${entry.other.id}` : `/catalog/${entry.other.id}`"
                      class="flex items-center gap-1.5 min-w-0 hover:text-primary"
                    >
                      <Badge :variant="kindVariant(entry.other.kind)" class="uppercase text-[9px] shrink-0">
                        {{ entry.other.kind }}
                      </Badge>
                      <span class="truncate">{{ entry.other.display_name || entry.other.name }}</span>
                    </NuxtLink>
                    <span v-else class="text-muted-foreground/60 font-mono text-[10px]">
                      {{ entry.relation.to_id.slice(0, 8) }}
                    </span>
                  </li>
                  <li
                    v-for="apiName in consumesApis"
                    :key="`api-${apiName}`"
                    class="flex items-center gap-1.5 text-xs"
                  >
                    <Badge variant="warning" class="uppercase text-[9px] shrink-0">api</Badge>
                    <span class="truncate text-muted-foreground">{{ apiName }}</span>
                  </li>
                  <li
                    v-for="res in dependsOnResources"
                    :key="`res-${res}`"
                    class="flex items-center gap-1.5 text-xs"
                  >
                    <Badge variant="success" class="uppercase text-[9px] shrink-0">resource</Badge>
                    <span class="truncate text-muted-foreground">{{ res }}</span>
                  </li>
                </ul>
                <div v-else class="text-[11px] text-muted-foreground/60">—</div>
              </div>

              <!-- Incoming -->
              <div>
                <div class="text-[10px] uppercase tracking-wider text-muted-foreground/70 mb-1.5">
                  {{ t('services.depsIn') }}
                </div>
                <ul v-if="incomingConsumers.length || providesApis.length" class="space-y-1">
                  <li
                    v-for="entry in incomingConsumers"
                    :key="entry.relation.id"
                    class="flex items-center justify-between gap-2 text-xs"
                  >
                    <NuxtLink
                      v-if="entry.other"
                      :to="entry.other.kind === 'component' ? `/services/${entry.other.id}` : `/catalog/${entry.other.id}`"
                      class="flex items-center gap-1.5 min-w-0 hover:text-primary"
                    >
                      <Badge :variant="kindVariant(entry.other.kind)" class="uppercase text-[9px] shrink-0">
                        {{ entry.other.kind }}
                      </Badge>
                      <span class="truncate">{{ entry.other.display_name || entry.other.name }}</span>
                    </NuxtLink>
                    <span v-else class="text-muted-foreground/60 font-mono text-[10px]">
                      {{ entry.relation.from_id.slice(0, 8) }}
                    </span>
                  </li>
                  <li
                    v-for="apiName in providesApis"
                    :key="`prov-${apiName}`"
                    class="flex items-center gap-1.5 text-xs"
                  >
                    <Badge variant="warning" class="uppercase text-[9px] shrink-0">api</Badge>
                    <span class="truncate text-muted-foreground">{{ apiName }}</span>
                  </li>
                </ul>
                <div v-else class="text-[11px] text-muted-foreground/60">—</div>
              </div>
            </div>
          </div>
        </div>

        <!-- Right: key-metric cards -->
        <aside class="space-y-3">
          <div class="rounded-lg bg-panel/60 p-4 space-y-1">
            <div class="text-[10px] uppercase tracking-wider text-muted-foreground/70">
              {{ t('services.metric.activeSlos') }}
            </div>
            <div class="flex items-baseline gap-1.5">
              <span class="text-2xl font-semibold tabular-nums">{{ activeSloCount }}</span>
              <span class="text-[11px] text-muted-foreground/70">/ {{ slos.length }}</span>
            </div>
          </div>

          <div class="rounded-lg bg-panel/60 p-4 space-y-1">
            <div class="text-[10px] uppercase tracking-wider text-muted-foreground/70">
              {{ t('services.metric.activeIncidents') }}
            </div>
            <div class="flex items-baseline gap-1.5">
              <span
                class="text-2xl font-semibold tabular-nums"
                :class="activeIncidentCount > 0 ? 'text-destructive' : 'text-foreground'"
              >
                {{ activeIncidentCount }}
              </span>
              <span class="text-[11px] text-muted-foreground/70">/ {{ incidents.length }}</span>
            </div>
          </div>

          <div class="rounded-lg bg-panel/60 p-4 space-y-1">
            <div class="text-[10px] uppercase tracking-wider text-muted-foreground/70">
              {{ t('services.metric.deploys30d') }}
            </div>
            <div class="text-2xl font-semibold tabular-nums">
              {{ changesUnavailable ? '—' : deployCount30d }}
            </div>
          </div>

          <div class="rounded-lg bg-panel/60 p-4 space-y-1.5">
            <div class="text-[10px] uppercase tracking-wider text-muted-foreground/70">
              {{ t('services.metric.burnHealth') }}
            </div>
            <div v-if="worstBurnRate !== null">
              <BurnRateBadge :rate="worstBurnRate" :label="t('services.metric.peakBurn')" />
            </div>
            <div v-else class="text-[11px] text-muted-foreground/70">
              {{ t('services.metric.noBurnData') }}
            </div>
          </div>
        </aside>
      </section>

      <!-- ─────── SLOs ─────── -->
      <section v-else-if="activeTab === 'slos'" class="space-y-3">
        <div v-if="slosLoading" class="grid gap-3 sm:grid-cols-2">
          <div v-for="n in 2" :key="n" class="h-28 rounded-md bg-panel/50 animate-pulse" />
        </div>

        <div
          v-else-if="slos.length === 0"
          class="rounded-lg border border-dashed border-border/50 py-10 text-center text-xs text-muted-foreground/70"
        >
          {{ t('services.noSlos') }}
        </div>

        <div v-else class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          <article
            v-for="s in slos"
            :key="s.id"
            class="group rounded-md bg-panel/60 hover:bg-panel/80 p-3.5 transition-colors cursor-pointer space-y-2"
            @click="router.push(`/slos/${s.id}`)"
          >
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0">
                <div class="text-sm font-medium truncate">{{ s.name }}</div>
                <div class="text-[10px] text-muted-foreground/70 mt-0.5 uppercase">
                  {{ s.sli_type }} · {{ s.objective_pct }}% / {{ s.window_days }}d
                </div>
              </div>
              <Badge v-if="!s.enabled" variant="secondary" class="text-[9px]">
                {{ t('slo.disabled') }}
              </Badge>
            </div>

            <BudgetBar
              v-if="budgets[s.id]"
              :remaining-pct="budgets[s.id]!.budget_remaining_pct"
              :consumed-minutes="budgets[s.id]!.budget_consumed_minutes"
              :total-minutes="budgets[s.id]!.budget_total_minutes"
            />
            <div v-else class="text-[10px] text-muted-foreground/60">
              {{ t('services.noSnapshot') }}
            </div>

            <div class="flex items-center gap-1.5 flex-wrap">
              <BurnRateBadge
                v-if="budgets[s.id]"
                :rate="budgets[s.id]!.burn_rate_1h"
                label="1h"
              />
              <BurnRateBadge
                v-if="budgets[s.id]"
                :rate="budgets[s.id]!.burn_rate_6h"
                label="6h"
              />
            </div>
          </article>
        </div>
      </section>

      <!-- ─────── INCIDENTS ─────── -->
      <section v-else-if="activeTab === 'incidents'" class="space-y-3">
        <div v-if="incidentsLoading" class="grid gap-3 sm:grid-cols-2">
          <div v-for="n in 2" :key="n" class="h-24 rounded-md bg-panel/50 animate-pulse" />
        </div>

        <div
          v-else-if="incidents.length === 0"
          class="rounded-lg border border-dashed border-border/50 py-10 text-center text-xs text-muted-foreground/70"
        >
          {{ t('services.noIncidents') }}
        </div>

        <template v-else>
          <div v-if="activeIncidentCount > 0" class="space-y-2">
            <div class="text-[10px] uppercase tracking-wider text-muted-foreground/70">
              {{ t('services.incidentsActive') }}
            </div>
            <article
              v-for="inc in incidents.filter(i => !i.resolved_at && !i.closed_at)"
              :key="inc.id"
              class="rounded-md bg-panel/60 hover:bg-panel/80 p-3 transition-colors cursor-pointer flex items-center gap-3"
              @click="router.push(`/incidents/${inc.id}`)"
            >
              <Badge :variant="SEVERITY_STYLE[inc.severity].variant" class="text-[10px] font-semibold shrink-0">
                {{ SEVERITY_STYLE[inc.severity].label }}
              </Badge>
              <div class="flex items-center gap-1 text-[11px] text-muted-foreground shrink-0 tabular-nums">
                <Hash class="h-3 w-3" />INC-{{ String(inc.number).padStart(4, '0') }}
              </div>
              <div class="flex-1 min-w-0 text-xs truncate">{{ inc.title }}</div>
              <div class="text-[11px] text-muted-foreground shrink-0 hidden sm:block">
                {{ formatDate(inc.detected_at) }}
              </div>
            </article>
          </div>

          <div class="space-y-2">
            <div class="text-[10px] uppercase tracking-wider text-muted-foreground/70">
              {{ t('services.incidentsRecent') }}
            </div>
            <div
              v-if="incidents.filter(i => i.resolved_at || i.closed_at).length === 0"
              class="text-[11px] text-muted-foreground/60 px-1"
            >
              {{ t('services.incidentsNoResolved') }}
            </div>
            <article
              v-for="inc in incidents.filter(i => i.resolved_at || i.closed_at).slice(0, 20)"
              :key="inc.id"
              class="rounded-md bg-panel/40 hover:bg-panel/60 p-3 transition-colors cursor-pointer flex items-center gap-3"
              @click="router.push(`/incidents/${inc.id}`)"
            >
              <Badge :variant="SEVERITY_STYLE[inc.severity].variant" class="text-[10px] shrink-0">
                {{ SEVERITY_STYLE[inc.severity].label }}
              </Badge>
              <div class="flex items-center gap-1 text-[11px] text-muted-foreground shrink-0 tabular-nums">
                <Hash class="h-3 w-3" />INC-{{ String(inc.number).padStart(4, '0') }}
              </div>
              <div class="flex-1 min-w-0 text-xs truncate text-muted-foreground">{{ inc.title }}</div>
              <div class="text-[11px] text-muted-foreground/80 shrink-0 hidden sm:block">
                {{ formatDate(inc.resolved_at || inc.detected_at) }}
              </div>
            </article>
          </div>
        </template>
      </section>

      <!-- ─────── CHANGES ─────── -->
      <section v-else-if="activeTab === 'changes'" class="space-y-3">
        <div v-if="changesLoading" class="space-y-2">
          <div v-for="n in 3" :key="n" class="h-14 rounded-md bg-panel/50 animate-pulse" />
        </div>

        <div
          v-else-if="changesUnavailable"
          class="rounded-lg border border-dashed border-border/50 py-10 text-center space-y-2"
        >
          <Activity class="h-8 w-8 mx-auto text-muted-foreground/40" />
          <p class="text-xs text-muted-foreground">{{ t('services.changesUnavailable') }}</p>
          <p class="text-[11px] text-muted-foreground/60">{{ t('services.changesHint') }}</p>
        </div>

        <div
          v-else-if="changes.length === 0"
          class="rounded-lg border border-dashed border-border/50 py-10 text-center text-xs text-muted-foreground/70"
        >
          {{ t('services.noChanges') }}
        </div>

        <ol v-else class="relative border-l border-border/40 ml-2 space-y-3">
          <li
            v-for="c in changes"
            :key="c.id"
            class="pl-4 relative"
          >
            <span
              class="absolute -left-[5px] top-2 h-2 w-2 rounded-full"
              :class="{
                'bg-success': c.kind === 'deploy',
                'bg-warning': c.kind === 'rollback' || c.kind === 'config',
                'bg-destructive': c.kind === 'slo_burn',
                'bg-muted-foreground': !['deploy','rollback','slo_burn','config'].includes(c.kind),
              }"
            />
            <div class="rounded-md bg-panel/60 p-3 space-y-1.5">
              <div class="flex items-center gap-2 flex-wrap">
                <Badge :variant="changeKindMeta(c.kind).variant" class="text-[10px] gap-1">
                  <component :is="changeKindMeta(c.kind).icon" class="h-3 w-3" />
                  {{ changeKindMeta(c.kind).label }}
                </Badge>
                <span class="text-xs font-medium truncate flex-1 min-w-0">{{ c.title }}</span>
                <span class="text-[11px] text-muted-foreground tabular-nums flex items-center gap-1 shrink-0">
                  <Clock3 class="h-3 w-3" />
                  {{ formatDate(c.occurred_at) }}
                </span>
              </div>
              <p v-if="c.description" class="text-[11px] text-muted-foreground leading-relaxed">
                {{ c.description }}
              </p>
              <p v-if="c.actor" class="text-[10px] text-muted-foreground/70">
                {{ t('services.changes.by', { actor: c.actor }) }}
              </p>
            </div>
          </li>
        </ol>
      </section>

      <!-- ─────── RUNBOOKS ─────── -->
      <section v-else-if="activeTab === 'runbooks'" class="space-y-3">
        <div v-if="runbookUrl" class="rounded-lg bg-panel/60 p-4 space-y-2">
          <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
            <FileText class="h-3 w-3" />
            {{ t('services.tabs.runbooks') }}
          </div>
          <a
            :href="runbookUrl"
            target="_blank"
            rel="noopener noreferrer"
            class="inline-flex items-center gap-1.5 text-sm text-primary hover:underline truncate"
          >
            {{ runbookUrl }}
            <ExternalLink class="h-3 w-3 shrink-0" />
          </a>
          <p class="text-[11px] text-muted-foreground/70">
            {{ t('services.runbooksFutureHint') }}
          </p>
        </div>

        <div
          v-else
          class="rounded-lg border border-dashed border-border/50 py-10 text-center space-y-1.5"
        >
          <FileText class="h-8 w-8 mx-auto text-muted-foreground/40" />
          <p class="text-xs text-muted-foreground">{{ t('services.noRunbook') }}</p>
          <p class="text-[11px] text-muted-foreground/60 px-4">{{ t('services.runbookHint') }}</p>
        </div>
      </section>

      <!-- ─────── DOCS ─────── -->
      <section v-else-if="activeTab === 'docs'" class="space-y-3">
        <div v-if="docsUrl" class="rounded-lg bg-panel/60 p-4 space-y-2">
          <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
            <BookOpen class="h-3 w-3" />
            {{ t('services.tabs.docs') }}
          </div>
          <a
            :href="docsUrl"
            target="_blank"
            rel="noopener noreferrer"
            class="inline-flex items-center gap-1.5 text-sm text-primary hover:underline truncate"
          >
            {{ docsUrl }}
            <ExternalLink class="h-3 w-3 shrink-0" />
          </a>
          <p class="text-[11px] text-muted-foreground/70">
            {{ t('services.docsFutureHint') }}
          </p>
        </div>

        <div
          v-else
          class="rounded-lg border border-dashed border-border/50 py-10 text-center space-y-1.5"
        >
          <BookOpen class="h-8 w-8 mx-auto text-muted-foreground/40" />
          <p class="text-xs text-muted-foreground">{{ t('services.noDocs') }}</p>
          <p class="text-[11px] text-muted-foreground/60 px-4">{{ t('services.docsHint') }}</p>
        </div>
      </section>
    </template>

    <ConfirmDialog
      :open="showDelete"
      :title="t('common.delete')"
      :description="t('catalog.deleteConfirm')"
      :confirm-text="t('common.delete')"
      variant="destructive"
      @confirm="handleDelete"
      @cancel="showDelete = false"
    />
  </div>
</template>
