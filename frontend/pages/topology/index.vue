<script setup lang="ts">
import { VueFlow, useVueFlow } from '@vue-flow/core'
import * as d3 from 'd3-force'
import { MiniMap } from '@vue-flow/minimap'
import { Controls } from '@vue-flow/controls'
import { Network, RefreshCw, Loader2 } from 'lucide-vue-next'
import dagre from '@dagrejs/dagre'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem } from '@/components/ui/select'
import TopologyNode from '@/components/topology/TopologyNode.vue'

import '@vue-flow/core/dist/style.css'
import '@vue-flow/core/dist/theme-default.css'
import '@vue-flow/minimap/dist/style.css'
import '@vue-flow/controls/dist/style.css'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

// ─── Types ──────────────────────────────────────────────────────
interface TopoNode {
  id: string
  label: string
  subtitle: string | null
  kind: string
  namespace: string
  cluster: string
  cluster_id: string
  status: string
  replicas: string | null
}

interface TopoEdge {
  id: string
  source: string
  target: string
  label: string | null
}

interface TopologyResponse {
  nodes: TopoNode[]
  edges: TopoEdge[]
}

// ─── State ──────────────────────────────────────────────────────
const loading = ref(false)
const refreshing = ref(false)
// Raw payload from /api/topology — kept unfiltered so namespace switching
// is zero-cost (no re-fetch). Layout is a pure transform over these.
const rawNodes = ref<TopoNode[]>([])
const rawEdges = ref<TopoEdge[]>([])
// Sentinel for "show everything"; real values are "cluster/namespace" strings.
const NS_ALL = '__all__'
const activeNamespace = ref<string>(NS_ALL)

// Layout engine: 'hierarchical' runs dagre (layered DAG columns), 'force'
// runs d3-force simulation (organic scatter). Different aesthetic, same
// underlying filter/aggregate pipeline.
type LayoutMode = 'hierarchical' | 'force'
const layoutMode = ref<LayoutMode>('force')

// A: Kubernetes-system namespaces that are always noise for app developers.
// Hidden by default; toggled back on for SRE/cluster-admin exploration.
// TODO(cluster-admin): extend this list per your cluster's install (e.g.
// add 'linkerd', 'istio-system', 'vault', 'external-secrets' if relevant).
const SYSTEM_NAMESPACES = new Set([
  'kube-system', 'kube-public', 'kube-node-lease',
  'gmp-system', 'gmp-public',                       // GKE-managed Prometheus
  'argocd', 'argo-rollouts', 'argo-events',
  'cert-manager',
  'ingress-nginx',
])
const showSystemNs = ref(false)

function isSystemNs(ns: string): boolean {
  // Prefix match catches 'istio-system' even if only 'istio' is in the list
  // — opt-in behaviour via `SYSTEM_NAMESPACES`, not a blanket prefix rule.
  if (SYSTEM_NAMESPACES.has(ns)) return true
  for (const s of SYSTEM_NAMESPACES) if (ns.startsWith(s + '-')) return true
  return false
}

const nodeTypes = { topology: TopologyNode }
// Use the string-id signature — the object-options form was deprecated in
// @vue-flow/core v1.x and removed in v2.
const { fitView } = useVueFlow('service-topology')

// ─── Focus mode ─────────────────────────────────────────────────
// When a node is selected (single click) we compute its 1-hop neighbour
// set and dim everything else. Semantics: "show me what talks to this
// service, and what it talks to". Ingress→Service→Deployment chains are
// almost always the user's mental model — 1-hop already reveals the full
// in-out pattern without pulling in the entire graph.
const focusedId = ref<string | null>(null)

// VueFlow emits `node-click` with a single payload `{ event, node }`
// — not two positional args. Guard against the node-click firing on
// ns-label pseudo-nodes (they have no edges so would hide the whole graph).
function onNodeClicked(payload: { event: MouseEvent; node: { id: string; type?: string } }) {
  const { node } = payload
  if (node.type !== 'topology') return   // ignore ns-labels
  focusedId.value = focusedId.value === node.id ? null : node.id
}
function clearFocus() {
  focusedId.value = null
}

