import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { ApiError } from '~/composables/useApi'
import { useServicesOverview } from '~/composables/useServicesOverview'

// Nuxt auto-imports are stubbed per-spec. useApi is the only thing the
// composable reaches for besides Vue primitives.
const getMock = vi.fn()
vi.stubGlobal('useApi', () => ({ get: getMock }))

describe('useServicesOverview', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('uses /api/services/overview when available', async () => {
    const payload = {
      systems: [],
      components: [{
        id: '1', name: 'x', display_name: null, description: null,
        lifecycle: 'production', system_id: null, owner_group_id: null,
        tags: [], runtime: null, runtime_detail: {}, health: 'healthy',
        health_reason: '', active_incident_count: 0,
        slo_summary: { total: 0, budget_remaining_min_pct: null, burn_rate_1h_max: null },
      }],
    }
    getMock.mockResolvedValueOnce(payload)
    const { fetchOnce, data, source } = useServicesOverview()
    await fetchOnce()
    expect(getMock).toHaveBeenCalledWith('/api/services/overview')
    expect(source.value).toBe('overview')
    expect(data.value?.components[0].id).toBe('1')
  })

  it('falls back to catalog+slo+incidents when overview returns 404', async () => {
    getMock
      // /api/services/overview
      .mockRejectedValueOnce(new ApiError(404, 'not found'))
      // fallback: /api/catalog/entities
      .mockResolvedValueOnce([
        {
          id: 'sys-a', kind: 'system', name: 'payments', display_name: 'Payments',
          description: null, lifecycle: 'production', owner_group_id: null, system_id: null,
          tags: [], spec: {}, updated_at: '',
        },
        {
          id: 'c1', kind: 'component', name: 'order-api', display_name: 'Order API',
          description: 'desc', lifecycle: 'production', owner_group_id: null, system_id: 'sys-a',
          tags: ['rust'],
          spec: { runtime: { kind: 'eks', cluster: 'prod' } },
          updated_at: '',
        },
      ])
      // fallback: /api/slos
      .mockResolvedValueOnce([{ id: 's1', component_id: 'c1', enabled: true }])
      // fallback: /api/incidents/active
      .mockResolvedValueOnce([])
      // fallback: /api/slos/budgets
      .mockResolvedValueOnce([{ slo_id: 's1', budget_remaining_pct: 90, burn_rate_1h: 0.1, burn_rate_6h: null }])

    const { fetchOnce, data, source } = useServicesOverview()
    await fetchOnce()
    expect(source.value).toBe('fallback')
    expect(data.value?.components).toHaveLength(1)
    expect(data.value?.components[0].runtime?.kind).toBe('eks')
    expect(data.value?.components[0].health).toBe('healthy')
    expect(data.value?.systems).toHaveLength(1)
    expect(data.value?.systems[0].health_summary.healthy).toBe(1)
  })

  it('reports error when neither overview nor catalog is reachable', async () => {
    getMock
      .mockRejectedValueOnce(new ApiError(500, 'internal'))
      .mockRejectedValueOnce(new Error('boom'))
    const { fetchOnce, error } = useServicesOverview()
    await fetchOnce()
    expect(error.value).toBeTruthy()
  })
})
