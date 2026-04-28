import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useAuthStore } from '~/stores/auth'

const apiGet = vi.fn()
const apiPost = vi.fn()
vi.stubGlobal('useApi', () => ({ get: apiGet, post: apiPost, put: vi.fn(), del: vi.fn() }))

describe('useAuthStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    apiGet.mockReset()
    apiPost.mockReset()
  })

  const sampleUser = {
    id: '00000000-0000-0000-0000-000000000001',
    username: 'alice',
    role: 'super_admin',
    tenant_id: null,
    email: 'alice@example.com',
    auth_method: 'local',
  }

  it('isSuperAdmin getter is true for super_admin role', () => {
    const store = useAuthStore()
    store.user = { ...sampleUser, role: 'super_admin' }
    expect(store.isSuperAdmin).toBe(true)
  })

  it('isSuperAdmin getter is false for member role', () => {
    const store = useAuthStore()
    store.user = { ...sampleUser, role: 'member' as const }
    expect(store.isSuperAdmin).toBe(false)
  })

  it('tenantId getter mirrors user.tenant_id', () => {
    const store = useAuthStore()
    store.user = { ...sampleUser, tenant_id: 'tenant-42' }
    expect(store.tenantId).toBe('tenant-42')
  })

  it('fetchMe populates user and sets isAuthenticated on success', async () => {
    apiGet.mockResolvedValue(sampleUser)
    const store = useAuthStore()
    await store.fetchMe()
    expect(store.user).toEqual(sampleUser)
    expect(store.isAuthenticated).toBe(true)
    expect(store.isLoading).toBe(false)
  })

  it('fetchMe clears auth state on error', async () => {
    apiGet.mockRejectedValue(new Error('401'))
    const store = useAuthStore()
    await store.fetchMe()
    expect(store.user).toBeNull()
    expect(store.isAuthenticated).toBe(false)
    expect(store.isLoading).toBe(false)
  })

  it('login stores user and flags authenticated', async () => {
    apiPost.mockResolvedValue({ user: sampleUser, token: 't' })
    const store = useAuthStore()
    await store.login('alice', 'pw')
    expect(store.user).toEqual(sampleUser)
    expect(store.isAuthenticated).toBe(true)
  })

  it('logout clears user state even if API call fails', async () => {
    apiPost.mockRejectedValue(new Error('network'))
    const store = useAuthStore()
    store.user = sampleUser
    store.isAuthenticated = true
    await expect(store.logout()).rejects.toThrow('network')
    // `finally` block must still clear state even when the request rejects
    expect(store.user).toBeNull()
    expect(store.isAuthenticated).toBe(false)
  })

  it('refreshAccessToken returns true on success and updates user', async () => {
    apiPost.mockResolvedValue({ user: sampleUser, token: 't' })
    const store = useAuthStore()
    const ok = await store.refreshAccessToken()
    expect(ok).toBe(true)
    expect(store.isAuthenticated).toBe(true)
    expect(store.user).toEqual(sampleUser)
  })

  it('refreshAccessToken returns false and clears state on failure', async () => {
    apiPost.mockRejectedValue(new Error('invalid refresh'))
    const store = useAuthStore()
    store.user = sampleUser
    store.isAuthenticated = true
    const ok = await store.refreshAccessToken()
    expect(ok).toBe(false)
    expect(store.user).toBeNull()
    expect(store.isAuthenticated).toBe(false)
  })

  it('refreshAccessToken is guarded against concurrent calls', async () => {
    apiPost.mockImplementation(() => new Promise((resolve) => setTimeout(() => resolve({ user: sampleUser, token: 't' }), 10)))
    const store = useAuthStore()
    const [r1, r2] = await Promise.all([store.refreshAccessToken(), store.refreshAccessToken()])
    // Second concurrent call must short-circuit to false without triggering another request
    expect([r1, r2].filter(Boolean).length).toBe(1)
  })

  it('setOAuthUser sets user + isAuthenticated synchronously', () => {
    const store = useAuthStore()
    store.setOAuthUser(sampleUser)
    expect(store.user).toEqual(sampleUser)
    expect(store.isAuthenticated).toBe(true)
  })
})
