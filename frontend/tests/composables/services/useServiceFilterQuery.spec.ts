/**
 * useServiceFilterQuery — route ⇄ filter-state sync for the Services page.
 *
 * Driven through a real vue-router instance on memory history, mounted behind a
 * real <RouterView>, so route components mount/unmount and navigation guards
 * run for real. Several cases install *delayed* guards to pin down orderings
 * that a synchronous router cannot express:
 *
 *  - Back/Forward arriving while a search debounce is still pending.
 *  - A filter write cancelled by a newer selection or by an unrelated
 *    navigation, which vue-router reports by *resolving* with a
 *    NavigationFailure rather than rejecting.
 *  - A debounce firing while a departure is parked in an async guard.
 *
 * The host component is a render-function stub, which keeps the suite free of
 * `@vitejs/plugin-vue`.
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { defineComponent, h, nextTick, ref } from 'vue'
import type { ComputedRef } from 'vue'
import {
  RouterView,
  createMemoryHistory,
  createRouter,
  isNavigationFailure,
  type NavigationFailure,
  type Router,
} from 'vue-router'
import { flushPromises, mount } from '@vue/test-utils'
import {
  SEARCH_QUERY_DEBOUNCE_MS,
  usePayloadAuthority,
  useServiceFilterQuery,
  type UseServiceFilterQueryOptions,
  type UseServiceFilterQueryReturn,
} from '~/composables/useServiceFilterQuery'
import { DEFAULT_SERVICE_FILTERS } from '~/composables/serviceFilterQuery'

// ─────────────────── deferred gate ───────────────────

interface Gate {
  promise: Promise<void>
  open: () => void
}

function gate(): Gate {
  let open!: () => void
  const promise = new Promise<void>((resolve) => {
    open = resolve
  })
  return { promise, open }
}

// ─────────────────── harness ───────────────────

/** Set per-test before mounting; read by the /services route component. */
let hostOptions: UseServiceFilterQueryOptions = {}
/** Captured composable handle of the currently mounted /services page. */
let hostApi: UseServiceFilterQueryReturn | null = null
/** Mount/unmount ledger, to prove ordering against navigation guards. */
let hostLifecycle: string[] = []
/** Blocks entry into /catalog until opened; models a slow guard or lazy chunk. */
let departureGate: Gate | null = null

const ServicesHost = defineComponent({
  name: 'ServicesHost',
  setup() {
    hostApi = useServiceFilterQuery(hostOptions)
    hostLifecycle.push('mounted')
    return () => h('div', 'services')
  },
  unmounted() {
    hostLifecycle.push('unmounted')
  },
})

const Blank = defineComponent({ name: 'Blank', render: () => h('div') })

const App = defineComponent({ name: 'App', setup: () => () => h(RouterView) })

interface Harness {
  router: Router
  api: () => UseServiceFilterQueryReturn
  navigations: string[]
  pushes: string[]
  replaces: string[]
  unmount: () => void
}

async function setup(
  initial = '/services',
  options: UseServiceFilterQueryOptions = {},
): Promise<Harness> {
  hostOptions = options
  hostApi = null
  hostLifecycle = []

  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/services', name: 'services', component: ServicesHost },
      {
        path: '/catalog',
        name: 'catalog',
        component: Blank,
        beforeEnter: async () => {
          if (departureGate) await departureGate.promise
        },
      },
      { path: '/:pathMatch(.*)*', name: 'catch-all', component: Blank },
    ],
  })

  await router.push(initial)
  await router.isReady()

  const navigations: string[] = []
  router.afterEach((to, _from, failure) => {
    if (!failure) navigations.push(to.fullPath)
  })

  const pushes: string[] = []
  const replaces: string[] = []
  const push = router.push.bind(router)
  const replace = router.replace.bind(router)
  vi.spyOn(router, 'push').mockImplementation((to) => {
    pushes.push(router.resolve(to as never).fullPath)
    return push(to as never)
  })
  vi.spyOn(router, 'replace').mockImplementation((to) => {
    replaces.push(router.resolve(to as never).fullPath)
    return replace(to as never)
  })

  const wrapper = mount(App, { global: { plugins: [router] } })
  await flushPromises()

  return {
    router,
    api: () => hostApi as UseServiceFilterQueryReturn,
    navigations,
    pushes,
    replaces,
    unmount: () => wrapper.unmount(),
  }
}

