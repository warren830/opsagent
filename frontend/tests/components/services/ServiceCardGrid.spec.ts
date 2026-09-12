/**
 * ServiceCardGrid — the grouping + health sort pipeline. We exercise the
 * pure helpers the grid delegates to (applyFilters + sortComponents +
 * groupBySystem) end-to-end with the same call pattern the SFC uses.
 */
import { describe, it, expect } from 'vitest'
import {
  applyFilters,
  sortComponents,
  groupBySystem,
} from '~/components/services/cardRegistry'
import type {
  ComponentOverview,
  HealthStatus,
  RuntimeKind,
  SystemSummary,
} from '~/composables/useServicesOverview'

function comp(
  id: string,
  health: HealthStatus,
  systemId: string | null,
  kind: RuntimeKind = 'generic',
): ComponentOverview {
  return {
    id,
    name: id,
    display_name: null,
    description: null,
    lifecycle: 'production',
    system_id: systemId,
    owner_group_id: null,
    tags: [],
    runtime: { kind },
    runtime_detail: {},
    health,
    health_reason: '',
    active_incident_count: 0,
    slo_summary: { total: 0, budget_remaining_min_pct: null, burn_rate_1h_max: null },
  }
}

describe('ServiceCardGrid pipeline', () => {
  const systems: SystemSummary[] = [
    { id: 'sys-p', name: 'payments', display_name: 'Payments', component_count: 4, health_summary: { healthy: 0, warning: 0, critical: 0 } },
    { id: 'sys-o', name: 'orders', display_name: 'Orders', component_count: 2, health_summary: { healthy: 0, warning: 0, critical: 0 } },
  ]

  const components: ComponentOverview[] = [
    comp('pay-healthy', 'healthy', 'sys-p', 'eks'),
    comp('pay-critical', 'critical', 'sys-p', 'lambda'),
    comp('pay-warning', 'warning', 'sys-p', 'rds'),
    comp('ord-critical', 'critical', 'sys-o', 'ec2'),
    comp('free-unknown', 'unknown', null, 'external'),
  ]

  it('filter-sort-group happy path: health order inside each system', () => {
    const filtered = applyFilters(components, { search: '', systemId: 'all', lifecycle: 'all', runtime: 'all' })
    const sorted = sortComponents(filtered, 'health')
    const groups = groupBySystem(sorted, systems)

    // Known systems first (payments, orders), then ungrouped.
    expect(groups.map(g => g.system.id)).toEqual(['sys-p', 'sys-o', null])

    // Inside payments: critical → warning → healthy.
    expect(groups[0].components.map(c => c.id)).toEqual([
      'pay-critical', 'pay-warning', 'pay-healthy',
    ])
    // Orders has only 1.
    expect(groups[1].components.map(c => c.id)).toEqual(['ord-critical'])
    // Ungrouped.
    expect(groups[2].components.map(c => c.id)).toEqual(['free-unknown'])
  })

  it('runtime filter applies before grouping', () => {
    const filtered = applyFilters(components, { search: '', systemId: 'all', lifecycle: 'all', runtime: 'lambda' })
    const sorted = sortComponents(filtered, 'health')
    const groups = groupBySystem(sorted, systems)
    expect(groups).toHaveLength(1)
    expect(groups[0].components.map(c => c.id)).toEqual(['pay-critical'])
  })

  it('systemId="none" keeps only ungrouped components', () => {
    const filtered = applyFilters(components, { search: '', systemId: 'none', lifecycle: 'all', runtime: 'all' })
    expect(filtered.map(c => c.id)).toEqual(['free-unknown'])
    const groups = groupBySystem(filtered, systems)
    expect(groups).toHaveLength(1)
    expect(groups[0].system.id).toBeNull()
  })

  it('defaults (health="all") render every component, same as omitting health', () => {
    const withAll = applyFilters(components, {
      search: '', systemId: 'all', lifecycle: 'all', runtime: 'all', health: 'all',
    })
    const withoutHealth = applyFilters(components, {
      search: '', systemId: 'all', lifecycle: 'all', runtime: 'all',
    })
    expect(withAll.map(c => c.id)).toEqual(withoutHealth.map(c => c.id))
    expect(withAll).toHaveLength(components.length)
  })

  it('health filter narrows the grid and group badges match the visible cards', () => {
    const filtered = applyFilters(components, {
      search: '', systemId: 'all', lifecycle: 'all', runtime: 'all', health: 'critical',
    })
    const sorted = sortComponents(filtered, 'health')
    const groups = groupBySystem(sorted, systems)

    expect(groups.map(g => g.system.id)).toEqual(['sys-p', 'sys-o'])
    expect(groups[0].components.map(c => c.id)).toEqual(['pay-critical'])
    expect(groups[1].components.map(c => c.id)).toEqual(['ord-critical'])

    for (const g of groups) {
      const s = g.healthSummary
      expect(s.critical).toBe(g.components.length)
      expect(s.warning).toBe(0)
      expect(s.healthy).toBe(0)
      expect(s.unknown ?? 0).toBe(0)
    }
  })

  it.each([
    ['critical', ['pay-critical', 'ord-critical']],
    ['warning', ['pay-warning']],
    ['unknown', ['free-unknown']],
    ['healthy', ['pay-healthy']],
  ] as const)('health="%s" selects the expected cards', (health, expected) => {
    const filtered = applyFilters(components, {
      search: '', systemId: 'all', lifecycle: 'all', runtime: 'all', health,
    })
    expect(sortComponents(filtered, 'name').map(c => c.id).sort()).toEqual([...expected].sort())
  })

  it('health combined with runtime can produce an empty grid', () => {
    const filtered = applyFilters(components, {
      search: '', systemId: 'all', lifecycle: 'all', runtime: 'eks', health: 'critical',
    })
    expect(filtered).toEqual([])
    expect(groupBySystem(filtered, systems)).toEqual([])
  })

  it('health filtering leaves the source overview data untouched', () => {
    const snapshot = JSON.parse(JSON.stringify(components))
    const systemsSnapshot = JSON.parse(JSON.stringify(systems))
    const filtered = applyFilters(components, {
      search: '', systemId: 'all', lifecycle: 'all', runtime: 'all', health: 'warning',
    })
    groupBySystem(sortComponents(filtered, 'health'), systems)
    expect(components).toEqual(snapshot)
    expect(systems).toEqual(systemsSnapshot)
  })
})
