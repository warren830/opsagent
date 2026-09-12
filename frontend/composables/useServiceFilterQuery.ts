/**
 * useServiceFilterQuery — keeps Services page filter state and the route query
 * in sync, in both directions.
 *
 * Synchronisation model
 * ---------------------
 * The route watcher is keyed on the *owned-query signature* (see
 * serviceFilterQuery.ts), so it only fires when a filter param actually
 * changes. A navigation that merely adds `?tab=` or moves the `#hash` never
 * reaches it, which means every inbound event is one of exactly two things:
 *
 *  1. the commit of a write this composable handed to the router, or
 *  2. a genuine external navigation — Back/Forward, a link, another component.
 *
 * They are told apart by `attempted`, the list of query signatures we asked
 * the router for and have not yet settled. Case 1 never touches the controls
 * (they are already the source of that URL) and is skipped outright when an
 * even newer write of ours is still outstanding. Case 2 always wins: an
 * explicit user navigation is newer than any debounced typing, so pending
 * writes are cancelled and the URL is adopted.
 *
 * Navigation failures are reconciled rather than ignored. `router.push` and
 * `router.replace` *resolve* with a NavigationFailure when a navigation is
 * aborted, cancelled or duplicated, so an optimistic write that loses a race
 * would otherwise leave the controls disagreeing with the URL. Every write is
 * awaited: a lost one is retried against the fresh route (which preserves the
 * navigation that cancelled it), and if it cannot land the URL is adopted so
 * the two never diverge silently.
 *
 * Writes are bound to a page identity (`route.path` at setup). A debounce that
 * fires while a navigation away is waiting on a guard or a lazy component must
 * not cancel the user's departure, so pending writes are dropped as soon as a
 * departure starts — not on unmount, which happens far too late.
 *
 * The composable owns no data fetching and widens no authorization scope: it
 * only reshuffles client-side state the grid already had in memory.
 */
import { computed, getCurrentInstance, onMounted, onScopeDispose, ref, toValue, watch } from 'vue'
import type { ComputedRef, MaybeRefOrGetter, Ref } from 'vue'
import { NavigationFailureType, isNavigationFailure, useRoute, useRouter } from 'vue-router'
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
  serviceFiltersToQuery,
} from '@/composables/serviceFilterQuery'

/** Debounce applied to search-only URL writes (ms). */
export const SEARCH_QUERY_DEBOUNCE_MS = 300

/** Retries for a write the router refused before the URL is adopted instead. */
export const DEFAULT_MAX_WRITE_RETRIES = 1

export interface UseServiceFilterQueryOptions {
  /** Ids of the systems the client knows about, for validating `?system=`. */
  knownSystemIds?: MaybeRefOrGetter<readonly string[] | null | undefined>
  /**
   * `true` only while the catalog in hand is **authoritative** — see
   * `usePayloadAuthority`, which is how callers should derive it.
   *
   * Neither a non-null payload nor `loading === false` says that, and neither
   * does "some response arrived once". `useServicesOverview` keeps its response
   * in module state that outlives the page and polls in the background, so the
   * list on hand may predate both this page visit and the request currently in
   * flight. Validating a `?system=` id against such a list erases a perfectly
   * valid selection.
   */
  systemsReady?: MaybeRefOrGetter<boolean>
  /** Override the search debounce; `0` writes synchronously. */
  searchDebounceMs?: number
  /** Override how often a refused write is retried before adopting the URL. */
  maxWriteRetries?: number
}

/**
 * Reports whether the payload currently in hand may be treated as the complete
 * catalog.
 *
 * Two boundaries make a payload authoritative, and both are needed:
 *
 *  - **Setup.** `useServicesOverview` caches its response in module state, and
 *    the page only starts a fetch in `onMounted` — after a child's `setup`. So a
 *    child can begin life looking at a payload from an *earlier visit* with
 *    nothing loading. Only a payload object it did not start with counts.
 *  - **Every refresh.** Polling reopens the same gap: while a refresh is in
 *    flight, the list on hand predates the response being fetched, so a system
 *    created since then would look nonexistent. A starting load therefore
 *    invalidates authority until *that* load answers with a new payload.
 *
 * A refresh that fails keeps the previous payload, so no new payload arrives and
 * authority is not restored — a pending selection survives untouched.
 */
