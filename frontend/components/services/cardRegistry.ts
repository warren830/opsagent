/**
 * Pure (no-Vue-render) helpers for ServiceCard dispatching & grid
 * grouping/sorting.
 *
 * Deliberately keeps zero `.vue` imports so Vitest can load this under
 * happy-dom without installing `@vitejs/plugin-vue`. Actual SFC component
 * resolution lives in ServiceCard.vue, which performs the kind→import
 * switch locally.
 */
import type {
  ComponentOverview,
  HealthStatus,
  RuntimeKind,
  SystemSummary,
} from '@/composables/useServicesOverview'

/**
 * Normalise any value into a runtime kind that has a registered card.
 * Unknown / missing values degrade to 'generic'.
 */
export type CardKind = RuntimeKind

const KNOWN_KINDS: readonly RuntimeKind[] = ['eks', 'ec2', 'rds', 'lambda', 'external', 'generic']

export function resolveCardKind(kind: unknown): CardKind {
  if (typeof kind === 'string' && (KNOWN_KINDS as readonly string[]).includes(kind)) {
    return kind as CardKind
  }
  return 'generic'
}

/**
 * Health ordering: Critical → Warning → Unknown → Healthy.
 * Used as the primary sort key; components break ties by name alpha.
 */
export const HEALTH_ORDER: Record<HealthStatus, number> = {
  critical: 0,
  warning: 1,
  unknown: 2,
  healthy: 3,
}

export type SortKey = 'health' | 'name' | 'incidents'

export function sortComponents(list: ComponentOverview[], sort: SortKey): ComponentOverview[] {
  const copy = [...list]
  switch (sort) {
    case 'name':
      copy.sort((a, b) => displayName(a).localeCompare(displayName(b)))
      return copy
    case 'incidents':
      copy.sort((a, b) => {
        if (b.active_incident_count !== a.active_incident_count) {
          return b.active_incident_count - a.active_incident_count
        }
        return displayName(a).localeCompare(displayName(b))
      })
      return copy
    case 'health':
    default:
      copy.sort((a, b) => {
        const ho = (HEALTH_ORDER[a.health] ?? 99) - (HEALTH_ORDER[b.health] ?? 99)
        if (ho !== 0) return ho
        return displayName(a).localeCompare(displayName(b))
      })
      return copy
  }
}

function displayName(c: ComponentOverview): string {
  return c.display_name || c.name
}

export interface ServiceFilterState {
  search: string
  systemId: string     // 'all' | uuid | 'none'
  lifecycle: string
  runtime: string
}

export function applyFilters(
  components: ComponentOverview[],
  filters: ServiceFilterState,
): ComponentOverview[] {
  const q = filters.search.trim().toLowerCase()
  return components.filter((c) => {
    if (filters.systemId === 'none' && c.system_id) return false
    if (filters.systemId !== 'all' && filters.systemId !== 'none' && c.system_id !== filters.systemId) return false
    if (filters.lifecycle !== 'all' && c.lifecycle !== filters.lifecycle) return false
    if (filters.runtime !== 'all') {
      const kind = c.runtime?.kind ?? 'generic'
      if (kind !== filters.runtime) return false
    }
    if (!q) return true
    return (
      c.name.toLowerCase().includes(q)
      || (c.display_name || '').toLowerCase().includes(q)
      || (c.description || '').toLowerCase().includes(q)
      || c.tags.some(tag => tag.toLowerCase().includes(q))
    )
  })
}

export interface GroupedSystem {
  system: { id: string | null, name: string, display_name: string | null }
  healthSummary: SystemSummary['health_summary']
  components: ComponentOverview[]
}

/**
 * Group components by their system_id. Components without a system_id end
 * up in an "ungrouped" bucket keyed by null. Within each group components
 * are already expected to be sorted (caller's responsibility).
 */
export function groupBySystem(
  components: ComponentOverview[],
  systems: SystemSummary[],
): GroupedSystem[] {
  const bySystem = new Map<string | null, ComponentOverview[]>()
  for (const c of components) {
    const key = c.system_id ?? null
    const bucket = bySystem.get(key) ?? []
    bucket.push(c)
    bySystem.set(key, bucket)
  }

  const orderedSystems: GroupedSystem[] = []
  // Known systems first, in the order returned by the backend.
  for (const s of systems) {
    const list = bySystem.get(s.id) ?? []
    if (list.length === 0) continue
    orderedSystems.push({
      system: { id: s.id, name: s.name, display_name: s.display_name },
      healthSummary: s.health_summary,
      components: list,
    })
  }
  // Ungrouped bucket at the end — only if it has members.
  const ungrouped = bySystem.get(null) ?? []
  if (ungrouped.length > 0) {
    orderedSystems.push({
      system: { id: null, name: 'ungrouped', display_name: null },
      healthSummary: summariseHealth(ungrouped),
      components: ungrouped,
    })
  }
  return orderedSystems
}

function summariseHealth(list: ComponentOverview[]): SystemSummary['health_summary'] {
  const h = { healthy: 0, warning: 0, critical: 0, unknown: 0 }
  for (const c of list) h[c.health]++
  return h
}
