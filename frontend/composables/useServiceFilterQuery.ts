/**
 * useServiceFilterQuery — keeps Services page filter state and the route query
 * in sync, in both directions.
 *
 * Behaviour:
 *  - First load restores every filter from the URL (validated, see
 *    serviceFilterQuery.ts) and canonicalises the URL with `replace` so
 *    unsupported/default params disappear without adding a history entry.
 *  - Local changes are written back. Discrete selections `push` (so
 *    back/forward walks the filter history); search-only changes are
 *    debounced and `replace`d, so typing never floods the history stack.
 *  - Back/forward and externally rewritten queries flow back into the
 *    controls. Two guards prevent feedback loops:
 *      1. an inbound query that normalises to the current state is ignored;
 *      2. an inbound query is refused while a *newer* local selection is
 *         still unwritten — the local intent is re-asserted instead.
 *  - Unrelated query params and the URL hash are always preserved.
 *
 * The composable owns no data fetching and widens no authorization scope: it
 * only reshuffles client-side state that the grid already had in memory.
 */
import { getCurrentInstance, onMounted, onScopeDispose, ref, toValue, watch } from 'vue'
import type { MaybeRefOrGetter, Ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  DEFAULT_SERVICE_FILTERS,
  type ParseServiceFiltersOptions,
  type ServiceFilters,
  isCanonicalServiceFilterQuery,
  isSearchOnlyChange,
  mergeServiceFiltersIntoQuery,
  normalizeServiceFilters,
  parseServiceFilters,
  serviceFilterQuerySignature,
  serviceFiltersEqual,
} from '@/composables/serviceFilterQuery'

/** Debounce applied to search-only URL writes (ms). */
export const SEARCH_QUERY_DEBOUNCE_MS = 300

export interface UseServiceFilterQueryOptions {
  /** Ids of the systems the client knows about, for validating `?system=`. */
  knownSystemIds?: MaybeRefOrGetter<readonly string[] | null | undefined>
  /** `false` while the overview payload is loading — keeps `?system=` intact. */
  systemsReady?: MaybeRefOrGetter<boolean>
  /** Override the search debounce; `0` writes synchronously (tests). */
  searchDebounceMs?: number
}

export interface UseServiceFilterQueryReturn {
  filters: Ref<ServiceFilters>
  /** Replace the whole filter state (the `update:filters` handler). */
  setFilters: (next: ServiceFilters) => void
  /** Patch a single field. */
  patchFilters: (patch: Partial<ServiceFilters>) => void
  /** Back to documented defaults (clears the params from the URL). */
  resetFilters: () => void
}

export function useServiceFilterQuery(
  options: UseServiceFilterQueryOptions = {},
): UseServiceFilterQueryReturn {
  const route = useRoute()
  const router = useRouter()
  const debounceMs = options.searchDebounceMs ?? SEARCH_QUERY_DEBOUNCE_MS

  function parseOptions(): ParseServiceFiltersOptions {
    return {
      knownSystemIds: toValue(options.knownSystemIds) ?? null,
      systemsReady: toValue(options.systemsReady) ?? false,
    }
  }

  const filters = ref<ServiceFilters>(parseServiceFilters(route.query, parseOptions()))

  // Monotonic counters. `local` advances on every state change (from either
  // direction); `synced` records the revision we have already handed to the
  // router. `local > synced` therefore means "a newer selection is pending",
  // which an inbound route update must not overwrite.
  let localRevision = 0
  let syncedRevision = 0
  let pendingTimer: ReturnType<typeof setTimeout> | null = null

  function clearPending() {
    if (pendingTimer !== null) {
      clearTimeout(pendingTimer)
      pendingTimer = null
    }
  }

  function writeNow(mode: 'push' | 'replace') {
    const revision = localRevision
    // Nothing to do when the URL already says exactly this.
    if (isCanonicalServiceFilterQuery(route.query, filters.value)) {
      syncedRevision = revision
      return
    }
    // Marked synced *before* awaiting navigation: a filter change landing
    // during the navigation bumps localRevision past this value and the route
    // echo is then recognised as stale.
    syncedRevision = revision
    const query = mergeServiceFiltersIntoQuery(route.query, filters.value)
    Promise.resolve(router[mode]({ path: route.path, query, hash: route.hash })).catch(() => {
      // Duplicated/aborted navigation — the next write reconciles.
    })
  }

  function scheduleWrite(mode: 'push' | 'replace', delay: number) {
    clearPending()
    if (delay <= 0) {
      writeNow(mode)
      return
    }
    pendingTimer = setTimeout(() => {
      pendingTimer = null
      writeNow(mode)
    }, delay)
  }

  function setFilters(next: ServiceFilters) {
    const previous = filters.value
    if (serviceFiltersEqual(previous, next)) return
    filters.value = { ...next }
    localRevision++
    if (isSearchOnlyChange(previous, next)) {
      // Typing: collapse into the current history entry.
      scheduleWrite('replace', debounceMs)
    } else {
      // A discrete selection also flushes any pending search text.
      scheduleWrite('push', 0)
    }
  }

  function patchFilters(patch: Partial<ServiceFilters>) {
    setFilters({ ...filters.value, ...patch })
  }

  function resetFilters() {
    setFilters({ ...DEFAULT_SERVICE_FILTERS })
  }

  function applyRouteQuery() {
    const parseOpts = parseOptions()
    const incoming = parseServiceFilters(route.query, parseOpts)
    const sameAsLocal = serviceFiltersEqual(incoming, normalizeServiceFilters(filters.value, parseOpts))

    if (!sameAsLocal) {
      // A newer local selection has not reached the URL yet — it wins.
      if (localRevision > syncedRevision) {
        scheduleWrite('replace', 0)
        return
      }
      clearPending()
      filters.value = incoming
      syncedRevision = ++localRevision
    }

    // An externally supplied query may spell defaults out or carry garbage.
    // Tidy it with `replace` so back/forward keep working.
    if (!isCanonicalServiceFilterQuery(route.query, filters.value)) {
      scheduleWrite('replace', 0)
    }
  }

  watch(() => serviceFilterQuerySignature(route.query), () => {
    applyRouteQuery()
  })

  // Once the systems list arrives, drop a `?system=` id that does not exist.
  watch(
    () => `${toValue(options.systemsReady) ?? false}\u0000${(toValue(options.knownSystemIds) ?? []).join('\u0001')}`,
    () => {
      if (!(toValue(options.systemsReady) ?? false)) return
      const current = filters.value.systemId
      if (current === 'all' || current === 'none') return
      if ((toValue(options.knownSystemIds) ?? []).includes(current)) return
      filters.value = { ...filters.value, systemId: DEFAULT_SERVICE_FILTERS.systemId }
      syncedRevision = ++localRevision
      scheduleWrite('replace', 0)
    },
  )

  function canonicaliseInitialUrl() {
    if (import.meta.server) return
    if (isCanonicalServiceFilterQuery(route.query, filters.value)) return
    scheduleWrite('replace', 0)
  }

  if (getCurrentInstance()) {
    onMounted(canonicaliseInitialUrl)
    onScopeDispose(clearPending)
  } else {
    canonicaliseInitialUrl()
  }

  return { filters, setFilters, patchFilters, resetFilters }
}