// When focus changes, zoom into just the highlighted subset so the user
// actually *sees* the answer instead of hunting for a 1%-bright node in a
// 72-node crowd. On clear, re-fit to the whole current ns.
watch(focusedId, (id) => {
  nextTick(() => {
    setTimeout(() => {
      if (id == null) {
        fitView({ padding: 0.15, minZoom: 0.3, maxZoom: 1.0 })
      } else {
        // VueFlow v1 `fitView({ nodes })` accepts a string[] of ids and
        // frames only those — perfect for zooming into the focused subset.
        const keep = oneHopNeighbours(id, layoutResult.value.edges)
        fitView({
          nodes: Array.from(keep) as any,
          padding: 0.4,
          minZoom: 0.6,
          maxZoom: 1.6,
          duration: 450,   // smooth camera glide into focus
        })
      }
    }, 180) // wait for the transition to start so the fit lands smooth
  })
})

// Unique "cluster/namespace" keys. Filtered by showSystemNs so the dropdown
// itself reflects the toggle — when infra ns are hidden they disappear from
// the list entirely, not just from the rendered graph.
const namespaceOptions = computed(() => {
  const keys = new Set<string>()
  for (const n of rawNodes.value) {
    if (!showSystemNs.value && isSystemNs(n.namespace)) continue
    keys.add(`${n.cluster}/${n.namespace}`)
  }
  const list = Array.from(keys).sort()
  return [
    { value: NS_ALL, label: t('topology.allNamespaces') || '所有 namespace' },
    ...list.map(k => ({ value: k, label: k })),
  ]
})

// C: pick the best default namespace on first data load. "Best" = the
// non-system ns with the most nodes — the highest-signal view for an app
// team. Zipf-distribution assumption: your biggest app ns dominates the
// cluster and is what people actually want to see.
function pickDefaultNamespace(nodes: TopoNode[]): string {
  const counts = new Map<string, number>()
  for (const n of nodes) {
    if (isSystemNs(n.namespace)) continue
    const k = `${n.cluster}/${n.namespace}`
    counts.set(k, (counts.get(k) || 0) + 1)
  }
  if (counts.size === 0) return NS_ALL
  let best = NS_ALL
  let max = -1
  for (const [k, c] of counts) if (c > max) { max = c; best = k }
  return best
}

// ─── Edge styles by kind ────────────────────────────────────────
// Token-driven so edges adapt to both light and aurora themes. SVG's
// stroke attribute resolves CSS vars the same way CSS properties do.
// Strokes are thin (0.8–1.2px) and slightly transparent so high-fanout
// services (1 Service → N Deployments) don't render as a solid barcode.
const edgeStyleMap: Record<string, Record<string, any>> = {
  'ingress-service':    { stroke: 'hsl(var(--ai) / 0.55)',      strokeWidth: 1.2 },
  'service-deployment': { stroke: 'hsl(var(--info) / 0.45)',    strokeWidth: 0.9 },
  'service-rollout':    { stroke: 'hsl(var(--warning) / 0.55)', strokeWidth: 1.2 },
  default:              { stroke: 'hsl(var(--muted-foreground) / 0.35)', strokeWidth: 0.8 },
}

function getEdgeStyle(sourceKind: string, targetKind: string) {
  return edgeStyleMap[`${sourceKind}-${targetKind}`] || edgeStyleMap.default
}

// ─── Layout (dagre LR) ──────────────────────────────────────────
// Node dimensions reported to dagre — keep in sync with TopologyNode.vue's
// rendered width/height or edges will attach at the wrong anchor.
const NODE_W = 200
const NODE_H = 70

/**
 * Run one dagre LR layout pass over a set of nodes + edges.
 * Returns absolute x/y (center-shifted) + the overall bounding box so the
 * caller can stack multiple sub-graphs horizontally.
 *
 * rankdir='LR' means Ingress→Service→Workload naturally separate into
 * vertical columns (column index = distance from source in the graph).
 */
function runDagre(
  nodes: TopoNode[],
  edges: TopoEdge[],
): { positions: Map<string, { x: number; y: number }>; width: number; height: number } {
  const g = new dagre.graphlib.Graph()
  g.setGraph({
    rankdir: 'LR',
    // Tight vertical packing — nodesep only separates nodes in the same
    // column, and we want columns to feel dense vertically (otherwise a
    // 30-Service column blows the viewport).
    nodesep: 14,
    // Horizontal gap between columns — big enough for edges to route
    // through without crossing node bodies.
    ranksep: 90,
    marginx: 16,
    marginy: 16,
  })
  g.setDefaultEdgeLabel(() => ({}))

  for (const n of nodes) g.setNode(n.id, { width: NODE_W, height: NODE_H })
  for (const e of edges) g.setEdge(e.source, e.target)
  dagre.layout(g)

  const positions = new Map<string, { x: number; y: number }>()
  for (const n of nodes) {
    const p = g.node(n.id)
    positions.set(n.id, { x: p.x - NODE_W / 2, y: p.y - NODE_H / 2 })
  }
  const gg = g.graph()
  return { positions, width: gg.width || 0, height: gg.height || 0 }
}

