import { describe, it, expect } from 'vitest'
import {
  resolveCardKind,
  sortComponents,
  applyFilters,
  groupBySystem,
  HEALTH_ORDER,
  HEALTH_FILTER_VALUES,
} from '~/components/services/cardRegistry'
import type {
  ComponentOverview,
  HealthStatus,
  RuntimeKind,
  SystemSummary,
} from '~/composables/useServicesOverview'

function make(
  overrides: Partial<ComponentOverview> & { id: string },
): ComponentOverview {
  return {
    id: overrides.id,
    name: overrides.name ?? overrides.id,
    display_name: overrides.display_name ?? null,
    description: overrides.description ?? null,
    lifecycle: overrides.lifecycle ?? 'production',
    system_id: overrides.system_id ?? null,
    owner_group_id: overrides.owner_group_id ?? null,
    tags: overrides.tags ?? [],
    runtime: overrides.runtime ?? null,
    runtime_detail: overrides.runtime_detail ?? {},
    health: overrides.health ?? 'healthy',
    health_reason: overrides.health_reason ?? '',
    active_incident_count: overrides.active_incident_count ?? 0,
    slo_summary: overrides.slo_summary ?? { total: 0, budget_remaining_min_pct: null, burn_rate_1h_max: null },
  }
}

describe('resolveCardKind (dispatcher)', () => {
  const known: RuntimeKind[] = ['eks', 'ec2', 'rds', 'lambda', 'external', 'generic']

  it.each(known)('passes through known kind %s', (kind) => {
    expect(resolveCardKind(kind)).toBe(kind)
  })

  it('falls back to generic for unknown kinds', () => {
    expect(resolveCardKind('wat')).toBe('generic')
  })

  it('falls back to generic when runtime is null/undefined', () => {
    expect(resolveCardKind(null)).toBe('generic')
    expect(resolveCardKind(undefined)).toBe('generic')
  })
})

describe('HEALTH_ORDER', () => {
  it('sorts critical before warning before unknown before healthy', () => {
    const order: HealthStatus[] = ['healthy', 'unknown', 'warning', 'critical']
    const sorted = [...order].sort((a, b) => HEALTH_ORDER[a] - HEALTH_ORDER[b])
    expect(sorted).toEqual(['critical', 'warning', 'unknown', 'healthy'])
  })
})

describe('sortComponents', () => {
  const sample: ComponentOverview[] = [
    make({ id: 'h1', name: 'zoo', health: 'healthy' }),
    make({ id: 'c1', name: 'alpha', health: 'critical' }),
    make({ id: 'w1', name: 'mid', health: 'warning' }),
    make({ id: 'c2', name: 'aardvark', health: 'critical' }),
    make({ id: 'u1', name: 'beta', health: 'unknown' }),
  ]

  it('orders by health (critical→warning→unknown→healthy), ties broken by name', () => {
    const sorted = sortComponents(sample, 'health')
    expect(sorted.map(c => c.id)).toEqual(['c2', 'c1', 'w1', 'u1', 'h1'])
  })

  it('name sort ignores health', () => {
    const sorted = sortComponents(sample, 'name')
    expect(sorted.map(c => c.name)).toEqual(['aardvark', 'alpha', 'beta', 'mid', 'zoo'])
  })

  it('incidents sort puts highest incident count first', () => {
    const list = [
      make({ id: 'a', name: 'a', active_incident_count: 0 }),
      make({ id: 'b', name: 'b', active_incident_count: 3 }),
      make({ id: 'c', name: 'c', active_incident_count: 1 }),
    ]
    const sorted = sortComponents(list, 'incidents')
    expect(sorted.map(c => c.id)).toEqual(['b', 'c', 'a'])
  })
})

