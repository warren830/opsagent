/**
 * Global deployment count composable — tracks non-Healthy rollouts across all clusters.
 * Primary updates via SSE (deployment_change events); fallback polling every 120s.
 * Used by the sidebar to show a badge on the Deployments nav item.
 */
import { ref, readonly, watch } from 'vue'

const deploymentCount = ref(0)
let polling: ReturnType<typeof setInterval> | null = null

interface RolloutSummary {
  status: string
}

interface Cluster {
  id: string
}

export function useDeploymentCount() {
  const api = useApi()
  const { lastNotification } = useNotifications()

  async function fetchCount() {
    try {
      const clusters = await api.get<Cluster[]>('/api/clusters')
      if (clusters.length === 0) {
        deploymentCount.value = 0
        return
      }

      const results = await Promise.allSettled(
        clusters.map(c => api.get<RolloutSummary[]>(`/api/clusters/${c.id}/rollouts`))
      )

      let count = 0
      for (const r of results) {
        if (r.status === 'fulfilled') {
          count += r.value.filter(ro => ro.status !== 'Healthy').length
        }
      }
      deploymentCount.value = count
    } catch {
      // ignore — user may not be authenticated yet
    }
  }

  // SSE-driven: refresh immediately when a deployment change event arrives
  watch(lastNotification, (n) => {
    if (n && n.event_type === 'deployment_change') {
      fetchCount()
    }
  })

  function startPolling() {
    if (polling) return
    fetchCount()
    // Fallback polling only — primary updates come via SSE
    polling = setInterval(fetchCount, 120_000)
  }

  function stopPolling() {
    if (polling) {
      clearInterval(polling)
      polling = null
    }
  }

  return {
    deploymentCount: readonly(deploymentCount),
    fetchCount,
    startPolling,
    stopPolling,
  }
}