/**
 * Force-directed layout via d3-force. Runs offline (no animation tick) for
 * DETERMINISTIC_TICKS iterations so positions converge before we hand the
 * result to VueFlow — VueFlow then treats them as static draggable nodes.
 *
 * Tuning:
 *  - linkDistance 140  : target edge length (pixels)
 *  - chargeStrength -600: node-node repulsion; more negative = more spread
 *  - collide NODE_W*0.6: hard-no-overlap radius
 *  - x/y forces light (0.04) so the whole graph drifts toward center but
 *    nodes still have room to find their natural position
 *
 * Seeding: kind-based initial position biases the final layout so Ingresses
 * end up on the left, Workloads on the right — even in "scatter" mode the
 * data-flow direction is readable at a glance.
 */
const DETERMINISTIC_TICKS = 300

function runForce(
  nodes: TopoNode[],
  edges: TopoEdge[],
): { positions: Map<string, { x: number; y: number }>; width: number; height: number } {
  if (nodes.length === 0) return { positions: new Map(), width: 0, height: 0 }

  // Prepare mutable copies — d3-force mutates these in place.
  const simNodes = nodes.map(n => {
    // Seed by kind so the simulation settles with a vague LR orientation.
    const xSeed = n.kind === 'ingress' ? 100 : n.kind === 'service' ? 500 : 900
    const ySeed = Math.random() * 600
    return { id: n.id, kind: n.kind, x: xSeed + (Math.random() - 0.5) * 200, y: ySeed }
  })
  const simLinks = edges.map(e => ({ source: e.source, target: e.target }))

  const sim = d3.forceSimulation(simNodes as any)
    .force('link', d3.forceLink(simLinks).id((d: any) => d.id).distance(140).strength(0.8))
    .force('charge', d3.forceManyBody().strength(-600))
    .force('center', d3.forceCenter(500, 400))
    .force('collide', d3.forceCollide(NODE_W * 0.6))
    // Gentle directional bias: ingress pulled left, workload pulled right.
    .force('x-bias', d3.forceX<any>(d =>
      d.kind === 'ingress' ? 100 : d.kind === 'service' ? 500 : 900,
    ).strength(0.08))
    .force('y-center', d3.forceY(400).strength(0.04))
    .stop()

  for (let i = 0; i < DETERMINISTIC_TICKS; i++) sim.tick()

  // Translate to VueFlow's top-left anchor + compute bounds.
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
  const positions = new Map<string, { x: number; y: number }>()
  for (const sn of simNodes as any[]) {
    const x = (sn.x ?? 0) - NODE_W / 2
    const y = (sn.y ?? 0) - NODE_H / 2
    positions.set(sn.id, { x, y })
    if (x < minX) minX = x
    if (y < minY) minY = y
    if (x + NODE_W > maxX) maxX = x + NODE_W
    if (y + NODE_H > maxY) maxY = y + NODE_H
  }
  // Shift so the layout's top-left sits at (0, 0) — makes per-ns tiling
  // predictable when runForce is called per subgraph in __all__ mode.
  for (const [id, p] of positions) {
    positions.set(id, { x: p.x - minX, y: p.y - minY })
  }
  return { positions, width: maxX - minX, height: maxY - minY }
}

// Dispatch based on current UI mode — the per-ns tiling logic in
// layoutGraph doesn't care which algorithm we picked.
function runLayout(
  nodes: TopoNode[],
  edges: TopoEdge[],
  mode: LayoutMode,
): { positions: Map<string, { x: number; y: number }>; width: number; height: number } {
  return mode === 'force' ? runForce(nodes, edges) : runDagre(nodes, edges)
}

