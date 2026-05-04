/**
 * useServicesOverview — fetches /api/services/overview and polls every 30s.
 *
 * The endpoint returns a pre-aggregated view of all Components in the
 * catalog with their runtime detail, active incident count and SLO summary.
 *
 * If the endpoint is unavailable (404/5xx — e.g. backend U2 not yet shipped)
 * we fall back to joining /api/catalog/entities + /api/slos + /api/incidents
 * on the client so the UI always renders something. The fallback produces
 * the same ServicesOverviewResponse shape, so downstream components do not
 * need to care which path was used.
 */
import { ref, readonly } from 'vue'
import { ApiError } from '@/composables/useApi'

// ─────────────────── Types (mirror backend U2 schema) ───────────────────

export type RuntimeKind = 'eks' | 'ec2' | 'rds' | 'lambda' | 'external' | 'generic'

export type HealthStatus = 'healthy' | 'warning' | 'critical' | 'unknown'

export type Lifecycle = 'production' | 'experimental' | 'deprecated' | 'retired' | string

export interface RuntimeDescriptor {
  kind: RuntimeKind
  cluster?: string
  namespace?: string
  workload?: string
  instance_id?: string
  region?: string
  arn?: string
  base_url?: string
  engine?: string
  [key: string]: unknown
}

export interface SloSummary {
  total: number
  budget_remaining_min_pct: number | null
  burn_rate_1h_max: number | null
}

export interface ComponentOverview {
  id: string
  name: string
  display_name: string | null
  description: string | null
  lifecycle: Lifecycle
  system_id: string | null
  owner_group_id: string | null
  tags: string[]
  runtime: RuntimeDescriptor | null
  runtime_detail: Record<string, unknown>
  health: HealthStatus
  health_reason: string
  active_incident_count: number
  slo_summary: SloSummary
}

export interface SystemSummary {
  id: string
  name: string
  display_name: string | null
  component_count: number
  health_summary: { healthy: number, warning: number, critical: number, unknown?: number }
}

export interface ServicesOverviewResponse {
  systems: SystemSummary[]
  components: ComponentOverview[]
}

// ─────────────────── Fallback types (existing endpoints) ────────────────

interface CatalogEntityRow {
  id: string
  kind: string
  name: string
  display_name: string | null
  description: string | null
  lifecycle: string
  owner_group_id: string | null
  system_id: string | null
  tags: string[]
  spec: Record<string, unknown>
  updated_at: string
}

interface SloRow {
  id: string
  component_id: string | null
  enabled: boolean
}

interface BudgetSnapshot {
  slo_id: string
  budget_remaining_pct: number
  burn_rate_1h: number | null
  burn_rate_6h: number | null
}

interface IncidentRow {
  id: string
  affected_component_ids: string[]
  resolved_at: string | null
  closed_at: string | null
}

// ─────────────────── Module state ────────────────

const data = ref<ServicesOverviewResponse | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
const source = ref<'overview' | 'fallback' | null>(null)
let timer: ReturnType<typeof setInterval> | null = null

const POLL_MS = 30_000

export function useServicesOverview() {
  const api = useApi()

  async function fetchOnce() {
    loading.value = true
    error.value = null
    try {
      const resp = await api.get<ServicesOverviewResponse>('/api/services/overview')
      data.value = resp
      source.value = 'overview'
    } catch (err) {
      // If endpoint doesn't exist yet, degrade gracefully via fallback.
      if (err instanceof ApiError && (err.status === 404 || err.status >= 500)) {
        try {
          data.value = await fetchFallback(api)
          source.value = 'fallback'
        } catch (inner) {
          error.value = inner instanceof Error ? inner.message : 'Failed to load services'
        }
      } else {
        error.value = err instanceof Error ? err.message : 'Failed to load services'
      }
    } finally {
      loading.value = false
    }
  }

  function startPolling() {
    if (timer) return
    fetchOnce()
    timer = setInterval(fetchOnce, POLL_MS)
  }

  function stopPolling() {
    if (timer) {
      clearInterval(timer)
      timer = null
    }
  }

  return {
    data: readonly(data),
    loading: readonly(loading),
    error: readonly(error),
    source: readonly(source),
    fetchOnce,
    startPolling,
    stopPolling,
  }
}

// ─────────────────── Helpers ────────────────

