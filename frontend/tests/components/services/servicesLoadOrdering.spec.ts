/**
 * Services page load ordering — the cached-payload trap.
 *
 * `useServicesOverview` keeps its response in *module* state, so it survives
 * navigation away from the page, and `pages/services/index.vue` only starts a
 * fetch in `onMounted`. Vue runs a child's `setup` during the parent's render,
 * i.e. **before** the parent's `onMounted`, so on a repeat visit the grid's
 * setup observes a non-null cached payload with `loading === false` — a state
 * indistinguishable, by those two flags alone, from a completed load.
 *
 * These cases drive that exact ordering through the real router, the real
 * filter composable, the real freshness tracker and the real grid pipeline
 * (`buildServiceGrid`, which is what ServiceCardGrid renders from), asserting
 * the selection, the URL and the displayed group counts at every step.
 *
 * The host is a render-function stand-in for ServiceCardGrid.vue: it wires the
 * same modules in the same order, since mounting the SFC would require
 * `@vitejs/plugin-vue`, which the suite deliberately does not install.
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { computed, defineComponent, h, nextTick, onMounted, ref } from 'vue'
import type { Ref } from 'vue'
import { RouterView, createMemoryHistory, createRouter } from 'vue-router'
import { flushPromises, mount } from '@vue/test-utils'
import { buildServiceGrid, type ServiceGridView } from '~/components/services/cardRegistry'
import {
  usePayloadSeenSinceSetup,
  useServiceFilterQuery,
} from '~/composables/useServiceFilterQuery'
import type { ServiceFilters } from '~/composables/serviceFilterQuery'
import type {
  ComponentOverview,
  ServicesOverviewResponse,
  SystemSummary,
} from '~/composables/useServicesOverview'

// ─────────────────── payload fixtures ───────────────────

function component(id: string, systemId: string): ComponentOverview {
  return {
    id,
    name: id,
    display_name: null,
    description: null,
    lifecycle: 'production',
    system_id: systemId,
    owner_group_id: null,
    tags: [],
    runtime: { kind: 'generic' },
    runtime_detail: {},
    health: 'healthy',
    health_reason: '',
    active_incident_count: 0,
    slo_summary: { total: 0, budget_remaining_min_pct: null, burn_rate_1h_max: null },
  }
}

function system(id: string): SystemSummary {
  return {
    id,
    name: id,
    display_name: null,
    component_count: 1,
    health_summary: { healthy: 1, warning: 0, critical: 0, unknown: 0 },
  }
}

/** A distinct payload object every call, exactly as a real fetch produces. */
function payload(includeNew: boolean): ServicesOverviewResponse {
  return {
    systems: [system('sys-old'), ...(includeNew ? [system('sys-new')] : [])],
    components: [
      component('component-old', 'sys-old'),
      ...(includeNew ? [component('component-new', 'sys-new')] : []),
    ],
  }
}

// ─────────────────── harness ───────────────────

interface Trace {
  phase: string
  systemId: string
  hasData: boolean
  loading: boolean
  knownSystemIds: string[]
  systemsReady: boolean
}

interface Harness {
  router: ReturnType<typeof createRouter>
  trace: Trace[]
  /** Resolves the fetch the page starts in onMounted. */
  deliver: (value: ServicesOverviewResponse) => void
  /** Rejects it, leaving the cached payload in place. */
  fail: () => void
  snapshot: () => {
    url: string
    query: Record<string, unknown>
    systemId: string
    total: number
    visible: string[]
    groups: Array<{ system: string | null, count: number, healthy: number }>
  }
  unmount: () => void
}

/**
 * Mirrors the Services page/grid pair:
 *   parent  — owns module-cached data, starts the fetch in onMounted
 *   child   — reads the URL at setup, then renders the grid pipeline
 */