/**
 * Filter by namespace + dedupe edges + layout.
 *
 * - Single-namespace: one dagre pass, rendered as-is.
 * - __all__ mode: split by namespace, run dagre per-ns, then tile the
 *   resulting sub-graphs in a row/column grid so the whole view reads as
 *   "N side-by-side mini-topologies" instead of one giant 3-column strip.
 */
function layoutGraph(apiNodes: TopoNode[], apiEdges: TopoEdge[], filterKey: string, includeSystem: boolean, mode: LayoutMode = 'hierarchical') {
  // 1a. System ns filter (cheap, drops 30-50% of noise on a typical cluster).
  // Applied before namespace-filter so counts in the dropdown reflect the
  // filtered set; `filterKey` is never a system ns because the dropdown
  // itself hides them.
  let stage1 = includeSystem ? apiNodes : apiNodes.filter(n => !isSystemNs(n.namespace))

  // 1b. B: Aggregate sibling Service nodes within a ns. K8s convention
  //     exposes one logical service via multiple API objects with suffixed
  //     names: `foo` (ClusterIP) + `foo-external` (LoadBalancer) + `foo-lb`
  //     + `foo-headless`. Three objects, one concept. We strip the suffix
  //     to find the canonical name and merge.
  //     Trade-off: may false-positive for genuinely distinct services that
  //     share the stripped prefix (e.g. an actual `-external` microservice).
  //     If this bites, constrain by also matching workload selector — for
  //     MVP the simpler rule is worth the occasional over-merge.
  const SERVICE_SUFFIXES = /-(external|internal|headless|lb|loadbalancer|nodeport|np|public|private)$/i
  const canonicalServiceName = (name: string) => name.replace(SERVICE_SUFFIXES, '')
  const aggAlias = new Map<string, string>()      // member id -> representative id
  const aggEndpoints = new Map<string, string[]>() // rep id -> list of subtitles
  {
    const groups = new Map<string, TopoNode[]>()
    for (const n of stage1) {
      if (n.kind !== 'service') { aggAlias.set(n.id, n.id); continue }
      const canonical = canonicalServiceName(n.label)
      const key = `svc|${n.cluster_id}|${n.namespace}|${canonical}`
      if (!groups.has(key)) groups.set(key, [])
      groups.get(key)!.push(n)
    }
    const mergedServices: TopoNode[] = []
    for (const group of groups.values()) {
      const rep = group[0]
      for (const m of group) aggAlias.set(m.id, rep.id)
      if (group.length === 1) {
        mergedServices.push(rep)
      } else {
        const subs = group.map(n => n.subtitle).filter(Boolean) as string[]
        aggEndpoints.set(rep.id, subs)
        mergedServices.push({
          ...rep,
          // Show the canonical (suffix-stripped) name — avoids the quirk of
          // the merged node inheriting whichever variant happened to be first.
          label: canonicalServiceName(rep.label),
          // Compact summary: "3 endpoints · ClusterIP:8080 · LoadBalancer:8080 · …"
          subtitle: `${group.length} endpoints${subs.length ? ' · ' + subs.slice(0, 2).join(' · ') : ''}${subs.length > 2 ? ' · …' : ''}`,
        })
      }
    }
    // Swap service nodes for merged version; keep non-service nodes as-is.
    stage1 = stage1.filter(n => n.kind !== 'service').concat(mergedServices)
  }

  // 2. Namespace filter (post-aggregation so one ns's Service count drops)
  const visibleNodes = filterKey === NS_ALL
    ? stage1
    : stage1.filter(n => `${n.cluster}/${n.namespace}` === filterKey)

  if (visibleNodes.length === 0) return { nodes: [], edges: [] }

  const visibleIds = new Set(visibleNodes.map(n => n.id))
  const nodeMap = new Map(visibleNodes.map(n => [n.id, n]))

  // 3. Edge dedupe + scope + rewrite via aggAlias so edges pointing at the
  //    dropped duplicate Services land on the representative instead.
  const seenPairs = new Set<string>()
  const visibleEdges: TopoEdge[] = []
  for (const e of apiEdges) {
    const src = aggAlias.get(e.source) || e.source
    const tgt = aggAlias.get(e.target) || e.target
    if (!visibleIds.has(src) || !visibleIds.has(tgt)) continue
    if (src === tgt) continue  // self-loops from aggregation
    const k = `${src}→${tgt}`
    if (seenPairs.has(k)) continue
    seenPairs.add(k)
    visibleEdges.push({ ...e, source: src, target: tgt })
  }

  // 3. Layout — split per-namespace when "all", single pass otherwise.
  const positions = new Map<string, { x: number; y: number }>()
  const nsLabels: Array<{ key: string; label: string; x: number; y: number; w: number }> = []

  if (filterKey === NS_ALL) {
    // Bucket by namespace
    const byNs = new Map<string, TopoNode[]>()
    for (const n of visibleNodes) {
      const k = `${n.cluster}/${n.namespace}`
      if (!byNs.has(k)) byNs.set(k, [])
      byNs.get(k)!.push(n)
    }

    // Tile sub-graphs in a 2-column grid so wide viewports fill horizontally
    // and we don't end up with a 5000-px-tall strip.
    const GRID_COLS = Math.min(2, byNs.size)
    const GAP_X = 80
    const GAP_Y = 60
    // Track max height per grid row so the next row starts below.
    const rowHeights: number[] = []
    let i = 0
    for (const [nsKey, nsNodes] of byNs) {
      const nsIds = new Set(nsNodes.map(n => n.id))
      const nsEdges = visibleEdges.filter(e => nsIds.has(e.source) && nsIds.has(e.target))
      const { positions: subPos, width: subW, height: subH } = runLayout(nsNodes, nsEdges, mode)

      const col = i % GRID_COLS
      const row = Math.floor(i / GRID_COLS)
      rowHeights[row] = Math.max(rowHeights[row] || 0, subH)
      // Accumulate previous rows' heights + their gaps.
      const offsetY = rowHeights.slice(0, row).reduce((a, h) => a + h + GAP_Y, 0) + 32 // leave room for ns label
      const offsetX = col * (subW + GAP_X) // uniform column width (uses this sub-graph's width; OK for 2 cols)

      for (const [id, p] of subPos) {
        positions.set(id, { x: p.x + offsetX, y: p.y + offsetY })
      }

      const firstNode = nsNodes[0]
      nsLabels.push({
        key: nsKey,
        label: `${firstNode.namespace} @ ${firstNode.cluster}`,
        x: offsetX,
        y: offsetY - 24,
        w: subW,
      })
      i++
    }
  } else {
    const { positions: subPos } = runLayout(visibleNodes, visibleEdges, mode)
    for (const [id, p] of subPos) positions.set(id, p)
  }

  // 4. Emit VueFlow nodes (topology data nodes + optional ns-label pseudo-nodes)
  const nodes: any[] = visibleNodes.map(n => {
    const p = positions.get(n.id)!
    return {
      id: n.id,
      type: 'topology',
      draggable: true,
      position: p,
      data: {
        label: n.label, subtitle: n.subtitle, kind: n.kind,
        status: n.status, replicas: n.replicas,
        namespace: n.namespace, cluster: n.cluster,
      },
    }
  })

  // Namespace header labels (only in __all__ mode)
  for (const lbl of nsLabels) {
    nodes.push({
      id: `nslabel-${lbl.key}`,
      type: 'default',
      position: { x: lbl.x, y: lbl.y },
      draggable: false,
      selectable: false,
      connectable: false,
      focusable: false,
      style: {
        width: `${Math.max(lbl.w, 240)}px`,
        height: 'auto',
        background: 'transparent',
        border: 'none',
        color: 'hsl(var(--muted-foreground) / 0.95)',
        fontSize: '11px',
        fontWeight: '600',
        letterSpacing: '0.08em',
        textTransform: 'uppercase',
        pointerEvents: 'none',
      },
      data: { label: lbl.label },
    })
  }

  // 5. Emit VueFlow edges
  const edges: any[] = visibleEdges.map(e => {
    const src = nodeMap.get(e.source)!
    const tgt = nodeMap.get(e.target)!
    return {
      id: e.id,
      source: e.source,
      target: e.target,
      type: 'smoothstep',     // orthogonal routing so parallel edges separate
      animated: false,        // animation on ~100 edges is noise
      label: e.label || undefined,
      style: getEdgeStyle(src.kind, tgt.kind),
    }
  })

  return { nodes, edges }
}