describe('applyFilters', () => {
  const list: ComponentOverview[] = [
    make({ id: '1', name: 'order-api', lifecycle: 'production', system_id: 'sys-p', tags: ['rust'], runtime: { kind: 'eks' } }),
    make({ id: '2', name: 'legacy-billing', lifecycle: 'deprecated', system_id: 'sys-p', runtime: { kind: 'ec2' } }),
    make({ id: '3', name: 'orders-db', lifecycle: 'production', system_id: null, runtime: { kind: 'rds' } }),
    make({ id: '4', name: 'payments-fn', lifecycle: 'experimental', system_id: 'sys-x', runtime: { kind: 'lambda' } }),
  ]

  it('search matches name, tags', () => {
    expect(applyFilters(list, { search: 'order', systemId: 'all', lifecycle: 'all', runtime: 'all' }).map(c => c.id)).toEqual(['1', '3'])
    expect(applyFilters(list, { search: 'RUST', systemId: 'all', lifecycle: 'all', runtime: 'all' }).map(c => c.id)).toEqual(['1'])
  })

  it('system filter with "none" keeps ungrouped only', () => {
    expect(applyFilters(list, { search: '', systemId: 'none', lifecycle: 'all', runtime: 'all' }).map(c => c.id)).toEqual(['3'])
  })

  it('runtime filter matches kind', () => {
    expect(applyFilters(list, { search: '', systemId: 'all', lifecycle: 'all', runtime: 'lambda' }).map(c => c.id)).toEqual(['4'])
  })

  it('lifecycle filter is exact match', () => {
    expect(applyFilters(list, { search: '', systemId: 'all', lifecycle: 'deprecated', runtime: 'all' }).map(c => c.id)).toEqual(['2'])
  })

  it('combining filters is conjunctive', () => {
    const got = applyFilters(list, { search: 'order', systemId: 'sys-p', lifecycle: 'production', runtime: 'eks' })
    expect(got.map(c => c.id)).toEqual(['1'])
  })

  it('omitting health behaves like "all" (backwards compatible)', () => {
    const got = applyFilters(list, { search: '', systemId: 'all', lifecycle: 'all', runtime: 'all' })
    expect(got.map(c => c.id)).toEqual(['1', '2', '3', '4'])
  })
})

describe('applyFilters — health', () => {
  const list: ComponentOverview[] = [
    make({ id: 'c1', name: 'checkout-api', health: 'critical', system_id: 'sys-p', runtime: { kind: 'eks' } }),
    make({ id: 'w1', name: 'warn-worker', health: 'warning', system_id: 'sys-p', runtime: { kind: 'ec2' } }),
    make({ id: 'u1', name: 'unknown-fn', health: 'unknown', system_id: 'sys-x', runtime: { kind: 'lambda' } }),
    make({ id: 'h1', name: 'happy-db', health: 'healthy', system_id: null, runtime: { kind: 'rds' } }),
    make({ id: 'c2', name: 'cart-api', health: 'critical', lifecycle: 'experimental', system_id: 'sys-x', runtime: { kind: 'eks' } }),
  ]

  const base = { search: '', systemId: 'all', lifecycle: 'all', runtime: 'all' }

  it('health="all" keeps every component', () => {
    expect(applyFilters(list, { ...base, health: 'all' }).map(c => c.id))
      .toEqual(['c1', 'w1', 'u1', 'h1', 'c2'])
  })

  it.each([
    ['critical', ['c1', 'c2']],
    ['warning', ['w1']],
    ['unknown', ['u1']],
    ['healthy', ['h1']],
  ] as const)('health="%s" keeps only that status', (health, expected) => {
    expect(applyFilters(list, { ...base, health }).map(c => c.id)).toEqual([...expected])
  })

  it('exposes the selectable filter values including the "all" default', () => {
    expect(HEALTH_FILTER_VALUES).toEqual(['all', 'critical', 'warning', 'unknown', 'healthy'])
  })

  it('intersects with search', () => {
    expect(applyFilters(list, { ...base, search: 'api', health: 'critical' }).map(c => c.id))
      .toEqual(['c1', 'c2'])
  })

  it('intersects with system, lifecycle and runtime', () => {
    expect(applyFilters(list, {
      search: '',
      systemId: 'sys-x',
      lifecycle: 'experimental',
      runtime: 'eks',
      health: 'critical',
    }).map(c => c.id)).toEqual(['c2'])
  })

  it('intersects with systemId="none"', () => {
    expect(applyFilters(list, { ...base, systemId: 'none', health: 'healthy' }).map(c => c.id))
      .toEqual(['h1'])
    expect(applyFilters(list, { ...base, systemId: 'none', health: 'critical' })).toEqual([])
  })

  it('returns an empty array when no component matches the health filter', () => {
    const onlyHealthy = [make({ id: 'only', health: 'healthy' })]
    expect(applyFilters(onlyHealthy, { ...base, health: 'critical' })).toEqual([])
  })

  it('returns an empty array when health contradicts another filter', () => {
    expect(applyFilters(list, { ...base, runtime: 'rds', health: 'critical' })).toEqual([])
  })

  it('does not mutate the input list or its members', () => {
    const snapshot = JSON.parse(JSON.stringify(list))
    const before = [...list]
    applyFilters(list, { ...base, health: 'critical' })
    expect(list).toEqual(snapshot)
    expect(list).toHaveLength(before.length)
    expect(list.map(c => c.id)).toEqual(before.map(c => c.id))
  })
})

