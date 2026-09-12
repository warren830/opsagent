/**
 * Services load ordering — driven through the real ServiceCardGrid SFC.
 *
 * Two independent staleness windows exist around the grid's catalog:
 *
 *  1. Setup. `useServicesOverview` caches its response in module state, so it
 *     survives navigation, and `pages/services/index.vue` only starts a fetch in
 *     `onMounted`. Vue runs a child's `setup` during the parent's render, which
 *     is ahead of the parent's `onMounted`, so on a repeat visit the grid begins
 *     life looking at an earlier visit's payload with nothing loading.
 *  2. Every refresh. Polling reopens the gap: while a refresh is in flight
 *     the list on hand predates the response being fetched, so a system created
 *     since the last completed poll looks nonexistent.
 *
 * The grid itself is mounted (not re-implemented) so the wiring between
 * `usePayloadAuthority`, `useServiceFilterQuery` and `buildServiceGrid` is what
 * gets tested. Only the three presentational children are stubbed, to keep the
 * UI kit and i18n out of the way; they surface the rendered selection, total and
 * per-group counts.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { defineComponent, h, nextTick, onMounted, ref } from 'vue'
import { RouterView, createMemoryHistory, createRouter } from 'vue-router'
import { flushPromises, mount } from '@vue/test-utils'
import ServiceCardGrid from '~/components/services/ServiceCardGrid.vue'
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
function payload(...systemIds: string[]): ServicesOverviewResponse {
  return {
    systems: systemIds.map(system),
    components: systemIds.map(id => component(`component-${id}`, id)),
  }
}

// ─────────────────── child stubs (presentation only) ───────────────────

const traces: Array<{ phase: string, systemId: string, hasData: boolean, loading: boolean }> = []

const FilterBarStub = defineComponent({
  name: 'ServiceFilterBar',
  props: ['filters', 'systems', 'total'],
  setup(props) {
    traces.push({
      phase: 'filter-bar:setup',
      systemId: props.filters.systemId,
      hasData: props.systems.length > 0,
      loading: false,
    })
    return () => h('div', [
      h('output', { 'data-testid': 'system-selection' }, props.filters.systemId),
      h('output', { 'data-testid': 'visible-total' }, String(props.total)),
    ])
  },
})

const CardStub = defineComponent({
  name: 'ServiceCard',
  props: ['component'],
  setup: props => () => h('article', {
    'data-testid': 'service-card',
    'data-component-id': props.component.id,
  }),
})

const GroupHeaderStub = defineComponent({
  name: 'SystemGroupHeader',
  props: ['system', 'componentCount', 'healthSummary', 'collapsed'],
  setup: props => () => h('header', {
    'data-testid': 'group-header',
    'data-system': props.system.id ?? '__ungrouped',
    'data-count': String(props.componentCount),
    'data-healthy': String(props.healthSummary.healthy),
  }),
})

// ─────────────────── harness ───────────────────

interface Harness {
  router: ReturnType<typeof createRouter>
  /** Marks a refresh as started, like fetchOnce does before awaiting. */
  startRefresh: () => Promise<void>
  /** Completes the current refresh with a new payload. */
  finishRefresh: (value: ServicesOverviewResponse) => Promise<void>
  /** Completes it without a payload, as a failed fetch does. */
  failRefresh: () => Promise<void>
  snapshot: () => {
    query: Record<string, unknown>
    hash: string
    systemId: string
    total: number
    cards: string[]
    groups: Array<{ system: string, count: number, healthy: number }>
  }
  unmount: () => void
}

