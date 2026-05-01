/**
 * useSurfaceMode — controls whether the current page renders in "focus" surface
 * mode, which dims the aurora halo and mesh texture on data-dense pages so
 * logs/metrics/tables stay crisp.
 *
 * Sets <html data-surface="focus"> (or removes it). The CSS variables
 * --halo-dim and --mesh-opacity react automatically (see tailwind.css).
 *
 * Focus routes (decided 2026-04-30):
 *   /logs, /telemetry, /metrics, /topology, /issues, and list-type pages.
 *
 * ⚠️ USER CONTRIBUTION POINT below — see `isFocusRoute()` TODO.
 */

// The focus-mode prefix set. Everything under these routes is a candidate
// for focus mode (subject to the rules in isFocusRoute below).
const FOCUS_PREFIXES = [
  '/logs',
  '/telemetry',
  '/metrics',
  '/topology',
  '/issues',
  '/clusters',      // list-type (kept per user's "列表型页面" call-out)
  '/accounts',      // list-type
  '/deployments',   // list-type
]

// Sub-path suffixes that should ESCAPE focus mode even when the parent is a
// focus route (forms need borders + full chrome).
//
// TODO (USER CONTRIBUTION — 5-10 lines):
// Pick your matching strategy in isFocusRoute() below. Three valid approaches
// are documented in the comment; the default scaffold uses strategy B
// (prefix + escape suffixes), which is the recommended balance of simplicity
// and correctness. Delete the approaches you don't want; keep the code for
// your chosen one. If you pick C (page meta), also delete FOCUS_PREFIXES and
// the ESCAPE_SUFFIXES below and switch to reading route.meta.surface instead.
const ESCAPE_SUFFIXES = ['/edit', '/new', '/create']

function isFocusRoute(path: string, _routeMeta?: Record<string, unknown>): boolean {
  // ─── Strategy A: coarse prefix match (simplest) ───
  // return FOCUS_PREFIXES.some((p) => path === p || path.startsWith(p + '/'))

  // ─── Strategy B: prefix + escape suffixes (RECOMMENDED) ───
  const inFocusPrefix = FOCUS_PREFIXES.some(
    (p) => path === p || path.startsWith(p + '/'),
  )
  if (!inFocusPrefix) return false
  return !ESCAPE_SUFFIXES.some((s) => path.endsWith(s))

  // ─── Strategy C: page meta opt-in (most flexible) ───
  // return _routeMeta?.surface === 'focus'
}

export const useSurfaceMode = () => {
  const route = useRoute()

  const isFocus = computed(() => isFocusRoute(route.path, route.meta))

  if (import.meta.client) {
    watchEffect(() => {
      const root = document.documentElement
      if (isFocus.value) {
        root.setAttribute('data-surface', 'focus')
      } else {
        root.removeAttribute('data-surface')
      }
    })
  }

  return { isFocus }
}
