/**
 * useHomeLiveData — hydrate the home page's "Systems online" panel with real
 * cluster + issue chips, turning the empty chat state into an "inhabited"
 * feeling. Auto-refreshes on the same cadence as the dashboard stats (60s).
 *
 * Contract:
 *   - Fetches /api/clusters + /api/issues in parallel on mount
 *   - Resilient: if one endpoint 500s, the other still renders
 *   - Truncates to `maxClusters` / `maxIssues` for compact display
 *   - Caller decides the sort strategy for issues (see USER CONTRIBUTION
 *     POINT below) — this is a product decision about what the operator
 *     wants to see first when they land on the home page
 */

// Minimal shapes — copied from the source pages to avoid cross-page imports.
// If the Cluster/Issue types on their home pages drift, these may need to
// follow. Keeping them minimal (only the fields we render) limits coupling.
export interface LiveCluster {
  id: string
  name: string
  cloud: string
  region: string | null
  status: string
  is_discovered: boolean
}

export interface LiveIssue {
  id: string
  title: string
  severity: 'critical' | 'high' | 'medium' | 'low'
  status: 'open' | 'investigating' | 'rca_done' | 'resolved'
  issue_type: 'incident' | 'prediction'
  created_at: string
}

export interface UseHomeLiveDataOptions {
  maxClusters?: number
  maxIssues?: number
  refreshMs?: number
}

export const useHomeLiveData = (opts: UseHomeLiveDataOptions = {}) => {
  const { maxClusters = 6, maxIssues = 5, refreshMs = 60_000 } = opts
  const api = useApi()

  const clusters = ref<LiveCluster[]>([])
  const issues = ref<LiveIssue[]>([])
  const clustersLoading = ref(true)
  const issuesLoading = ref(true)
  const clustersError = ref<string | null>(null)
  const issuesError = ref<string | null>(null)

  let timer: ReturnType<typeof setInterval> | null = null

  async function fetchClusters() {
    try {
      clustersError.value = null
      const data = await api.get<LiveCluster[]>('/api/clusters')
      clusters.value = truncateClusters(data)
    } catch (e) {
      clustersError.value = e instanceof Error ? e.message : String(e)
    } finally {
      clustersLoading.value = false
    }
  }

  async function fetchIssues() {
    try {
      issuesError.value = null
      const data = await api.get<LiveIssue[]>('/api/issues')
      issues.value = rankIssues(data)
    } catch (e) {
      issuesError.value = e instanceof Error ? e.message : String(e)
    } finally {
      issuesLoading.value = false
    }
  }

  // Clusters: show the most recently seen ones first, capped at maxClusters.
  function truncateClusters(raw: LiveCluster[]): LiveCluster[] {
    return raw
      .slice()
      .sort((a, b) => {
        // Discovered + healthy-looking first, then by name for stability.
        if (a.is_discovered !== b.is_discovered) return a.is_discovered ? -1 : 1
        return a.name.localeCompare(b.name)
      })
      .slice(0, maxClusters)
  }

  // ─────────────────────────────────────────────────────────────────
  // USER CONTRIBUTION POINT — issues ranking
  //
  // On the home page, when the operator lands and looks at "live issues",
  // what should they see first? Three reasonable product stances:
  //
  //   A) Fires first (recommended for on-call / NOC): sort by severity
  //      (critical > high > medium > low), tiebreak by recency.
  //   B) Fresh first (recommended for morning-review): sort by created_at
  //      desc, show the newest N regardless of severity.
  //   C) Pending-work first: filter out status ∈ {rca_done, resolved},
  //      then sort by severity.
  //
  // The scaffold below implements A. Uncomment B or C if your team works
  // differently. Safe to change — it's the ONE place that decides what
  // "priority" means for the home page.
  function rankIssues(raw: LiveIssue[]): LiveIssue[] {
    const sevOrder: Record<LiveIssue['severity'], number> = {
      critical: 0, high: 1, medium: 2, low: 3,
    }
    // ─── A. Fires first (default) ───
    return raw
      .slice()
      .sort((a, b) => {
        const ds = sevOrder[a.severity] - sevOrder[b.severity]
        if (ds !== 0) return ds
        return b.created_at.localeCompare(a.created_at) // newer wins tie
      })
      .slice(0, maxIssues)

    // ─── B. Fresh first ───
    // return raw.slice()
    //   .sort((a, b) => b.created_at.localeCompare(a.created_at))
    //   .slice(0, maxIssues)

    // ─── C. Pending work first ───
    // return raw
    //   .filter(i => i.status !== 'rca_done' && i.status !== 'resolved')
    //   .sort((a, b) => sevOrder[a.severity] - sevOrder[b.severity])
    //   .slice(0, maxIssues)
  }
  // ─────────────────────────────────────────────────────────────────

  async function refreshAll() {
    await Promise.all([fetchClusters(), fetchIssues()])
  }

  onMounted(() => {
    refreshAll()
    timer = setInterval(refreshAll, refreshMs)
  })
  onUnmounted(() => {
    if (timer) clearInterval(timer)
  })

  return {
    clusters,
    issues,
    clustersLoading,
    issuesLoading,
    clustersError,
    issuesError,
    refresh: refreshAll,
  }
}