// Reactive derived output — recomputes on namespace switch without refetch.
// Memoise the layout result so flowNodes + flowEdges don't re-run
// `layoutGraph` twice per reactive tick (it's a heavy function).
// Focus-mode dimming is applied on top as a cheap mapping — flipping
// focusedId does NOT rebuild the dagre layout, only re-tags classes.
const layoutResult = computed(() =>
  layoutGraph(rawNodes.value, rawEdges.value, activeNamespace.value, showSystemNs.value, layoutMode.value),
)

/**
 * 1-hop neighbour set for the currently focused node.
 * Returns null when no focus is set (caller skips dimming).
 */
function oneHopNeighbours(id: string, edges: Array<{ source: string; target: string }>): Set<string> {
  const s = new Set<string>([id])
  for (const e of edges) {
    if (e.source === id) s.add(e.target)
    else if (e.target === id) s.add(e.source)
  }
  return s
}

const flow = computed(() => {
  const { nodes, edges } = layoutResult.value
  if (focusedId.value == null) return { nodes, edges }

  const keep = oneHopNeighbours(focusedId.value, edges)
  const dimmedNodes = nodes.map(n => ({
    ...n,
    class: keep.has(n.id) ? '' : 'topo-dimmed',
  }))
  // Edge is "focused" only if BOTH endpoints are in the neighbour set —
  // otherwise we'd highlight a stub edge leading to a dimmed-out target.
  const dimmedEdges = edges.map(e => ({
    ...e,
    class: keep.has(e.source) && keep.has(e.target) ? '' : 'topo-dimmed',
  }))
  return { nodes: dimmedNodes, edges: dimmedEdges }
})