const LOADED_SYSTEMS: UseServiceFilterQueryOptions = {
  knownSystemIds: ['sys-p', 'sys-o'],
  systemsReady: true,
}

describe('useServiceFilterQuery', () => {
  afterEach(() => {
    departureGate = null
    hostApi = null
    vi.restoreAllMocks()
    vi.useRealTimers()
  })

  describe('restore on first load', () => {
    it('hydrates every filter from the URL', async () => {
      const { api, unmount } = await setup(
        '/services?q=order&system=sys-o&lifecycle=experimental&runtime=lambda&health=critical&sort=incidents',
        LOADED_SYSTEMS,
      )
      expect(api().filters.value).toEqual({
        search: 'order',
        systemId: 'sys-o',
        lifecycle: 'experimental',
        runtime: 'lambda',
        health: 'critical',
        sort: 'incidents',
      })
      unmount()
    })

    it('leaves a clean URL untouched', async () => {
      const { api, navigations, unmount } = await setup('/services', LOADED_SYSTEMS)
      expect(api().filters.value).toEqual({ ...DEFAULT_SERVICE_FILTERS })
      expect(navigations).toEqual([])
      unmount()
    })

    it('leaves an already-canonical URL untouched', async () => {
      const { navigations, unmount } = await setup('/services?health=critical', LOADED_SYSTEMS)
      expect(navigations).toEqual([])
      unmount()
    })

    it('canonicalises unsupported and default params with replace', async () => {
      const { api, router, pushes, replaces, unmount } = await setup(
        '/services?health=bogus&sort=health&runtime=eks',
        LOADED_SYSTEMS,
      )
      expect(api().filters.value.health).toBe('all')
      expect(api().filters.value.sort).toBe('health')
      expect(api().filters.value.runtime).toBe('eks')
      expect(router.currentRoute.value.query).toEqual({ runtime: 'eks' })
      expect(replaces).toHaveLength(1)
      expect(pushes).toHaveLength(0)
      unmount()
    })
  })

  describe('system validation against a loading list', () => {
    it('keeps a newly selected system until an authoritative list arrives', async () => {
      const ready = ref(false)
      const ids = ref<string[]>(['sys-old'])
      const { api, router, replaces, unmount } = await setup('/services?system=sys-new', {
        knownSystemIds: ids,
        systemsReady: ready,
      })

      expect(api().filters.value.systemId).toBe('sys-new')
      expect(router.currentRoute.value.query).toEqual({ system: 'sys-new' })
      expect(replaces).toHaveLength(0)

      // The authoritative response confirms the system.
      ids.value = ['sys-old', 'sys-new']
      ready.value = true
      await flushPromises()

      expect(api().filters.value.systemId).toBe('sys-new')
      expect(replaces).toHaveLength(0)
      unmount()
    })

    it('drops the system only once an authoritative list disowns it', async () => {
      const ready = ref(false)
      const ids = ref<string[]>(['sys-old'])
      const { api, router, unmount } = await setup('/services?system=sys-gone', {
        knownSystemIds: ids,
        systemsReady: ready,
      })
      expect(api().filters.value.systemId).toBe('sys-gone')

      ready.value = true
      await flushPromises()

      expect(api().filters.value.systemId).toBe('all')
      expect(router.currentRoute.value.query).toEqual({})
      unmount()
    })

    it('keeps a selection made before the list is authoritative', async () => {
      const ready = ref(false)
      const ids = ref<string[]>(['sys-p'])
      const { api, unmount } = await setup('/services', {
        knownSystemIds: ids,
        systemsReady: ready,
      })
      api().patchFilters({ systemId: 'sys-fresh' })
      await flushPromises()
      expect(api().filters.value.systemId).toBe('sys-fresh')

      // The response that finally arrives confirms it.
      ids.value = ['sys-p', 'sys-fresh']
      ready.value = true
      await flushPromises()
      expect(api().filters.value.systemId).toBe('sys-fresh')
      unmount()
    })
  })

  describe('usePayloadAuthority', () => {
    /** Mounts the tracker over payload + loading refs, returning all sides. */
    function track(initial: unknown, initialLoading = false) {
      const source = ref(initial)
      const loading = ref(initialLoading)
      let authoritative: ComputedRef<boolean> | null = null
      const wrapper = mount(
        defineComponent({
          setup() {
            authoritative = usePayloadAuthority(() => source.value, () => loading.value)
            return () => h('div')
          },
        }),
      )
      return {
        source,
        loading,
        value: () => (authoritative as ComputedRef<boolean>).value,
        unmount: () => wrapper.unmount(),
      }
    }

    const response = () => ({ systems: [], components: [] })

    it('a payload present at setup is not authority', async () => {
      const t = track(response())
      expect(t.value()).toBe(false)
      await nextTick()
      expect(t.value()).toBe(false)
      t.unmount()
    })

    it('a payload the instance did not start with grants authority', async () => {
      const t = track(response())
      t.source.value = response() // distinct object
      await nextTick()
      expect(t.value()).toBe(true)
      t.unmount()
    })

    it('a cold start grants authority on the first payload', async () => {
      const t = track(null)
      expect(t.value()).toBe(false)
      t.source.value = response()
      await nextTick()
      expect(t.value()).toBe(true)
      t.unmount()
    })

    it('re-assigning the same object is not a new response', async () => {
      const cached = response()
      const t = track(cached)
      t.source.value = cached
      await nextTick()
      expect(t.value()).toBe(false)
      t.unmount()
    })

    it('a starting refresh revokes authority until that refresh answers', async () => {
      const t = track(null)
      t.source.value = response()
      await nextTick()
      expect(t.value()).toBe(true)

      // Poll N+1 begins: the list on hand predates its response.
      t.loading.value = true
      await nextTick()
      expect(t.value()).toBe(false)

      t.source.value = response()
      t.loading.value = false
      await nextTick()
      expect(t.value()).toBe(true)
      t.unmount()
    })

    it('a refresh that keeps its old payload never regains authority', async () => {
      const t = track(null)
      const first = response()
      t.source.value = first
      await nextTick()
      expect(t.value()).toBe(true)

      t.loading.value = true
      await nextTick()
      // Failure path: payload untouched, loading clears.
      t.loading.value = false
      await nextTick()
      expect(t.value()).toBe(false)

      // Only the next successful poll restores it.
      t.loading.value = true
      await nextTick()
      t.source.value = response()
      t.loading.value = false
      await nextTick()
      expect(t.value()).toBe(true)
      t.unmount()
    })

    it('a payload arriving before loading clears is not authority yet', async () => {
      const t = track(null, true)
      t.source.value = response()
      await nextTick()
      expect(t.value()).toBe(false)
      t.loading.value = false
      await nextTick()
      expect(t.value()).toBe(true)
      t.unmount()
    })

    it('defaults to no loading signal when none is supplied', async () => {
      const source = ref<unknown>(null)
      let authoritative: ComputedRef<boolean> | null = null
      const wrapper = mount(
        defineComponent({
          setup() {
            authoritative = usePayloadAuthority(() => source.value)
            return () => h('div')
          },
        }),
      )
      source.value = { systems: [], components: [] }
      await nextTick()
      expect((authoritative as ComputedRef<boolean>).value).toBe(true)
      wrapper.unmount()
    })
  })

  describe('writing local changes', () => {
    it('pushes discrete selections and omits defaults', async () => {
      const { api, router, pushes, unmount } = await setup('/services', LOADED_SYSTEMS)
      api().patchFilters({ health: 'critical' })
      await flushPromises()
      expect(router.currentRoute.value.fullPath).toBe('/services?health=critical')
      expect(pushes).toEqual(['/services?health=critical'])

      api().patchFilters({ health: 'all' })
      await flushPromises()
      expect(router.currentRoute.value.fullPath).toBe('/services')
      unmount()
    })

    it('debounces search into a single replace, never a push', async () => {
      vi.useFakeTimers()
      const { api, router, pushes, replaces, unmount } = await setup('/services', LOADED_SYSTEMS)
      for (const text of ['o', 'or', 'ord', 'orde', 'order']) {
        api().patchFilters({ search: text })
        vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS / 3)
      }
      expect(replaces).toHaveLength(0)

      vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS)
      await flushPromises()

      expect(replaces).toEqual(['/services?q=order'])
      expect(pushes).toHaveLength(0)
      expect(router.currentRoute.value.fullPath).toBe('/services?q=order')
      expect(api().filters.value.search).toBe('order')
      unmount()
    })

    it('a selection flushes pending search text in one push', async () => {
      vi.useFakeTimers()
      const { api, router, pushes, replaces, unmount } = await setup('/services', LOADED_SYSTEMS)
      api().patchFilters({ search: 'api' })
      api().patchFilters({ health: 'warning' })
      await flushPromises()

      expect(pushes).toEqual(['/services?q=api&health=warning'])
      expect(replaces).toHaveLength(0)
      expect(router.currentRoute.value.query).toEqual({ q: 'api', health: 'warning' })

      vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS * 2)
      await flushPromises()
      expect(pushes).toHaveLength(1)
      unmount()
    })

    it('ignores a no-op update', async () => {
      const { api, navigations, unmount } = await setup('/services?health=critical', LOADED_SYSTEMS)
      api().patchFilters({ health: 'critical' })
      await flushPromises()
      expect(navigations).toEqual([])
      unmount()
    })

    it('resetFilters clears every owned param', async () => {
      const { api, router, unmount } = await setup(
        '/services?q=api&health=critical&sort=name',
        LOADED_SYSTEMS,
      )
      api().resetFilters()
      await flushPromises()
      expect(router.currentRoute.value.fullPath).toBe('/services')
      expect(api().filters.value).toEqual({ ...DEFAULT_SERVICE_FILTERS })
      unmount()
    })

    it('preserves unrelated params and the hash', async () => {
      const { api, router, unmount } = await setup(
        '/services?tab=grid&ref=slack#pay',
        LOADED_SYSTEMS,
      )
      api().patchFilters({ runtime: 'rds' })
      await flushPromises()
      expect(router.currentRoute.value.query).toEqual({ tab: 'grid', ref: 'slack', runtime: 'rds' })
      expect(router.currentRoute.value.hash).toBe('#pay')

      api().patchFilters({ runtime: 'all' })
      await flushPromises()
      expect(router.currentRoute.value.query).toEqual({ tab: 'grid', ref: 'slack' })
      expect(router.currentRoute.value.hash).toBe('#pay')
      unmount()
    })
  })

  describe('reacting to route changes', () => {
    it('back and forward move the controls', async () => {
      const { api, router, unmount } = await setup('/services', LOADED_SYSTEMS)
      api().patchFilters({ health: 'critical' })
      await flushPromises()
      api().patchFilters({ sort: 'name' })
      await flushPromises()
      expect(api().filters.value).toMatchObject({ health: 'critical', sort: 'name' })

      router.back()
      await flushPromises()
      expect(api().filters.value).toMatchObject({ health: 'critical', sort: 'health' })

      router.back()
      await flushPromises()
      expect(api().filters.value).toEqual({ ...DEFAULT_SERVICE_FILTERS })

      router.forward()
      await flushPromises()
      expect(api().filters.value).toMatchObject({ health: 'critical', sort: 'health' })
      unmount()
    })

    it('an externally rewritten query updates the controls without looping', async () => {
      const { api, router, navigations, unmount } = await setup('/services', LOADED_SYSTEMS)
      await router.replace({ path: '/services', query: { q: 'redis', system: 'sys-p', sort: 'name' } })
      await flushPromises()

      expect(api().filters.value).toMatchObject({ search: 'redis', systemId: 'sys-p', sort: 'name' })
      expect(navigations).toHaveLength(1)
      unmount()
    })

    it('sanitises an externally supplied invalid query in one extra replace', async () => {
      const { api, router, navigations, unmount } = await setup('/services', LOADED_SYSTEMS)
      await router.replace({
        path: '/services',
        query: { health: ['critical', 'warning'], sort: 'owner', runtime: 'eks', tab: 'grid' },
      })
      await flushPromises()

      expect(api().filters.value).toMatchObject({ health: 'all', sort: 'health', runtime: 'eks' })
      expect(router.currentRoute.value.query).toEqual({ runtime: 'eks', tab: 'grid' })
      expect(navigations).toHaveLength(2)
      unmount()
    })

    it('strips a spelled-out default arriving from outside', async () => {
      const { router, navigations, unmount } = await setup('/services', LOADED_SYSTEMS)
      await router.replace({ path: '/services', query: { health: 'all', sort: 'health' } })
      await flushPromises()
      expect(router.currentRoute.value.query).toEqual({})
      expect(navigations).toHaveLength(2)
      unmount()
    })

    it('ignores unrelated param churn', async () => {
      const { api, router, replaces, unmount } = await setup('/services?health=warning', LOADED_SYSTEMS)
      await router.replace({ path: '/services', query: { health: 'warning', page: '3' } })
      await flushPromises()
      expect(api().filters.value.health).toBe('warning')
      expect(router.currentRoute.value.query).toEqual({ health: 'warning', page: '3' })
      expect(replaces).toHaveLength(1) // only the test's own navigation
      unmount()
    })
  })

  // ─────────── finding 1: a genuine navigation beats a pending debounce ───────────

  describe('a later navigation wins over a pending debounce', () => {
    it('Back during the debounce window wins and the debounce never resurrects', async () => {
      vi.useFakeTimers()
      const { api, router, replaces, unmount } = await setup('/services', LOADED_SYSTEMS)
      api().patchFilters({ health: 'critical' })
      await flushPromises()
      expect(router.currentRoute.value.fullPath).toBe('/services?health=critical')

      // Typing: still only in the controls, not in the URL.
      api().patchFilters({ search: 'api' })
      expect(replaces).toHaveLength(0)
      expect(api().filters.value.search).toBe('api')

      // A real Back navigation lands mid-debounce. It is newer, so it wins.
      router.back()
      await flushPromises()

      expect(router.currentRoute.value.fullPath).toBe('/services')
      expect(api().filters.value).toEqual({ ...DEFAULT_SERVICE_FILTERS })

      // The cancelled debounce must not re-apply the abandoned search.
      vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS * 4)
      await flushPromises()
      expect(router.currentRoute.value.fullPath).toBe('/services')
      expect(api().filters.value.search).toBe('')
      expect(replaces).toHaveLength(0)
      unmount()
    })

    it('Forward during the debounce window wins too', async () => {
      vi.useFakeTimers()
      const { api, router, unmount } = await setup('/services', LOADED_SYSTEMS)
      api().patchFilters({ runtime: 'eks' })
      await flushPromises()
      router.back()
      await flushPromises()
      expect(api().filters.value.runtime).toBe('all')

      api().patchFilters({ search: 'redis' })
      router.forward()
      await flushPromises()

      expect(router.currentRoute.value.fullPath).toBe('/services?runtime=eks')
      expect(api().filters.value).toMatchObject({ runtime: 'eks', search: '' })

      vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS * 4)
      await flushPromises()
      expect(router.currentRoute.value.fullPath).toBe('/services?runtime=eks')
      unmount()
    })

    it('an external push during the debounce window wins', async () => {
      vi.useFakeTimers()
      const { api, router, unmount } = await setup('/services', LOADED_SYSTEMS)
      api().patchFilters({ search: 'api' })
      void router.push({ path: '/services', query: { health: 'critical' } })
      await flushPromises()

      // The navigation is later than the typing, so it decides both sides.
      expect(api().filters.value).toMatchObject({ health: 'critical', search: '' })
      expect(router.currentRoute.value.query).toEqual({ health: 'critical' })

      vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS * 4)
      await flushPromises()
      expect(router.currentRoute.value.query).toEqual({ health: 'critical' })
      unmount()
    })

    it('but the commit of an older write of ours never clobbers a newer selection', async () => {
      const stuck = gate()
      const { api, router, unmount } = await setup('/services', LOADED_SYSTEMS)
      // Park the first filter write inside a guard.
      const stopGuard = router.beforeEach(async (to) => {
        if (to.query.health === 'critical') await stuck.promise
      })

      api().patchFilters({ health: 'critical' })
      await flushPromises()
      expect(router.currentRoute.value.query).toEqual({})

      // A newer selection arrives while the first write is still parked.
      api().patchFilters({ health: 'warning' })
      await flushPromises()

      stuck.open()
      await flushPromises()
      stopGuard()

      // The newer selection owns both the controls and the URL.
      expect(api().filters.value.health).toBe('warning')
      expect(router.currentRoute.value.query).toEqual({ health: 'warning' })
      unmount()
    })
  })

  // ─────────── finding 2: resolved navigation failures are reconciled ───────────

  describe('reconciling refused writes', () => {
    it('recovers a write cancelled by an unrelated navigation, keeping that navigation', async () => {
      const stuck = gate()
      const { api, router, unmount } = await setup('/services?tab=grid', LOADED_SYSTEMS)
      const stopGuard = router.beforeEach(async (to) => {
        if (to.query.health) await stuck.promise
      })

      api().patchFilters({ health: 'critical' })
      await flushPromises()
      expect(router.currentRoute.value.query).toEqual({ tab: 'grid' })

      // An unrelated navigation on the same page cancels the filter write.
      await router.replace({ path: '/services', query: { tab: 'grid' }, hash: '#pay' })
      expect(router.currentRoute.value.hash).toBe('#pay')

      stuck.open()
      await flushPromises()
      await flushPromises()
      stopGuard()

      // Selection recovered, and the navigation that cancelled it survived.
      expect(api().filters.value.health).toBe('critical')
      expect(router.currentRoute.value.query).toEqual({ tab: 'grid', health: 'critical' })
      expect(router.currentRoute.value.hash).toBe('#pay')
      unmount()
    })

    it('retries a refused write once and then commits', async () => {
      let refusals = 1
      const { api, router, replaces, unmount } = await setup('/services', LOADED_SYSTEMS)
      const stopGuard = router.beforeEach((to) => {
        if (to.query.health && refusals > 0) {
          refusals--
          return false
        }
        return true
      })

      api().patchFilters({ health: 'critical' })
      await flushPromises()
      await flushPromises()
      stopGuard()

      expect(api().filters.value.health).toBe('critical')
      expect(router.currentRoute.value.query).toEqual({ health: 'critical' })
      // One push (refused) + one reconciling replace.
      expect(replaces).toEqual(['/services?health=critical'])
      unmount()
    })

    it('adopts the URL when the write cannot land, never diverging silently', async () => {
      const { api, router, unmount } = await setup('/services', {
        ...LOADED_SYSTEMS,
        maxWriteRetries: 0,
      })
      const stopGuard = router.beforeEach((to) => {
        if (to.query.health) return false
        return true
      })

      api().patchFilters({ health: 'critical' })
      await flushPromises()
      await flushPromises()
      stopGuard()

      // Controls fall back to what the URL actually says.
      expect(router.currentRoute.value.query).toEqual({})
      expect(api().filters.value.health).toBe('all')
      unmount()
    })

    it('a duplicated navigation is not treated as a loss', async () => {
      const { api, router, unmount } = await setup('/services?health=critical', LOADED_SYSTEMS)
      // Same target as the URL already carries.
      const outcome = (await router.push({
        path: '/services',
        query: { health: 'critical' },
      })) as NavigationFailure | undefined
      await flushPromises()
      expect(isNavigationFailure(outcome)).toBe(true)
      expect(api().filters.value.health).toBe('critical')
      expect(router.currentRoute.value.query).toEqual({ health: 'critical' })
      unmount()
    })
  })

  // ─────────── finding 4: pending writes must not cancel a departure ───────────

  describe('leaving the page', () => {
    it('a debounce firing inside a slow leave guard does not cancel the departure', async () => {
      vi.useFakeTimers()
      departureGate = gate()
      const { api, router, replaces, unmount } = await setup('/services', LOADED_SYSTEMS)

      api().patchFilters({ search: 'api' })
      expect(replaces).toHaveLength(0)

      // Departure starts and parks in /catalog's async beforeEnter.
      const departure = router.push('/catalog')
      await flushPromises()
      expect(router.currentRoute.value.path).toBe('/services')
      expect(hostLifecycle).toEqual(['mounted'])

      // The debounce comes due while the departure is still waiting.
      vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS * 4)
      await flushPromises()
      expect(replaces).toHaveLength(0)

      departureGate.open()
      const failure = await departure
      await flushPromises()

      expect(failure).toBeUndefined()
      expect(router.currentRoute.value.path).toBe('/catalog')
      expect(hostLifecycle).toEqual(['mounted', 'unmounted'])
      expect(replaces).toHaveLength(0)
      unmount()
    })

    it('an in-flight write does not survive the departure', async () => {
      departureGate = gate()
      const stuck = gate()
      const { api, router, unmount } = await setup('/services', LOADED_SYSTEMS)
      const stopGuard = router.beforeEach(async (to) => {
        if (to.query.health) await stuck.promise
      })

      api().patchFilters({ health: 'critical' })
      await flushPromises()

      const departure = router.push('/catalog')
      departureGate.open()
      stuck.open()
      await flushPromises()
      const failure = await departure
      await flushPromises()
      stopGuard()

      expect(failure).toBeUndefined()
      expect(router.currentRoute.value.path).toBe('/catalog')
      expect(router.currentRoute.value.query).toEqual({})
      unmount()
    })

    it('an aborted departure leaves the selection in the URL', async () => {
      vi.useFakeTimers()
      const { api, router, unmount } = await setup('/services', LOADED_SYSTEMS)
      const stopGuard = router.beforeEach((to) => {
        if (to.path === '/catalog') return false
        return true
      })

      api().patchFilters({ search: 'api' })
      const departure = router.push('/catalog')
      await flushPromises()
      const failure = (await departure) as NavigationFailure | undefined
      await flushPromises()
      stopGuard()

      expect(isNavigationFailure(failure)).toBe(true)
      expect(router.currentRoute.value.path).toBe('/services')
      // Departure cancelled the debounce, so reconciliation re-asserts it.
      expect(router.currentRoute.value.query).toEqual({ q: 'api' })
      expect(api().filters.value.search).toBe('api')

      vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS * 4)
      await flushPromises()
      expect(router.currentRoute.value.query).toEqual({ q: 'api' })
      unmount()
    })

    it('drops pending writes on unmount', async () => {
      vi.useFakeTimers()
      const { api, replaces, unmount } = await setup('/services', LOADED_SYSTEMS)
      api().patchFilters({ search: 'api' })
      unmount()
      vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS * 3)
      await flushPromises()
      expect(replaces).toHaveLength(0)
    })
  })

  it('exposes a mutable filters ref for v-model style binding', async () => {
    const { api, unmount } = await setup('/services', LOADED_SYSTEMS)
    api().setFilters({ ...DEFAULT_SERVICE_FILTERS, lifecycle: 'retired' })
    await nextTick()
    expect(api().filters.value.lifecycle).toBe('retired')
    unmount()
  })
})
