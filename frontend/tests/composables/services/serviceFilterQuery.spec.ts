/**
 * serviceFilterQuery — the filter ⇄ URL codec that backs Services deep links.
 * Covers validation of every enum, the documented defaults for missing/null/
 * array/unsupported values, default omission, and unrelated-param preservation.
 */
import { describe, it, expect } from 'vitest'
import {
  DEFAULT_SERVICE_FILTERS,
  LIFECYCLE_FILTER_VALUES,
  RUNTIME_FILTER_VALUES,
  SERVICE_FILTER_QUERY_KEYS,
  SORT_FILTER_VALUES,
  isCanonicalServiceFilterQuery,
  isSearchOnlyChange,
  mergeServiceFiltersIntoQuery,
  normalizeServiceFilters,
  parseServiceFilters,
  serviceFilterQuerySignature,
  serviceFiltersEqual,
  serviceFiltersToQuery,
  type ServiceFilters,
} from '~/composables/serviceFilterQuery'
import { HEALTH_FILTER_VALUES } from '~/components/services/cardRegistry'

const SYSTEMS = ['sys-p', 'sys-o']
const LOADED = { knownSystemIds: SYSTEMS, systemsReady: true }

function filters(patch: Partial<ServiceFilters> = {}): ServiceFilters {
  return { ...DEFAULT_SERVICE_FILTERS, ...patch }
}

describe('parseServiceFilters', () => {
  it('returns documented defaults for an empty query', () => {
    expect(parseServiceFilters({}, LOADED)).toEqual({
      search: '',
      systemId: 'all',
      lifecycle: 'all',
      runtime: 'all',
      health: 'all',
      sort: 'health',
    })
  })

  it.each([undefined, null, {}])('treats %s as all-defaults', (query) => {
    expect(parseServiceFilters(query as never, LOADED)).toEqual({ ...DEFAULT_SERVICE_FILTERS })
  })

  it('restores every supported field from the URL', () => {
    const parsed = parseServiceFilters({
      q: 'order api',
      system: 'sys-o',
      lifecycle: 'experimental',
      runtime: 'lambda',
      health: 'critical',
      sort: 'incidents',
    }, LOADED)
    expect(parsed).toEqual({
      search: 'order api',
      systemId: 'sys-o',
      lifecycle: 'experimental',
      runtime: 'lambda',
      health: 'critical',
      sort: 'incidents',
    })
  })

  it.each(LIFECYCLE_FILTER_VALUES)('accepts lifecycle=%s', (value) => {
    expect(parseServiceFilters({ lifecycle: value }, LOADED).lifecycle).toBe(value)
  })

  it.each(RUNTIME_FILTER_VALUES)('accepts runtime=%s', (value) => {
    expect(parseServiceFilters({ runtime: value }, LOADED).runtime).toBe(value)
  })

  it.each(HEALTH_FILTER_VALUES)('accepts health=%s', (value) => {
    expect(parseServiceFilters({ health: value }, LOADED).health).toBe(value)
  })

  it.each(SORT_FILTER_VALUES)('accepts sort=%s', (value) => {
    expect(parseServiceFilters({ sort: value }, LOADED).sort).toBe(value)
  })

  it('falls back to defaults for unsupported enum values', () => {
    const parsed = parseServiceFilters({
      lifecycle: 'legacy',
      runtime: 'fargate',
      health: 'degraded',
      sort: 'owner',
    }, LOADED)
    expect(parsed.lifecycle).toBe('all')
    expect(parsed.runtime).toBe('all')
    expect(parsed.health).toBe('all')
    expect(parsed.sort).toBe('health')
  })

  it('falls back to defaults for null values (?health)', () => {
    const parsed = parseServiceFilters({
      q: null, system: null, lifecycle: null, runtime: null, health: null, sort: null,
    }, LOADED)
    expect(parsed).toEqual({ ...DEFAULT_SERVICE_FILTERS })
  })

  it('falls back to defaults for repeated (array) values', () => {
    const parsed = parseServiceFilters({
      q: ['a', 'b'],
      system: ['sys-p', 'sys-o'],
      lifecycle: ['production', 'retired'],
      runtime: ['eks', 'rds'],
      health: ['critical', 'warning'],
      sort: ['name', 'health'],
    }, LOADED)
    expect(parsed).toEqual({ ...DEFAULT_SERVICE_FILTERS })
  })

  it('tolerates surrounding whitespace on enums', () => {
    expect(parseServiceFilters({ health: ' warning ', sort: ' name ' }, LOADED)).toMatchObject({
      health: 'warning',
      sort: 'name',
    })
  })

  it('is case sensitive — casing variants are unsupported', () => {
    expect(parseServiceFilters({ health: 'Critical' }, LOADED).health).toBe('all')
    expect(parseServiceFilters({ runtime: 'EKS' }, LOADED).runtime).toBe('all')
  })

  it('keeps the raw search string, including inner spacing', () => {
    expect(parseServiceFilters({ q: 'order  api' }, LOADED).search).toBe('order  api')
  })

  it('treats a whitespace-only search as absent once round-tripped', () => {
    expect(normalizeServiceFilters(filters({ search: '   ' }), LOADED).search).toBe('')
  })

  describe('system validation', () => {
    it('accepts the all/none sentinels', () => {
      expect(parseServiceFilters({ system: 'all' }, LOADED).systemId).toBe('all')
      expect(parseServiceFilters({ system: 'none' }, LOADED).systemId).toBe('none')
    })

    it('accepts a known system id', () => {
      expect(parseServiceFilters({ system: 'sys-p' }, LOADED).systemId).toBe('sys-p')
    })

    it('drops an unknown system id once systems are loaded', () => {
      expect(parseServiceFilters({ system: 'sys-gone' }, LOADED).systemId).toBe('all')
    })

    it('drops an unknown system id when the loaded list is empty', () => {
      expect(
        parseServiceFilters({ system: 'sys-p' }, { knownSystemIds: [], systemsReady: true }).systemId,
      ).toBe('all')
    })

    it('keeps the selection while systems are still loading', () => {
      expect(
        parseServiceFilters({ system: 'sys-p' }, { knownSystemIds: [], systemsReady: false }).systemId,
      ).toBe('sys-p')
      expect(
        parseServiceFilters({ system: 'sys-p' }, { knownSystemIds: null, systemsReady: true }).systemId,
      ).toBe('sys-p')
      expect(parseServiceFilters({ system: 'sys-p' }).systemId).toBe('sys-p')
    })

    it('still rejects an empty system value while loading', () => {
      expect(parseServiceFilters({ system: '  ' }, { systemsReady: false }).systemId).toBe('all')
    })
  })
})