const flowNodes = computed(() => flow.value.nodes)
const flowEdges = computed(() => flow.value.edges)

// ─── API ────────────────────────────────────────────────────────
async function fetchTopology(forceRefresh = false) {
  loading.value = true
  try {
    const url = forceRefresh ? '/api/topology?refresh=true' : '/api/topology'
    const data = await api.get<TopologyResponse>(url)
    rawNodes.value = data.nodes
    rawEdges.value = data.edges
    // C: On the very first load, jump to the biggest app namespace instead
    // of showing the user all 136 nodes at once. A manual refresh keeps the
    // user's current selection (they intentionally picked it).
    if (!forceRefresh && activeNamespace.value === NS_ALL) {
      activeNamespace.value = pickDefaultNamespace(data.nodes)
    }
    // Fit once after the initial load so the user sees the whole graph
    // regardless of how big the subset is. The minZoom floor keeps
    // 1000-node __all__ views from collapsing into a barcode.
    nextTick(() => {
      setTimeout(() => fitView({ padding: 0.15, minZoom: 0.3, maxZoom: 1.0 }), 150)
    })
  } catch {
    // silent — empty state shown
  } finally {
    loading.value = false
  }
}

// Re-fit when the namespace changes (smaller subset → zoom in naturally).
watch(activeNamespace, () => {
  nextTick(() => {
    setTimeout(() => fitView({ padding: 0.2, minZoom: 0.4, maxZoom: 1.2 }), 120)
  })
})

async function refresh() {
  refreshing.value = true
  await fetchTopology(true)
  refreshing.value = false
}

onMounted(() => {
  fetchTopology()
})

// ─── Legend ──────────────────────────────────────────────────────
const kindLegend = [
  { color: 'bg-ai', label: 'Ingress' },
  { color: 'bg-blue-400', label: 'Service' },
  { color: 'bg-success', label: 'Deployment' },
  { color: 'bg-warning', label: 'Rollout' },
]

const statusLegend = [
  { color: 'bg-success', label: t('topology.healthy') },
  { color: 'bg-warning', label: t('topology.warning') },
  { color: 'bg-destructive animate-pulse', label: t('topology.critical') },
]
</script>