async function fetchFallback(api: ReturnType<typeof useApi>): Promise<ServicesOverviewResponse> {
  // Join catalog + SLO + active incident entirely in the browser.
  const [entities, slos, incidents] = await Promise.all([
    api.get<CatalogEntityRow[]>('/api/catalog/entities'),
    api.get<SloRow[]>('/api/slos?include_disabled=true').catch(() => [] as SloRow[]),
    api.get<IncidentRow[]>('/api/incidents/active').catch(() => [] as IncidentRow[]),
  ])

  // Best-effort budget snapshots. If budgets endpoint times out, treat them
  // as unavailable — health drops to "unknown" for that component.
  const budgetMap: Record<string, BudgetSnapshot> = {}
  if (slos.length > 0) {
    try {
      const ids = slos.map(s => s.id).join(',')
      const summaries = await api.get<BudgetSnapshot[]>(`/api/slos/budgets?ids=${ids}`)
      for (const b of summaries ?? []) budgetMap[b.slo_id] = b
    } catch {
      // ignore, some/all budgets missing
    }
  }

  const systemEntities = entities.filter(e => e.kind === 'system')
  const componentEntities = entities.filter(e => e.kind === 'component')

  const systemsById = new Map(systemEntities.map(s => [s.id, s]))

  // Per-component active incidents.
  const activeByComp = new Map<string, number>()
  for (const inc of incidents) {
    if (inc.resolved_at || inc.closed_at) continue
    for (const cid of inc.affected_component_ids) {
      activeByComp.set(cid, (activeByComp.get(cid) || 0) + 1)
    }
  }

  // Per-component SLO summaries.
  const sloByComp = new Map<string, SloRow[]>()
  for (const s of slos) {
    if (!s.component_id || !s.enabled) continue
    const arr = sloByComp.get(s.component_id) || []
    arr.push(s)
    sloByComp.set(s.component_id, arr)
  }

  const components: ComponentOverview[] = componentEntities.map((e) => {
    const spec = e.spec || {}
    const specRuntime = (spec.runtime as RuntimeDescriptor | undefined) ?? null
    const componentSlos = sloByComp.get(e.id) || []

    let minBudgetPct: number | null = null
    let maxBurn1h: number | null = null
    for (const s of componentSlos) {
      const snap = budgetMap[s.id]
      if (!snap) continue
      if (minBudgetPct == null || snap.budget_remaining_pct < minBudgetPct) {
        minBudgetPct = snap.budget_remaining_pct
      }
      if (typeof snap.burn_rate_1h === 'number' && Number.isFinite(snap.burn_rate_1h)) {
        if (maxBurn1h == null || snap.burn_rate_1h > maxBurn1h) {
          maxBurn1h = snap.burn_rate_1h
        }
      }
    }

    const incidentCount = activeByComp.get(e.id) || 0
    let health: HealthStatus = 'unknown'
    let reason = 'no signals available'
    if (incidentCount > 0) {
      health = 'critical'
      reason = `${incidentCount} active incident(s)`
    } else if (maxBurn1h != null && maxBurn1h >= 3) {
      health = 'critical'
      reason = `1h burn rate ${maxBurn1h.toFixed(2)}`
    } else if (maxBurn1h != null && maxBurn1h >= 1) {
      health = 'warning'
      reason = `1h burn rate ${maxBurn1h.toFixed(2)}`
    } else if (minBudgetPct != null) {
      health = 'healthy'
      reason = `SLO budget ${minBudgetPct.toFixed(1)}% remaining`
    }

    return {
      id: e.id,
      name: e.name,
      display_name: e.display_name,
      description: e.description,
      lifecycle: e.lifecycle,
      system_id: e.system_id,
      owner_group_id: e.owner_group_id,
      tags: e.tags || [],
      runtime: specRuntime,
      runtime_detail: (spec.runtime_detail as Record<string, unknown>) || {},
      health,
      health_reason: reason,
      active_incident_count: incidentCount,
      slo_summary: {
        total: componentSlos.length,
        budget_remaining_min_pct: minBudgetPct,
        burn_rate_1h_max: maxBurn1h,
      },
    }
  })

  // Assemble systems summary.
  const systems: SystemSummary[] = Array.from(systemsById.values()).map((s) => {
    const members = components.filter(c => c.system_id === s.id)
    const health_summary = { healthy: 0, warning: 0, critical: 0, unknown: 0 }
    for (const c of members) health_summary[c.health]++
    return {
      id: s.id,
      name: s.name,
      display_name: s.display_name,
      component_count: members.length,
      health_summary,
    }
  })

  return { systems, components }
}
