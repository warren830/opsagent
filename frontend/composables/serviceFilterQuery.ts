/**
 * serviceFilterQuery — pure (Vue-free) codec between the Services page filter
 * state and the route query string.
 *
 * Contract (see docs/services-url-filters.md):
 *
 *   ?q=<free text>&system=<all|none|uuid>&lifecycle=<enum>
 *   &runtime=<enum>&health=<enum>&sort=<enum>
 *
 * Rules:
 *  - Every key is optional. A key at its default value is NEVER written, so a
 *    pristine view has a clean URL.
 *  - Unsupported values fall back to the documented default. That covers a
 *    missing key, `?health` (null value), repeated keys (`?health=a&health=b`
 *    → array) and typos (`?sort=bogus`).
 *  - `system` is data-dependent, not an enum. While the systems list is still
 *    loading we keep whatever id the URL carried; once loaded, an id that does
 *    not exist degrades to `all`.
 *  - Keys not owned by this module are preserved untouched by
 *    `mergeServiceFiltersIntoQuery`.
 *
 * Kept free of Vue/vue-router imports so it is trivially unit-testable and can
 * be reused by any caller (page, composable, deep-link builders).
 */
import { HEALTH_FILTER_VALUES } from '@/components/services/cardRegistry'

/** A single route-query value as vue-router models it. */
export type ServiceFilterQueryValue = string | null | undefined | (string | null)[]

/** A loose route-query record (`LocationQuery` is assignable to this). */
export type ServiceFilterQueryRecord = Record<string, ServiceFilterQueryValue>

/** Filter state owned by the Services page. */
export interface ServiceFilters {
  /** Free-text match against name / display name / description / tags. */
  search: string
  /** `'all'` | `'none'` (components without a system) | system uuid. */
  systemId: string
  /** `'all'` | production | experimental | deprecated | retired. */
  lifecycle: string
  /** `'all'` | eks | ec2 | rds | lambda | external | generic. */
  runtime: string
  /** `'all'` | critical | warning | unknown | healthy. */
  health: string
  /** health | name | incidents. */
  sort: string
}

/** Filter field → query-string key. */
export const SERVICE_FILTER_QUERY_KEYS = {
  search: 'q',
  systemId: 'system',
  lifecycle: 'lifecycle',
  runtime: 'runtime',
  health: 'health',
  sort: 'sort',
} as const satisfies Record<keyof ServiceFilters, string>

/** Query keys this module owns; everything else in the URL is left alone. */
export const SERVICE_FILTER_OWNED_KEYS: readonly string[] = Object.values(SERVICE_FILTER_QUERY_KEYS)

export const LIFECYCLE_FILTER_VALUES = [
  'all',
  'production',
  'experimental',
  'deprecated',
  'retired',
] as const

export const RUNTIME_FILTER_VALUES = [
  'all',
  'eks',
  'ec2',
  'rds',
  'lambda',
  'external',
  'generic',
] as const

export const SORT_FILTER_VALUES = ['health', 'name', 'incidents'] as const

/** Sentinels that are always valid for `system`, regardless of loaded data. */
export const SYSTEM_FILTER_SENTINELS = ['all', 'none'] as const

/** Documented defaults. Values equal to these are omitted from the URL. */
export const DEFAULT_SERVICE_FILTERS: Readonly<ServiceFilters> = Object.freeze({
  search: '',
  systemId: 'all',
  lifecycle: 'all',
  runtime: 'all',
  health: 'all',
  sort: 'health',
})

export interface ParseServiceFiltersOptions {
  /**
   * Ids of the systems currently known to the client. Used to validate the
   * `system` param. `null`/omitted means "unknown yet" and keeps the value.
   */
  knownSystemIds?: readonly string[] | null
  /**
   * `false` while the overview payload is still loading. A valid-looking
   * `system` id survives the load so shared links do not reset to "all"
   * before the data lands.
   */
  systemsReady?: boolean
}

/**
 * Reads a scalar string out of a route-query value. `null` (`?health`),
 * `undefined` (absent) and arrays (`?health=a&health=b`) are unsupported and
 * return `null` so the caller substitutes the default.
 */
function readScalar(value: ServiceFilterQueryValue): string | null {
  return typeof value === 'string' ? value : null
}

function readEnum(
  value: ServiceFilterQueryValue,
  allowed: readonly string[],
  fallback: string,
): string {
  const raw = readScalar(value)?.trim()
  if (!raw) return fallback
  return allowed.includes(raw) ? raw : fallback
}

function readSystemId(
  value: ServiceFilterQueryValue,
  options: ParseServiceFiltersOptions,
): string {
  const raw = readScalar(value)?.trim()
  if (!raw) return DEFAULT_SERVICE_FILTERS.systemId
  if ((SYSTEM_FILTER_SENTINELS as readonly string[]).includes(raw)) return raw

  const known = options.knownSystemIds
  // Not loaded yet → cannot validate, so keep the selection.
  if (known == null || options.systemsReady === false) return raw
  return known.includes(raw) ? raw : DEFAULT_SERVICE_FILTERS.systemId
}