<template>
  <div class="space-y-3">
    <!-- Header -->
    <div class="flex items-center justify-between flex-wrap gap-3">
      <div class="flex items-center gap-3">
        <div class="p-2 rounded-lg bg-blue-500/10 border border-blue-500/20">
          <Network class="h-5 w-5 text-info" />
        </div>
        <div>
          <h1 class="text-base font-semibold text-foreground">{{ t('topology.titleK8sLive') }}</h1>
          <p class="text-[11px] text-muted-foreground">{{ t('topology.subtitleK8sLive') }}</p>
        </div>
      </div>

      <div class="flex items-center gap-3">
        <!-- Namespace filter -->
        <Select v-model="activeNamespace">
          <SelectTrigger class="h-8 w-[240px] text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="opt in namespaceOptions" :key="opt.value" :value="opt.value" class="text-xs">
              {{ opt.label }}
            </SelectItem>
          </SelectContent>
        </Select>

        <span class="text-[10px] text-muted-foreground tabular-nums whitespace-nowrap">
          {{ flowNodes.length }} / {{ rawNodes.length }} {{ t('topology.nodeCountLabel') }}
        </span>

        <!-- Layout mode toggle: 散点 (force-directed) vs 层次 (dagre LR).
             Two-button segmented control — cheap and obvious. -->
        <div class="inline-flex rounded-md border border-border/60 p-0.5 gap-0.5 bg-muted/20">
          <button
            type="button"
            class="h-6 px-2 rounded text-[10px] font-medium transition-colors"
            :class="layoutMode === 'force'
              ? 'bg-primary/15 text-primary'
              : 'text-muted-foreground hover:text-foreground'"
            @click="layoutMode = 'force'"
          >{{ t('topology.layoutForce') }}</button>
          <button
            type="button"
            class="h-6 px-2 rounded text-[10px] font-medium transition-colors"
            :class="layoutMode === 'hierarchical'
              ? 'bg-primary/15 text-primary'
              : 'text-muted-foreground hover:text-foreground'"
            @click="layoutMode = 'hierarchical'"
          >{{ t('topology.layoutHierarchical') }}</button>
        </div>

        <!-- A: System-ns toggle. Default OFF so app teams don't see kube-system
             noise. SRE flips it on for cluster-wide investigation. -->
        <label class="flex items-center gap-1.5 text-[10px] text-muted-foreground whitespace-nowrap cursor-pointer select-none">
          <input type="checkbox" v-model="showSystemNs" class="h-3 w-3 cursor-pointer" />
          <span>{{ t('topology.showSystemNs') }}</span>
        </label>

        <!-- Legend -->
        <div class="hidden md:flex items-center gap-3">
          <div v-for="item in kindLegend" :key="item.label" class="flex items-center gap-1.5">
            <span class="h-2 w-2 rounded-full" :class="item.color" />
            <span class="text-[10px] text-muted-foreground">{{ item.label }}</span>
          </div>
          <div class="h-3 w-px bg-border/50" />
          <div v-for="item in statusLegend" :key="item.label" class="flex items-center gap-1.5">
            <span class="h-2 w-2 rounded-full shadow-[0_0_4px]" :class="item.color" />
            <span class="text-[10px] text-muted-foreground">{{ item.label }}</span>
          </div>
        </div>

        <Button variant="ghost" size="sm" class="h-8 w-8 p-0" :disabled="refreshing" @click="refresh">
          <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': refreshing }" />
        </Button>
      </div>
    </div>

    <!-- Loading -->
    <div v-if="loading && flowNodes.length === 0" class="flex flex-col items-center justify-center rounded-lg border border-border/60 bg-card/30" style="height: calc(100vh - 140px)">
      <Loader2 class="h-6 w-6 animate-spin text-blue-400 mb-3" />
      <p class="text-sm text-muted-foreground">{{ t('topology.loading') }}</p>
    </div>

    <!-- Empty -->
    <div v-else-if="flowNodes.length === 0" class="flex flex-col items-center justify-center rounded-lg border border-border/60 bg-card/30" style="height: calc(100vh - 140px)">
      <Network class="h-10 w-10 opacity-20 mb-3" />
      <p class="text-sm text-muted-foreground">{{ t('topology.empty') }}</p>
    </div>

    <!-- Topology Canvas -->
    <div v-else class="relative rounded-lg border border-border/60 bg-card/30 overflow-hidden" style="height: calc(100vh - 140px)">
      <!-- Flow direction labels -->
      <div class="absolute top-3 left-3 z-10 flex items-center gap-2 pointer-events-none">
        <Badge variant="outline" class="text-[9px] px-1.5 py-0 border-ai/40 text-ai">Ingress</Badge>
        <span class="text-muted-foreground text-[10px]">→</span>
        <Badge variant="outline" class="text-[9px] px-1.5 py-0 border-info/40 text-info">Service</Badge>
        <span class="text-muted-foreground text-[10px]">→</span>
        <Badge variant="outline" class="text-[9px] px-1.5 py-0 border-success/40 text-success">Workload</Badge>
      </div>

      <VueFlow
        id="service-topology"
        :nodes="flowNodes"
        :edges="flowEdges"
        :node-types="nodeTypes"
        :default-viewport="{ x: 40, y: 20, zoom: 0.85 }"
        :min-zoom="0.2"
        :max-zoom="2.5"
        :nodes-draggable="true"
        :nodes-connectable="false"
        :pan-on-drag="true"
        :zoom-on-scroll="true"
        :fit-view-on-init="false"
        class="topology-flow"
        @node-click="onNodeClicked"
        @pane-click="clearFocus"
      >
        <MiniMap
          :node-color="(n: any) => {
            // Vue Flow's minimap renders to canvas → needs resolved color
            // strings, not CSS vars. Read the live token value at call time
            // so switching themes updates the minimap on next render.
            const v = (name: string) => {
              if (typeof document === 'undefined') return '#6b7280'
              const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
              return raw ? `hsl(${raw})` : '#6b7280'
            }
            const kind = n.data?.kind
            if (kind === 'ingress') return v('--ai')
            if (kind === 'service') return v('--info')
            if (kind === 'rollout') return v('--warning')
            if (kind === 'deployment') return v('--success')
            return v('--muted-foreground')
          }"
          :mask-color="'hsl(var(--muted-foreground) / 0.2)'"
          class="!bg-panel/80 !backdrop-blur-md !border !border-border !rounded-lg"
        />
        <Controls class="!bg-panel/80 !backdrop-blur-md !border !border-border !rounded-lg" />
      </VueFlow>
    </div>
  </div>