describe('groupBySystem', () => {
  const systems: SystemSummary[] = [
    { id: 'sys-p', name: 'payments', display_name: 'Payments', component_count: 2, health_summary: { healthy: 1, warning: 0, critical: 1 } },
    { id: 'sys-x', name: 'misc', display_name: null, component_count: 1, health_summary: { healthy: 0, warning: 0, critical: 0 } },
  ]

  it('groups components into declared systems and appends ungrouped', () => {
    const components: ComponentOverview[] = [
      make({ id: '1', system_id: 'sys-p', health: 'critical' }),
      make({ id: '2', system_id: 'sys-p', health: 'healthy' }),
      make({ id: '3', system_id: null, health: 'warning' }),
      make({ id: '4', system_id: 'sys-x', health: 'healthy' }),
    ]
    const groups = groupBySystem(components, systems)
    expect(groups).toHaveLength(3)
    expect(groups[0].system.id).toBe('sys-p')
    expect(groups[0].components.map(c => c.id)).toEqual(['1', '2'])
    expect(groups[1].system.id).toBe('sys-x')
    expect(groups[2].system.id).toBeNull()
    expect(groups[2].components.map(c => c.id)).toEqual(['3'])
  })

  it('skips empty known systems', () => {
    const components: ComponentOverview[] = [
      make({ id: 'a', system_id: 'sys-p' }),
    ]
    const groups = groupBySystem(components, systems)
    expect(groups).toHaveLength(1)
    expect(groups[0].system.id).toBe('sys-p')
  })

  it('computes health summary for ungrouped bucket', () => {
    const components: ComponentOverview[] = [
      make({ id: 'a', system_id: null, health: 'critical' }),
      make({ id: 'b', system_id: null, health: 'critical' }),
      make({ id: 'c', system_id: null, health: 'healthy' }),
    ]
    const groups = groupBySystem(components, [])
    expect(groups).toHaveLength(1)
    expect(groups[0].healthSummary).toEqual({ healthy: 1, warning: 0, critical: 2, unknown: 0 })
  })

  it('derives known-system health summary from the given components, not the backend totals', () => {
    // Backend claims sys-p has 1 healthy + 1 critical; we only pass the critical one.
    const components: ComponentOverview[] = [
      make({ id: 'crit', system_id: 'sys-p', health: 'critical' }),
    ]
    const groups = groupBySystem(components, systems)
    expect(groups).toHaveLength(1)
    expect(groups[0].system.id).toBe('sys-p')
    expect(groups[0].healthSummary).toEqual({ healthy: 0, warning: 0, critical: 1, unknown: 0 })
    // Explicitly diverges from the unfiltered SystemSummary.
    expect(groups[0].healthSummary).not.toEqual(systems[0].health_summary)
  })

  it('keeps badge totals in sync with component count after a health filter', () => {
    const components: ComponentOverview[] = [
      make({ id: 'p-crit', system_id: 'sys-p', health: 'critical' }),
      make({ id: 'p-warn', system_id: 'sys-p', health: 'warning' }),
      make({ id: 'p-ok', system_id: 'sys-p', health: 'healthy' }),
      make({ id: 'x-unknown', system_id: 'sys-x', health: 'unknown' }),
    ]
    const filtered = applyFilters(components, {
      search: '', systemId: 'all', lifecycle: 'all', runtime: 'all', health: 'critical',
    })
    const groups = groupBySystem(filtered, systems)

    expect(groups).toHaveLength(1)
    expect(groups[0].system.id).toBe('sys-p')
    expect(groups[0].components).toHaveLength(1)
    const s = groups[0].healthSummary
    const badgeTotal = s.critical + s.warning + s.healthy + (s.unknown ?? 0)
    expect(badgeTotal).toBe(groups[0].components.length)
  })

  it('preserves system name / display_name / ordering metadata', () => {
    const components: ComponentOverview[] = [
      make({ id: 'x1', system_id: 'sys-x', health: 'unknown' }),
      make({ id: 'p1', system_id: 'sys-p', health: 'critical' }),
      make({ id: 'free', system_id: null, health: 'healthy' }),
    ]
    const groups = groupBySystem(components, systems)
    // Backend order is preserved (sys-p declared first), ungrouped last.
    expect(groups.map(g => g.system.id)).toEqual(['sys-p', 'sys-x', null])
    expect(groups[0].system).toEqual({ id: 'sys-p', name: 'payments', display_name: 'Payments' })
    expect(groups[1].system).toEqual({ id: 'sys-x', name: 'misc', display_name: null })
  })

  it('does not mutate the inputs', () => {
    const components: ComponentOverview[] = [
      make({ id: 'p1', system_id: 'sys-p', health: 'critical' }),
      make({ id: 'free', system_id: null, health: 'warning' }),
    ]
    const componentsSnapshot = JSON.parse(JSON.stringify(components))
    const systemsSnapshot = JSON.parse(JSON.stringify(systems))

    groupBySystem(components, systems)

    expect(components).toEqual(componentsSnapshot)
    expect(systems).toEqual(systemsSnapshot)
    // The backend summary object itself is untouched.
    expect(systems[0].health_summary).toEqual({ healthy: 1, warning: 0, critical: 1 })
  })
})