async function setup(options: {
  initial?: string
  cached: ServicesOverviewResponse | null
  loading?: boolean
}): Promise<Harness> {
  // Predates page setup, like the module-level refs in useServicesOverview.
  const data = ref<ServicesOverviewResponse | null>(options.cached)
  const loading = ref(options.loading ?? false)

  const trace: Trace[] = []
  let filtersRef: Ref<ServiceFilters> | null = null
  let gridRef: Ref<ServiceGridView> | null = null

  let deliver!: (value: ServicesOverviewResponse) => void
  let fail!: () => void
  const fetched = new Promise<ServicesOverviewResponse>((resolve, reject) => {
    deliver = resolve
    fail = () => reject(new Error('overview fetch failed'))
  })

  const Grid = defineComponent({
    name: 'GridHost',
    props: {
      data: { type: Object as () => ServicesOverviewResponse | null, default: null },
      loading: { type: Boolean, default: false },
    },
    setup(props) {
      const allComponents = computed(() => props.data?.components ?? [])
      const allSystems = computed(() => props.data?.systems ?? [])
      const systemsReady = usePayloadSeenSinceSetup(() => props.data)
      const { filters } = useServiceFilterQuery({
        knownSystemIds: computed(() => allSystems.value.map(s => s.id)),
        systemsReady,
      })
      const grid = computed(() =>
        buildServiceGrid(allComponents.value, allSystems.value, filters.value),
      )
      filtersRef = filters
      gridRef = grid

      const record = (phase: string) => trace.push({
        phase,
        systemId: filters.value.systemId,
        hasData: props.data != null,
        loading: props.loading,
        knownSystemIds: allSystems.value.map(s => s.id),
        systemsReady: systemsReady.value,
      })
      record('grid:setup')
      onMounted(() => record('grid:mounted'))
      return () => h('div', String(grid.value.total))
    },
  })

  const Page = defineComponent({
    name: 'PageHost',
    setup() {
      onMounted(() => {
        trace.push({
          phase: 'page:fetch-started',
          systemId: filtersRef?.value.systemId ?? '',
          hasData: data.value != null,
          loading: loading.value,
          knownSystemIds: data.value?.systems.map(s => s.id) ?? [],
          systemsReady: false,
        })
        loading.value = true
        void fetched
          .then((value) => {
            data.value = value
            loading.value = false
          })
          .catch(() => {
            // Failure leaves the cached payload untouched, as fetchOnce does.
            loading.value = false
          })
      })
      return () => h(Grid, { data: data.value, loading: loading.value })
    },
  })

  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/services', name: 'services', component: Page },
      { path: '/:pathMatch(.*)*', name: 'catch-all', component: { render: () => h('div') } },
    ],
  })

  await router.push(options.initial ?? '/services?system=sys-new&tab=grid#services')
  await router.isReady()

  const wrapper = mount(defineComponent({ render: () => h(RouterView) }), {
    global: { plugins: [router] },
  })
  await flushPromises()

  return {
    router,
    trace,
    deliver,
    fail: () => {
      fail()
    },
    snapshot: () => ({
      url: router.currentRoute.value.fullPath,
      query: { ...router.currentRoute.value.query },
      systemId: filtersRef!.value.systemId,
      total: gridRef!.value.total,
      visible: gridRef!.value.visible.map(c => c.id),
      groups: gridRef!.value.groups.map(g => ({
        system: g.system.id,
        count: g.components.length,
        healthy: g.healthSummary.healthy,
      })),
    }),
    unmount: () => wrapper.unmount(),
  }
}

async function settle() {
  await flushPromises()
  await nextTick()
  await flushPromises()
}