</template>

<style>
/* Vue Flow — token-driven so it adapts to light (Sky & Lavender) and
   aurora (dark) themes automatically. */
.topology-flow .vue-flow__background {
  background: transparent;
}
.topology-flow .vue-flow__edge-path {
  stroke: hsl(var(--primary));
  stroke-opacity: 0.55;
  filter: drop-shadow(0 0 1px hsl(var(--primary) / 0.35));
}
.topology-flow .vue-flow__edge-text {
  font-size: 9px;
  fill: hsl(var(--muted-foreground) / 0.85);
}
.topology-flow .vue-flow__edge-textbg {
  fill: hsl(var(--panel) / 0.9);
  rx: 3;
}
.topology-flow .vue-flow__controls-button {
  background: hsl(var(--panel) / 0.85) !important;
  border-color: hsl(var(--border)) !important;
  color: hsl(var(--foreground) / 0.85) !important;
  backdrop-filter: blur(6px);
}
.topology-flow .vue-flow__controls-button:hover {
  background: hsl(var(--panel) / 0.95) !important;
  color: hsl(var(--primary)) !important;
}

/* Focus mode: everything outside the focused node's 1-hop set fades
   out. Nodes 12% + edges 6% so dense graphs (72+ nodes) still get a
   clear visual pop; lowering more would lose the spatial context that
   helps users orient themselves. `filter: blur(0.4px)` adds a second
   dimension of de-emphasis so even zoomed-in dimmed nodes read as
   "not the answer" at a glance. */
.topology-flow .vue-flow__node.topo-dimmed {
  opacity: 0.12;
  filter: blur(0.4px);
  transition: opacity 180ms ease-out, filter 180ms ease-out;
}
.topology-flow .vue-flow__node {
  transition: opacity 180ms ease-out, filter 180ms ease-out;
}
.topology-flow .vue-flow__edge.topo-dimmed {
  opacity: 0.06;
  transition: opacity 180ms ease-out;
}
.topology-flow .vue-flow__edge {
  transition: opacity 180ms ease-out;
}
.topology-flow .vue-flow__controls-button svg {
  fill: currentColor;
}
.topology-flow .vue-flow__minimap {
  background: hsl(var(--panel) / 0.8) !important;
  border: 1px solid hsl(var(--border)) !important;
  backdrop-filter: blur(8px);
  border-radius: 8px;
}
.topology-flow .vue-flow__minimap-mask {
  fill: hsl(var(--primary) / 0.2);
  stroke: hsl(var(--primary) / 0.5);
  stroke-width: 2;
}
.topology-flow .vue-flow__edge.animated path {
  animation: edgeGlow 3s ease-in-out infinite;
}
@keyframes edgeGlow {
  0%, 100% { stroke-opacity: 0.4; }
  50% { stroke-opacity: 0.75; }
}
</style>
