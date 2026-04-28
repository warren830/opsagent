import { describe, it, expect, vi, beforeEach } from 'vitest'
import { useApi, ApiError } from '~/composables/useApi'

// Mock useAuthStore once — useApi calls it on 401
const refreshAccessToken = vi.fn()
vi.stubGlobal('useAuthStore', () => ({ refreshAccessToken }))

function mockFetch(responses: Array<{ status: number; body?: unknown; ok?: boolean }>) {
  let i = 0
  const fn = vi.fn(async () => {
    const r = responses[i++] ?? responses[responses.length - 1]
    return {
      ok: r.ok ?? (r.status >= 200 && r.status < 300),
      status: r.status,
      statusText: r.status === 200 ? 'OK' : 'Error',
      json: async () => r.body ?? {},
      blob: async () => new Blob([JSON.stringify(r.body ?? {})]),
    } as Response
  })
  vi.stubGlobal('fetch', fn)
  return fn
}

describe('useApi', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    refreshAccessToken.mockReset()
  })

  it('GET returns parsed JSON on 200', async () => {
    mockFetch([{ status: 200, body: { hello: 'world' } }])
    const api = useApi()
    const data = await api.get<{ hello: string }>('/api/test')
    expect(data).toEqual({ hello: 'world' })
  })

  it('POST sends JSON body with Content-Type header', async () => {
    const fetchMock = mockFetch([{ status: 200, body: { ok: true } }])
    const api = useApi()
    await api.post('/api/x', { name: 'alice' })
    const [url, opts] = fetchMock.mock.calls[0]
    expect(url).toContain('/api/x')
    expect(opts.method).toBe('POST')
    expect(opts.body).toBe(JSON.stringify({ name: 'alice' }))
    expect(opts.headers['Content-Type']).toBe('application/json')
  })

  it('PUT and DELETE send correct verbs', async () => {
    const fetchMock = mockFetch([
      { status: 200 },
      { status: 200 },
    ])
    const api = useApi()
    await api.put('/api/x', { a: 1 })
    await api.del('/api/x')
    expect(fetchMock.mock.calls[0][1].method).toBe('PUT')
    expect(fetchMock.mock.calls[1][1].method).toBe('DELETE')
  })

  it('throws ApiError with status on non-200', async () => {
    mockFetch([{ status: 404, body: { error: 'not found' } }])
    const api = useApi()
    await expect(api.get('/api/missing')).rejects.toThrow(ApiError)
    await expect(api.get('/api/missing')).rejects.toMatchObject({ status: 404 })
  })

  it('auto-refreshes on 401 and retries the original request', async () => {
    refreshAccessToken.mockResolvedValue(true)
    // First call → 401, retry → 200
    const fetchMock = mockFetch([
      { status: 401, body: { error: 'unauthorized' } },
      { status: 200, body: { ok: true } },
    ])
    const api = useApi()
    const data = await api.get<{ ok: boolean }>('/api/needs-auth')
    expect(data).toEqual({ ok: true })
    expect(refreshAccessToken).toHaveBeenCalledOnce()
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('does not retry if refresh fails', async () => {
    refreshAccessToken.mockResolvedValue(false)
    const fetchMock = mockFetch([{ status: 401, body: { error: 'unauthorized' } }])
    const api = useApi()
    await expect(api.get('/api/needs-auth')).rejects.toThrow(ApiError)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('skips refresh-retry loop for /api/auth/login and /api/auth/refresh', async () => {
    const fetchMock = mockFetch([{ status: 401 }])
    const api = useApi()
    await expect(api.post('/api/auth/login', { u: 'x' })).rejects.toThrow(ApiError)
    expect(refreshAccessToken).not.toHaveBeenCalled()
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('getBlob returns a Blob', async () => {
    mockFetch([{ status: 200, body: { x: 1 } }])
    const api = useApi()
    const blob = await api.getBlob('/api/file')
    expect(blob).toBeInstanceOf(Blob)
  })

  it('getBlob throws ApiError on failure', async () => {
    mockFetch([{ status: 500 }])
    const api = useApi()
    await expect(api.getBlob('/api/file')).rejects.toThrow(ApiError)
  })
})