describe('Services load ordering: cached payload at setup', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('keeps a URL system id that only the fresh response can confirm', async () => {
    const host = await setup({ cached: payload(false) })

    // Ordering guard: the grid's setup ran before the page started its fetch,
    // and it saw a non-null cached payload that does not know `sys-new`.
    const setupTrace = host.trace.find(t => t.phase === 'grid:setup')!
    expect(setupTrace.hasData).toBe(true)
    expect(setupTrace.loading).toBe(false)
    expect(setupTrace.knownSystemIds).toEqual(['sys-old'])
    expect(setupTrace.systemsReady).toBe(false)
    expect(host.trace.findIndex(t => t.phase === 'grid:setup'))
      .toBeLessThan(host.trace.findIndex(t => t.phase === 'page:fetch-started'))

    // The cached list must not have been treated as authority.
    const duringFetch = host.snapshot()
    expect(duringFetch.systemId).toBe('sys-new')
    expect(duringFetch.query.system).toBe('sys-new')
    expect(duringFetch.query.tab).toBe('grid')
    expect(duringFetch.url).toContain('#services')
    // Nothing matches the unknown system yet, so the grid is legitimately empty.
    expect(duringFetch.total).toBe(0)
    expect(duringFetch.groups).toEqual([])

    // The fresh response confirms the system.
    host.deliver(payload(true))
    await settle()

    const afterFresh = host.snapshot()
    expect(afterFresh.systemId).toBe('sys-new')
    expect(afterFresh.query.system).toBe('sys-new')
    expect(afterFresh.query.tab).toBe('grid')
    expect(afterFresh.url).toContain('#services')
    // Results and group counts stay scoped to the requested system.
    expect(afterFresh.visible).toEqual(['component-new'])
    expect(afterFresh.total).toBe(1)
    expect(afterFresh.groups).toEqual([{ system: 'sys-new', count: 1, healthy: 1 }])
    host.unmount()
  })

  it('an authoritative response still clears a system it does not know', async () => {
    const host = await setup({
      cached: payload(false),
      initial: '/services?system=sys-deleted',
    })
    expect(host.snapshot().systemId).toBe('sys-deleted')

    host.deliver(payload(true))
    await settle()

    const after = host.snapshot()
    expect(after.systemId).toBe('all')
    expect(after.query.system).toBeUndefined()
    expect(after.visible).toEqual(['component-new', 'component-old'])
    expect(after.total).toBe(2)
    expect(after.groups).toEqual([
      { system: 'sys-old', count: 1, healthy: 1 },
      { system: 'sys-new', count: 1, healthy: 1 },
    ])
    host.unmount()
  })

  it('a failed refresh never masquerades as confirmation', async () => {
    const host = await setup({ cached: payload(false) })
    expect(host.snapshot().systemId).toBe('sys-new')

    host.fail()
    await settle()

    const after = host.snapshot()
    expect(after.systemId).toBe('sys-new')
    expect(after.query.system).toBe('sys-new')
    host.unmount()
  })

  it('control: a cached payload already loading keeps the selection', async () => {
    const host = await setup({ cached: payload(false), loading: true })
    expect(host.snapshot().systemId).toBe('sys-new')

    host.deliver(payload(true))
    await settle()

    const after = host.snapshot()
    expect(after.systemId).toBe('sys-new')
    expect(after.query.system).toBe('sys-new')
    expect(after.visible).toEqual(['component-new'])
    host.unmount()
  })

  it('control: a cold first load keeps the selection', async () => {
    const host = await setup({ cached: null })
    const setupTrace = host.trace.find(t => t.phase === 'grid:setup')!
    expect(setupTrace.hasData).toBe(false)
    expect(host.snapshot().systemId).toBe('sys-new')

    host.deliver(payload(true))
    await settle()

    const after = host.snapshot()
    expect(after.systemId).toBe('sys-new')
    expect(after.query.system).toBe('sys-new')
    expect(after.visible).toEqual(['component-new'])
    expect(after.groups).toEqual([{ system: 'sys-new', count: 1, healthy: 1 }])
    host.unmount()
  })

  it('a system picked from the UI survives the next poll that confirms it', async () => {
    const host = await setup({ cached: payload(false), initial: '/services' })
    host.deliver(payload(true))
    await settle()

    // Poll N+1 is in flight when the user picks a system.
    host.router.push({ path: '/services', query: { system: 'sys-new' } })
    await settle()
    expect(host.snapshot().systemId).toBe('sys-new')

    // A later completed poll that still knows the system leaves it alone.
    const after = host.snapshot()
    expect(after.query.system).toBe('sys-new')
    expect(after.visible).toEqual(['component-new'])
    host.unmount()
  })
})
