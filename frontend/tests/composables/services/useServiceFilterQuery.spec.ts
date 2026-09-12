/**
 * useServiceFilterQuery — route ⇄ filter-state sync for the Services page.
 *
 * Driven through a real vue-router instance on memory history so the assertions
 * cover genuine navigations (push vs replace, back/forward, external rewrites)
 * rather than a mocked router surface. The host component is a render-function
 * stub, which keeps the suite free of `@vitejs/plugin-vue`.
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { defineComponent, h, nextTick, ref } from 'vue'
import { createMemoryHistory, createRouter, type Router } from 'vue-router'
import { flushPromises, mount } from '@vue/test-utils'
import {
  SEARCH_QUERY_DEBOUNCE_MS,
  useServiceFilterQuery,
  type UseServiceFilterQueryOptions,
  type UseServiceFilterQueryReturn,
} from '~/composables/useServiceFilterQuery'
import { DEFAULT_SERVICE_FILTERS } from '~/composables/serviceFilterQuery'

const Blank = defineComponent({ render: () => h('div') })

function makeRouter(): Router {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/services', name: 'services', component: Blank },
      { path: '/:pathMatch(.*)*', name: 'catch-all', component: Blank },
    ],
  })
}

interface Harness {
  router: Router
  api: UseServiceFilterQueryReturn
  navigations: string[]
  pushes: string[]
  replaces: string[]
  unmount: () => void
}

async function setup(
  initial = '/services',
  options: UseServiceFilterQueryOptions = {},
): Promise<Harness> {
  const router = makeRouter()
  await router.push(initial)
  await router.isReady()

  const navigations: string[] = []
  router.afterEach(to => void navigations.push(to.fullPath))

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

  let api: UseServiceFilterQueryReturn | null = null
  const wrapper = mount(
    defineComponent({
      setup() {
        api = useServiceFilterQuery(options)
        return () => h('div')
      },
    }),
    { global: { plugins: [router] } },
  )
  await flushPromises()

  return {
    router,
    api: api as unknown as UseServiceFilterQueryReturn,
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
    vi.restoreAllMocks()
    vi.useRealTimers()
  })

  describe('restore on first load', () => {
    it('hydrates every filter from the URL', async () => {
      const { api, unmount } = await setup(
        '/services?q=order&system=sys-o&lifecycle=experimental&runtime=lambda&health=critical&sort=incidents',
        LOADED_SYSTEMS,
      )
      expect(api.filters.value).toEqual({
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
      expect(api.filters.value).toEqual({ ...DEFAULT_SERVICE_FILTERS })
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
      expect(api.filters.value.health).toBe('all')
      expect(api.filters.value.sort).toBe('health')
      expect(api.filters.value.runtime).toBe('eks')
      expect(router.currentRoute.value.query).toEqual({ runtime: 'eks' })
      expect(replaces).toHaveLength(1)
      expect(pushes).toHaveLength(0)
      unmount()
    })

    it('keeps a system id that cannot be validated yet, then drops it when data lands', async () => {
      const ready = ref(false)
      const ids = ref<string[]>([])
      const { api, router, replaces, unmount } = await setup('/services?system=sys-x', {
        knownSystemIds: ids,
        systemsReady: ready,
      })
      expect(api.filters.value.systemId).toBe('sys-x')
      expect(router.currentRoute.value.query).toEqual({ system: 'sys-x' })

      ids.value = ['sys-p']
      ready.value = true
      await flushPromises()

      expect(api.filters.value.systemId).toBe('all')
      expect(router.currentRoute.value.query).toEqual({})
      expect(replaces).toHaveLength(1)
      unmount()
    })

    it('keeps a system id that the loaded data confirms', async () => {
      const ready = ref(false)
      const ids = ref<string[]>([])
      const { api, unmount } = await setup('/services?system=sys-p', {
        knownSystemIds: ids,
        systemsReady: ready,
      })
      ids.value = ['sys-p', 'sys-o']
      ready.value = true
      await flushPromises()
      expect(api.filters.value.systemId).toBe('sys-p')
      unmount()
    })
  })

  describe('writing local changes', () => {
    it('pushes discrete selections and omits defaults', async () => {
      const { api, router, pushes, unmount } = await setup('/services', LOADED_SYSTEMS)
      api.patchFilters({ health: 'critical' })
      await flushPromises()
      expect(router.currentRoute.value.fullPath).toBe('/services?health=critical')
      expect(pushes).toEqual(['/services?health=critical'])

      api.patchFilters({ health: 'all' })
      await flushPromises()
      expect(router.currentRoute.value.fullPath).toBe('/services')
      unmount()
    })

    it('debounces search into a single replace, never a push', async () => {
      vi.useFakeTimers()
      const { api, router, pushes, replaces, unmount } = await setup('/services', LOADED_SYSTEMS)
      for (const text of ['o', 'or', 'ord', 'orde', 'order']) {
        api.patchFilters({ search: text })
        vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS / 3)
      }
      expect(replaces).toHaveLength(0)

      vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS)
      await flushPromises()

      expect(replaces).toEqual(['/services?q=order'])
      expect(pushes).toHaveLength(0)
      expect(router.currentRoute.value.fullPath).toBe('/services?q=order')
      expect(api.filters.value.search).toBe('order')
      unmount()
    })

    it('a selection flushes pending search text in one push', async () => {
      vi.useFakeTimers()
      const { api, router, pushes, replaces, unmount } = await setup('/services', LOADED_SYSTEMS)
      api.patchFilters({ search: 'api' })
      api.patchFilters({ health: 'warning' })
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
      api.patchFilters({ health: 'critical' })
      await flushPromises()
      expect(navigations).toEqual([])
      unmount()
    })

    it('resetFilters clears every owned param', async () => {
      const { api, router, unmount } = await setup(
        '/services?q=api&health=critical&sort=name',
        LOADED_SYSTEMS,
      )
      api.resetFilters()
      await flushPromises()
      expect(router.currentRoute.value.fullPath).toBe('/services')
      expect(api.filters.value).toEqual({ ...DEFAULT_SERVICE_FILTERS })
      unmount()
    })

    it('preserves unrelated params and the hash', async () => {
      const { api, router, unmount } = await setup(
        '/services?tab=grid&ref=slack#pay',
        LOADED_SYSTEMS,
      )
      api.patchFilters({ runtime: 'rds' })
      await flushPromises()
      expect(router.currentRoute.value.query).toEqual({ tab: 'grid', ref: 'slack', runtime: 'rds' })
      expect(router.currentRoute.value.hash).toBe('#pay')

      api.patchFilters({ runtime: 'all' })
      await flushPromises()
      expect(router.currentRoute.value.query).toEqual({ tab: 'grid', ref: 'slack' })
      expect(router.currentRoute.value.hash).toBe('#pay')
      unmount()
    })
  })

  describe('reacting to route changes', () => {
    it('back and forward move the controls', async () => {
      const { api, router, unmount } = await setup('/services', LOADED_SYSTEMS)
      api.patchFilters({ health: 'critical' })
      await flushPromises()
      api.patchFilters({ sort: 'name' })
      await flushPromises()
      expect(api.filters.value).toMatchObject({ health: 'critical', sort: 'name' })

      router.back()
      await flushPromises()
      expect(api.filters.value).toMatchObject({ health: 'critical', sort: 'health' })

      router.back()
      await flushPromises()
      expect(api.filters.value).toEqual({ ...DEFAULT_SERVICE_FILTERS })

      router.forward()
      await flushPromises()
      expect(api.filters.value).toMatchObject({ health: 'critical', sort: 'health' })
      unmount()
    })

    it('an externally rewritten query updates the controls without looping', async () => {
      const { api, router, navigations, unmount } = await setup('/services', LOADED_SYSTEMS)
      await router.replace({ path: '/services', query: { q: 'redis', system: 'sys-p', sort: 'name' } })
      await flushPromises()

      expect(api.filters.value).toMatchObject({ search: 'redis', systemId: 'sys-p', sort: 'name' })
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

      expect(api.filters.value).toMatchObject({ health: 'all', sort: 'health', runtime: 'eks' })
      expect(router.currentRoute.value.query).toEqual({ runtime: 'eks', tab: 'grid' })
      // One external navigation + exactly one canonicalising replace.
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

    it('a route update cannot overwrite a newer filter selection', async () => {
      vi.useFakeTimers()
      const { api, router, unmount } = await setup('/services', LOADED_SYSTEMS)

      // Newer local intent, still inside the search debounce window.
      api.patchFilters({ search: 'api' })
      // A stale route update lands first.
      void router.replace({ path: '/services', query: { health: 'critical' } })
      await flushPromises()

      expect(api.filters.value.search).toBe('api')
      expect(api.filters.value.health).toBe('all')
      expect(router.currentRoute.value.query).toEqual({ q: 'api' })

      vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS * 2)
      await flushPromises()
      expect(router.currentRoute.value.query).toEqual({ q: 'api' })
      unmount()
    })

    it('ignores unrelated param churn', async () => {
      const { api, router, unmount } = await setup('/services?health=warning', LOADED_SYSTEMS)
      await router.replace({ path: '/services', query: { health: 'warning', page: '3' } })
      await flushPromises()
      expect(api.filters.value.health).toBe('warning')
      expect(router.currentRoute.value.query).toEqual({ health: 'warning', page: '3' })
      unmount()
    })
  })

  it('drops pending writes on unmount', async () => {
    vi.useFakeTimers()
    const { api, replaces, unmount } = await setup('/services', LOADED_SYSTEMS)
    api.patchFilters({ search: 'api' })
    unmount()
    vi.advanceTimersByTime(SEARCH_QUERY_DEBOUNCE_MS * 3)
    await flushPromises()
    expect(replaces).toHaveLength(0)
  })

  it('exposes a mutable filters ref for v-model style binding', async () => {
    const { api, unmount } = await setup('/services', LOADED_SYSTEMS)
    api.setFilters({ ...DEFAULT_SERVICE_FILTERS, lifecycle: 'retired' })
    await nextTick()
    expect(api.filters.value.lifecycle).toBe('retired')
    unmount()
  })
})
