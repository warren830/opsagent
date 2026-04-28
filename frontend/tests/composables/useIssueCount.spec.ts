import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { useIssueCount } from '~/composables/useIssueCount'

const getMock = vi.fn()
vi.stubGlobal('useApi', () => ({ get: getMock }))

describe('useIssueCount', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('fetchCount updates the reactive count', async () => {
    getMock.mockResolvedValue({ count: 7 })
    const { issueCount, fetchCount } = useIssueCount()
    await fetchCount()
    expect(issueCount.value).toBe(7)
  })

  it('fetchCount swallows errors (user may not be authed)', async () => {
    getMock.mockRejectedValue(new Error('401'))
    const { fetchCount } = useIssueCount()
    await expect(fetchCount()).resolves.toBeUndefined()
  })

  it('startPolling triggers an immediate fetch and then every 30s', async () => {
    getMock.mockResolvedValue({ count: 1 })
    const { startPolling, stopPolling } = useIssueCount()
    startPolling()
    // initial call is synchronous-ish; await a tick
    await vi.advanceTimersByTimeAsync(0)
    expect(getMock).toHaveBeenCalledTimes(1)

    await vi.advanceTimersByTimeAsync(30_000)
    expect(getMock).toHaveBeenCalledTimes(2)

    await vi.advanceTimersByTimeAsync(30_000)
    expect(getMock).toHaveBeenCalledTimes(3)

    stopPolling()
    await vi.advanceTimersByTimeAsync(60_000)
    expect(getMock).toHaveBeenCalledTimes(3)
  })

  it('startPolling is idempotent — second call does not create extra interval', async () => {
    getMock.mockResolvedValue({ count: 1 })
    const { startPolling, stopPolling } = useIssueCount()
    startPolling()
    startPolling()
    await vi.advanceTimersByTimeAsync(30_000)
    // Only one timer fired — original + 1 tick = 2 total, not 4
    expect(getMock).toHaveBeenCalledTimes(2)
    stopPolling()
  })
})
