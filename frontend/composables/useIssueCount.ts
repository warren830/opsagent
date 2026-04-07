/**
 * Global issue count composable — polls /api/issues/count every 30 seconds.
 * Used by the sidebar to show a badge on the Issues nav item.
 */
import { ref, readonly } from 'vue'

const issueCount = ref(0)
let polling: ReturnType<typeof setInterval> | null = null

export function useIssueCount() {
  const api = useApi()

  async function fetchCount() {
    try {
      const data = await api.get<{ count: number }>('/api/issues/count')
      issueCount.value = data.count
    } catch {
      // ignore — user may not be authenticated yet
    }
  }

  function startPolling() {
    if (polling) return
    fetchCount()
    polling = setInterval(fetchCount, 30_000)
  }

  function stopPolling() {
    if (polling) {
      clearInterval(polling)
      polling = null
    }
  }

  return {
    issueCount: readonly(issueCount),
    fetchCount,
    startPolling,
    stopPolling,
  }
}