async function setup(options: {
  initial?: string
  cached?: ServicesOverviewResponse | null
  /** Payload the page's own onMounted fetch resolves with, if any. */
  firstResponse?: ServicesOverviewResponse
}): Promise<Harness> {
  // These refs predate page setup, like the module-level refs in
  // useServicesOverview that survive navigating away and back.
  const data = ref<ServicesOverviewResponse | null>(options.cached ?? null)
  const loading = ref(false)

  let firstFetch!: () => void
  const firstFetchGate = new Promise<void>((resolve) => {
    firstFetch = resolve
  })

  const Page = defineComponent({
    name: 'ServicesPageHost',
    setup() {
      onMounted(() => {
        // pages/services/index.vue starts polling here — after the grid's setup.
        loading.value = true
        void firstFetchGate.then(() => {
          if (options.firstResponse) data.value = options.firstResponse
          loading.value = false
        })
      })
      return () => h(ServiceCardGrid, { data: data.value, loading: loading.value })
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
    global: {
      plugins: [router],
      stubs: {
        ServiceFilterBar: FilterBarStub,
        ServiceCard: CardStub,
        SystemGroupHeader: GroupHeaderStub,
      },
    },
  })
  await settle()
  // Release the page's initial fetch so tests start from a settled state.
  firstFetch()
  await settle()

  return {
    router,
    startRefresh: async () => {
      loading.value = true
      await settle()
    },
    finishRefresh: async (value) => {
      data.value = value
      loading.value = false
      await settle()
    },
    failRefresh: async () => {
      // fetchOnce's failure path: the previous payload stays, loading clears.
      loading.value = false
      await settle()
    },
    snapshot: () => ({
      query: { ...router.currentRoute.value.query },
      hash: router.currentRoute.value.hash,
      systemId: wrapper.get('[data-testid="system-selection"]').text(),
      total: Number(wrapper.get('[data-testid="visible-total"]').text()),
      cards: wrapper.findAll('[data-testid="service-card"]')
        .map(node => node.attributes('data-component-id') as string),
      groups: wrapper.findAll('[data-testid="group-header"]').map(node => ({
        system: node.attributes('data-system') as string,
        count: Number(node.attributes('data-count')),
        healthy: Number(node.attributes('data-healthy')),
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

beforeEach(() => {
  traces.length = 0
  window.localStorage.clear()
  vi.stubGlobal('useI18n', () => ({ t: (key: string) => key }))
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('ServiceCardGrid load ordering (real component)', () => {
  it('keeps a URL system id that only the first fresh response can confirm', async () => {
    const host = await setup({
      cached: payload('sys-old'),
      firstResponse: payload('sys-old', 'sys-new'),
    })

    // Ordering guard: the grid rendered before the page's fetch could answer,
    // against a catalog that did not know `sys-new`.
    expect(traces[0]).toMatchObject({ phase: 'filter-bar:setup', systemId: 'sys-new' })

    const after = host.snapshot()
    expect(after.systemId).toBe('sys-new')
    expect(after.query.system).toBe('sys-new')
    expect(after.query.tab).toBe('grid')
    expect(after.hash).toBe('#services')
    expect(after.cards).toEqual(['component-sys-new'])
    expect(after.total).toBe(1)
    expect(after.groups).toEqual([{ system: 'sys-new', count: 1, healthy: 1 }])
    host.unmount()
  })

  it('a later refresh gets its own boundary: selection made while loading survives, failure keeps it, fresh data confirms it', async () => {
    const host = await setup({
      initial: '/services?tab=grid#services',
      cached: payload('sys-old'),
      firstResponse: payload('sys-old'),
    })

    // The first response has landed, so the catalog is authoritative here.
    expect(host.snapshot().systemId).toBe('all')
    expect(host.snapshot().cards).toEqual(['component-sys-old'])

    // A background poll starts: the list on hand now predates its response.
    await host.startRefresh()

    // The user navigates to a system that only the incoming payload knows.
    await host.router.push({
      path: '/services',
      query: { tab: 'grid', system: 'sys-fresh' },
      hash: '#services',
    })
    await settle()

    const duringRefresh = host.snapshot()
    expect(duringRefresh.systemId).toBe('sys-fresh')
    expect(duringRefresh.query.system).toBe('sys-fresh')
    expect(duringRefresh.query.tab).toBe('grid')
    expect(duringRefresh.hash).toBe('#services')

    // That refresh fails, leaving the old payload in place. It must not be
    // mistaken for confirmation that `sys-fresh` does not exist.
    await host.failRefresh()

    const afterFailure = host.snapshot()
    expect(afterFailure.systemId).toBe('sys-fresh')
    expect(afterFailure.query.system).toBe('sys-fresh')
    expect(afterFailure.query.tab).toBe('grid')
    expect(afterFailure.hash).toBe('#services')

    // The next poll succeeds and confirms the system.
    await host.startRefresh()
    await host.finishRefresh(payload('sys-old', 'sys-fresh'))

    const afterFresh = host.snapshot()
    expect(afterFresh.systemId).toBe('sys-fresh')
    expect(afterFresh.query.system).toBe('sys-fresh')
    expect(afterFresh.cards).toEqual(['component-sys-fresh'])
    expect(afterFresh.total).toBe(1)
    expect(afterFresh.groups).toEqual([{ system: 'sys-fresh', count: 1, healthy: 1 }])
    host.unmount()
  })

  it('a completed refresh still clears a system it does not know', async () => {
    const host = await setup({
      initial: '/services',
      cached: payload('sys-old'),
      firstResponse: payload('sys-old'),
    })

    await host.startRefresh()
    await host.router.push({ path: '/services', query: { system: 'sys-deleted' } })
    await settle()
    expect(host.snapshot().systemId).toBe('sys-deleted')

    await host.finishRefresh(payload('sys-old', 'sys-other'))

    const after = host.snapshot()
    expect(after.systemId).toBe('all')
    expect(after.query.system).toBeUndefined()
    expect(after.total).toBe(2)
    expect(after.groups).toEqual([
      { system: 'sys-old', count: 1, healthy: 1 },
      { system: 'sys-other', count: 1, healthy: 1 },
    ])
    host.unmount()
  })

  it('the initial fetch failing keeps the URL selection', async () => {
    const host = await setup({ cached: payload('sys-old') })

    const after = host.snapshot()
    expect(after.systemId).toBe('sys-new')
    expect(after.query.system).toBe('sys-new')
    // Nothing matches an unconfirmed system yet, so the grid is legitimately empty.
    expect(after.total).toBe(0)
    expect(after.groups).toEqual([])
    host.unmount()
  })

  it('control: a cold first load keeps the selection', async () => {
    const host = await setup({
      cached: null,
      firstResponse: payload('sys-old', 'sys-new'),
    })

    const after = host.snapshot()
    expect(after.systemId).toBe('sys-new')
    expect(after.query.system).toBe('sys-new')
    expect(after.cards).toEqual(['component-sys-new'])
    expect(after.groups).toEqual([{ system: 'sys-new', count: 1, healthy: 1 }])
    host.unmount()
  })

  it('an unrelated filter still writes through during a refresh', async () => {
    const host = await setup({
      initial: '/services?tab=grid',
      cached: payload('sys-old'),
      firstResponse: payload('sys-old'),
    })
    await host.startRefresh()

    await host.router.push({ path: '/services', query: { tab: 'grid', runtime: 'eks' } })
    await settle()

    const after = host.snapshot()
    expect(after.query.runtime).toBe('eks')
    expect(after.query.tab).toBe('grid')
    // sys-old's component is generic, so an eks filter empties the grid.
    expect(after.total).toBe(0)
    host.unmount()
  })
})