describe('serviceFiltersToQuery', () => {
  it('omits every default', () => {
    expect(serviceFiltersToQuery({ ...DEFAULT_SERVICE_FILTERS })).toEqual({})
  })

  it('omits a whitespace-only search', () => {
    expect(serviceFiltersToQuery(filters({ search: '  \t ' }))).toEqual({})
  })

  it('writes only the non-default fields', () => {
    expect(serviceFiltersToQuery(filters({ health: 'critical', sort: 'name' }))).toEqual({
      health: 'critical',
      sort: 'name',
    })
  })

  it('uses the documented key names', () => {
    expect(serviceFiltersToQuery(filters({
      search: 'api', systemId: 'sys-p', lifecycle: 'retired',
      runtime: 'rds', health: 'warning', sort: 'incidents',
    }))).toEqual({
      [SERVICE_FILTER_QUERY_KEYS.search]: 'api',
      [SERVICE_FILTER_QUERY_KEYS.systemId]: 'sys-p',
      [SERVICE_FILTER_QUERY_KEYS.lifecycle]: 'retired',
      [SERVICE_FILTER_QUERY_KEYS.runtime]: 'rds',
      [SERVICE_FILTER_QUERY_KEYS.health]: 'warning',
      [SERVICE_FILTER_QUERY_KEYS.sort]: 'incidents',
    })
  })

  it('round-trips: parse(serialize(x)) is stable', () => {
    const source = filters({
      search: 'order api', systemId: 'sys-o', lifecycle: 'experimental',
      runtime: 'lambda', health: 'critical', sort: 'incidents',
    })
    const once = parseServiceFilters(serviceFiltersToQuery(source), LOADED)
    const twice = parseServiceFilters(serviceFiltersToQuery(once), LOADED)
    expect(once).toEqual(source)
    expect(twice).toEqual(once)
  })
})