export function usePayloadAuthority(
  payload: MaybeRefOrGetter<unknown>,
  loading: MaybeRefOrGetter<boolean> = () => false,
): ComputedRef<boolean> {
  /** Payload the in-flight (or most recent) load started from. */
  let generationStart = toValue(payload)
  const answered = ref(false)

  watch(
    () => toValue(payload),
    (current) => {
      if (current != null && current !== generationStart) answered.value = true
    },
  )

  watch(
    () => toValue(loading),
    (isLoading, wasLoading) => {
      if (!isLoading || wasLoading) return
      // A load just started: whatever is on hand predates its response.
      generationStart = toValue(payload)
      answered.value = false
    },
  )

  return computed(() => answered.value && !toValue(loading))
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
  const maxWriteRetries = options.maxWriteRetries ?? DEFAULT_MAX_WRITE_RETRIES

  /** Page this instance belongs to. Writes outside it are never valid. */
  const pageKey = route.path

  function parseOptions(): ParseServiceFiltersOptions {
    return {
      knownSystemIds: toValue(options.knownSystemIds) ?? null,
      systemsReady: toValue(options.systemsReady) ?? false,
    }
  }

  const filters = ref<ServiceFilters>(parseServiceFilters(route.query, parseOptions()))

  /** Owned-query signatures handed to the router and not yet settled. */
  const attempted: string[] = []
  let pendingTimer: ReturnType<typeof setTimeout> | null = null
  /** Navigations to another page that have started and not yet settled. */
  let departures = 0
  let disposed = false
  let retries = 0

  function clearPending() {
    if (pendingTimer !== null) {
      clearTimeout(pendingTimer)
      pendingTimer = null
    }
  }

  /** A write is only ever valid while this instance still owns the route. */
  function canWrite(): boolean {
    return !disposed && departures === 0 && route.path === pageKey
  }

  function forget(signature: string) {
    const index = attempted.indexOf(signature)
    if (index >= 0) attempted.splice(index, 1)
  }

  /** Make the controls match the URL, so the two can never disagree. */
  function adoptRoute() {
    clearPending()
    const incoming = parseServiceFilters(route.query, parseOptions())
    if (!serviceFiltersEqual(incoming, filters.value)) filters.value = incoming
  }

  function writeNow(mode: 'push' | 'replace') {
    if (!canWrite()) return
    // Nothing to do when the URL already says exactly this.
    if (isCanonicalServiceFilterQuery(route.query, filters.value)) {
      retries = 0
      return
    }
    const signature = serviceFilterQuerySignature(serviceFiltersToQuery(filters.value))
    const query = mergeServiceFiltersIntoQuery(route.query, filters.value)
    attempted.push(signature)
    void settleWrite(signature, router[mode]({ path: pageKey, query, hash: route.hash }))
  }

  /**
   * Awaits a navigation and reconciles the outcome. vue-router resolves with a
   * NavigationFailure instead of rejecting, so success cannot be assumed.
   */
  async function settleWrite(signature: string, navigation: Promise<unknown>) {
    let outcome: unknown
    try {
      outcome = await navigation
    } catch (error) {
      outcome = error
    }
    forget(signature)

    if (!outcome) {
      // Committed. The watcher sees it as our own echo and leaves state alone.
      retries = 0
      return
    }
    if (isNavigationFailure(outcome, NavigationFailureType.duplicated)) {
      // The URL already carried this target; nothing was lost.
      retries = 0
      return
    }
    reconcileRefusedWrite()
  }

  function reconcileRefusedWrite() {
    // Left the page (or on the way out) — never fight the user's departure.
    if (!canWrite()) return
    // A newer write of ours is already in flight; it decides the final URL.
    if (attempted.length > 0) return
    // Something else already put our selection in the URL.
    if (isCanonicalServiceFilterQuery(route.query, filters.value)) {
      retries = 0
      return
    }
    if (retries < maxWriteRetries) {
      retries++
      // Re-merges against the *current* route, so whatever navigation cancelled
      // us (an unrelated param, a new hash) is preserved.
      scheduleWrite('replace', 0)
      return
    }
    // Out of retries: prefer an honest URL over a silent divergence.
    retries = 0
    adoptRoute()
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
    retries = 0
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

  // Fires only when a filter param changed — see the header note.
  watch(() => serviceFilterQuerySignature(route.query), (signature) => {
    if (disposed || route.path !== pageKey) return

    const index = attempted.indexOf(signature)
    if (index >= 0) {
      // Our own write landed. Drop it and anything it superseded.
      attempted.splice(0, index + 1)
      // An even newer write of ours is still outstanding; it has the last word.
      if (attempted.length > 0) return
      retries = 0
    } else {
      // Genuine external navigation (Back/Forward, link, another component).
      // It is newer than any pending write of ours, so it wins outright.
      clearPending()
      attempted.length = 0
      retries = 0
      const parseOpts = parseOptions()
      const incoming = parseServiceFilters(route.query, parseOpts)
      if (!serviceFiltersEqual(incoming, normalizeServiceFilters(filters.value, parseOpts))) {
        filters.value = incoming
      }
    }

    // An inbound query may spell defaults out or carry garbage. Tidy it with
    // `replace` so Back/Forward keep working.
    if (!isCanonicalServiceFilterQuery(route.query, filters.value)) {
      scheduleWrite('replace', 0)
    }
  })

  // Once a completed systems list is on hand, drop a `?system=` id that does
  // not exist. Never runs against a stale list — see `systemsReady`.
  watch(
    () => `${toValue(options.systemsReady) ?? false}\u0000${(toValue(options.knownSystemIds) ?? []).join('\u0001')}`,
    () => {
      if (!(toValue(options.systemsReady) ?? false)) return
      const current = filters.value.systemId
      if (current === 'all' || current === 'none') return
      if ((toValue(options.knownSystemIds) ?? []).includes(current)) return
      filters.value = { ...filters.value, systemId: DEFAULT_SERVICE_FILTERS.systemId }
      scheduleWrite('replace', 0)
    },
  )

  function registerNavigationGuards() {
    // A departure may sit in an async guard or a lazy chunk for a long time.
    // Drop pending writes as soon as one starts so none of them cancels it.
    const stopBefore = router.beforeEach((to) => {
      if (to.path === pageKey) return
      departures++
      clearPending()
      attempted.length = 0
    })
    // Settles a departure. Only the departure's *own* outcome may clear it —
    // a same-page navigation failing while a departure is still parked must
    // not reopen writes, or the retry would cancel the departure.
    const stopAfter = router.afterEach((to, _from, failure) => {
      if (to.path === pageKey) return
      if (departures === 0) return
      departures--
      if (!failure || departures > 0 || disposed) return
      // The departure was refused and we are still here: the write that was
      // dropped for it never landed, so reconcile the URL with the controls.
      if (router.currentRoute.value.path === pageKey) reconcileRefusedWrite()
    })
    return () => {
      stopBefore()
      stopAfter()
    }
  }

  function canonicaliseInitialUrl() {
    if (isCanonicalServiceFilterQuery(route.query, filters.value)) return
    scheduleWrite('replace', 0)
  }

  if (!import.meta.server) {
    if (getCurrentInstance()) {
      const stopGuards = registerNavigationGuards()
      onMounted(canonicaliseInitialUrl)
      onScopeDispose(() => {
        disposed = true
        clearPending()
        attempted.length = 0
        stopGuards()
      })
    } else {
      // No scope to clean up after, so skip the global guards rather than leak
      // them. Route-leave protection needs a component instance.
      canonicaliseInitialUrl()
    }
  }

  return { filters, setFilters, patchFilters, resetFilters }
}
