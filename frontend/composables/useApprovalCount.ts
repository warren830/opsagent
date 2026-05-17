/**
 * Global pending-approval count composable — polls /api/approvals/count every 30 seconds.
 * Used by the sidebar to show a badge on the Approvals nav item.
 */
import { ref, readonly } from 'vue'

const approvalCount = ref(0)
let polling: ReturnType<typeof setInterval> | null = null

export function useApprovalCount() {
  const api = useApi()

  async function fetchCount() {
    try {
      const data = await api.get<{ count: number }>('/api/approvals/count')
      approvalCount.value = data.count
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
    approvalCount: readonly(approvalCount),
    fetchCount,
    startPolling,
    stopPolling,
  }
}