describe('mergeServiceFiltersIntoQuery', () => {
  it('preserves unrelated query params', () => {
    const merged = mergeServiceFiltersIntoQuery(
      { tab: 'grid', ref: 'slack', page: '2' },
      filters({ health: 'critical' }),
    )
    expect(merged).toEqual({ tab: 'grid', ref: 'slack', page: '2', health: 'critical' })
  })

  it('preserves unrelated array and null params verbatim', () => {
    const merged = mergeServiceFiltersIntoQuery(
      { tags: ['a', 'b'], debug: null },
      filters({ sort: 'name' }),
    )
    expect(merged).toEqual({ tags: ['a', 'b'], debug: null, sort: 'name' })
  })

  it('removes owned keys that fell back to their default', () => {
    const merged = mergeServiceFiltersIntoQuery(
      { q: 'api', health: 'critical', sort: 'name', tab: 'grid' },
      filters({ sort: 'name' }),
    )
    expect(merged).toEqual({ sort: 'name', tab: 'grid' })
    expect(merged).not.toHaveProperty('q')
    expect(merged).not.toHaveProperty('health')
  })

  it('drops stale array/null values from owned keys', () => {
    const merged = mergeServiceFiltersIntoQuery(
      { health: ['critical', 'warning'], sort: null },
      filters({ health: 'warning' }),
    )
    expect(merged).toEqual({ health: 'warning' })
  })

  it('handles a missing current query', () => {
    expect(mergeServiceFiltersIntoQuery(undefined, filters({ runtime: 'eks' }))).toEqual({ runtime: 'eks' })
  })
})

describe('signatures and equality', () => {
  it('signature ignores unrelated params', () => {
    expect(serviceFilterQuerySignature({ health: 'critical', tab: 'grid' }))
      .toBe(serviceFilterQuerySignature({ health: 'critical', page: '9' }))
  })

  it('signature distinguishes absent from null from array', () => {
    const absent = serviceFilterQuerySignature({})
    const nulled = serviceFilterQuerySignature({ health: null })
    const arrayed = serviceFilterQuerySignature({ health: ['critical'] })
    expect(new Set([absent, nulled, arrayed]).size).toBe(3)
  })

  it('isCanonicalServiceFilterQuery detects redundant defaults', () => {
    expect(isCanonicalServiceFilterQuery({ health: 'all' }, { ...DEFAULT_SERVICE_FILTERS })).toBe(false)
    expect(isCanonicalServiceFilterQuery({}, { ...DEFAULT_SERVICE_FILTERS })).toBe(true)
    expect(isCanonicalServiceFilterQuery({ health: 'critical', tab: 'x' }, filters({ health: 'critical' }))).toBe(true)
    expect(isCanonicalServiceFilterQuery({ health: 'bogus' }, { ...DEFAULT_SERVICE_FILTERS })).toBe(false)
  })

  it('serviceFiltersEqual compares every field', () => {
    expect(serviceFiltersEqual(filters(), filters())).toBe(true)
    for (const key of ['search', 'systemId', 'lifecycle', 'runtime', 'health', 'sort'] as const) {
      expect(serviceFiltersEqual(filters(), filters({ [key]: 'changed' }))).toBe(false)
    }
  })

  it('isSearchOnlyChange only fires when search alone moved', () => {
    expect(isSearchOnlyChange(filters(), filters({ search: 'api' }))).toBe(true)
    expect(isSearchOnlyChange(filters(), filters())).toBe(false)
    expect(isSearchOnlyChange(filters(), filters({ search: 'api', health: 'critical' }))).toBe(false)
    expect(isSearchOnlyChange(filters(), filters({ health: 'critical' }))).toBe(false)
  })
})