/** Parses a route query into a fully-populated, validated filter state. */
export function parseServiceFilters(
  query: ServiceFilterQueryRecord | null | undefined,
  options: ParseServiceFiltersOptions = {},
): ServiceFilters {
  const q = query ?? {}
  return {
    search: readScalar(q[SERVICE_FILTER_QUERY_KEYS.search]) ?? DEFAULT_SERVICE_FILTERS.search,
    systemId: readSystemId(q[SERVICE_FILTER_QUERY_KEYS.systemId], options),
    lifecycle: readEnum(
      q[SERVICE_FILTER_QUERY_KEYS.lifecycle],
      LIFECYCLE_FILTER_VALUES,
      DEFAULT_SERVICE_FILTERS.lifecycle,
    ),
    runtime: readEnum(
      q[SERVICE_FILTER_QUERY_KEYS.runtime],
      RUNTIME_FILTER_VALUES,
      DEFAULT_SERVICE_FILTERS.runtime,
    ),
    health: readEnum(
      q[SERVICE_FILTER_QUERY_KEYS.health],
      HEALTH_FILTER_VALUES,
      DEFAULT_SERVICE_FILTERS.health,
    ),
    sort: readEnum(
      q[SERVICE_FILTER_QUERY_KEYS.sort],
      SORT_FILTER_VALUES,
      DEFAULT_SERVICE_FILTERS.sort,
    ),
  }
}

/**
 * Serialises filters to the owned subset of a query record. Defaults are
 * omitted, so `serviceFiltersToQuery(DEFAULT_SERVICE_FILTERS)` is `{}`.
 */
export function serviceFiltersToQuery(filters: ServiceFilters): Record<string, string> {
  const out: Record<string, string> = {}
  // Whitespace-only search is not a filter — treat it as absent.
  if (filters.search.trim() !== '') out[SERVICE_FILTER_QUERY_KEYS.search] = filters.search
  for (const key of ['systemId', 'lifecycle', 'runtime', 'health', 'sort'] as const) {
    const value = filters[key]
    if (!value || value === DEFAULT_SERVICE_FILTERS[key]) continue
    out[SERVICE_FILTER_QUERY_KEYS[key]] = value
  }
  return out
}

/**
 * Rebuilds a full query record: unrelated params are carried over verbatim,
 * the owned keys are replaced by the (default-stripped) serialisation.
 */
export function mergeServiceFiltersIntoQuery(
  current: ServiceFilterQueryRecord | null | undefined,
  filters: ServiceFilters,
): ServiceFilterQueryRecord {
  const next: ServiceFilterQueryRecord = {}
  for (const [key, value] of Object.entries(current ?? {})) {
    if (SERVICE_FILTER_OWNED_KEYS.includes(key)) continue
    if (value === undefined) continue
    next[key] = value
  }
  return Object.assign(next, serviceFiltersToQuery(filters))
}

/** Picks only the filter-owned keys out of a query record. */
export function pickServiceFilterQuery(
  query: ServiceFilterQueryRecord | null | undefined,
): ServiceFilterQueryRecord {
  const out: ServiceFilterQueryRecord = {}
  for (const key of SERVICE_FILTER_OWNED_KEYS) {
    const value = (query ?? {})[key]
    if (value === undefined) continue
    out[key] = value
  }
  return out
}

/**
 * Stable signature of the owned query keys. Used to watch for *relevant*
 * route changes without re-running on unrelated param churn.
 */
export function serviceFilterQuerySignature(
  query: ServiceFilterQueryRecord | null | undefined,
): string {
  const owned = pickServiceFilterQuery(query)
  return SERVICE_FILTER_OWNED_KEYS
    .map((key) => {
      const value = owned[key]
      if (value === undefined) return `${key}=\u0000absent`
      if (value === null) return `${key}=\u0000null`
      if (Array.isArray(value)) return `${key}=\u0000[${value.map(v => v ?? '\u0000null').join('\u0001')}]`
      return `${key}=${value}`
    })
    .join('\u0002')
}

/** True when the URL already carries exactly the canonical serialisation. */
export function isCanonicalServiceFilterQuery(
  query: ServiceFilterQueryRecord | null | undefined,
  filters: ServiceFilters,
): boolean {
  return serviceFilterQuerySignature(query) === serviceFilterQuerySignature(serviceFiltersToQuery(filters))
}

export function serviceFiltersEqual(a: ServiceFilters, b: ServiceFilters): boolean {
  return (
    a.search === b.search
    && a.systemId === b.systemId
    && a.lifecycle === b.lifecycle
    && a.runtime === b.runtime
    && a.health === b.health
    && a.sort === b.sort
  )
}

/**
 * Round-trips filters through the URL codec. Two states that normalise to the
 * same value are indistinguishable to a reader of the URL — the equality test
 * that keeps route↔state sync from looping.
 */
export function normalizeServiceFilters(
  filters: ServiceFilters,
  options: ParseServiceFiltersOptions = {},
): ServiceFilters {
  return parseServiceFilters(serviceFiltersToQuery(filters), options)
}

/** True when only `search` differs — the change class that must not push history. */
export function isSearchOnlyChange(previous: ServiceFilters, next: ServiceFilters): boolean {
  return (
    previous.search !== next.search
    && previous.systemId === next.systemId
    && previous.lifecycle === next.lifecycle
    && previous.runtime === next.runtime
    && previous.health === next.health
    && previous.sort === next.sort
  )
}
