<script setup lang="ts">
import { VueFlow, useVueFlow } from '@vue-flow/core'
import { MiniMap } from '@vue-flow/minimap'
import { Controls } from '@vue-flow/controls'
import { Network, RefreshCw, Loader2 } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
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
const flowNodes = ref<any[]>([])
const flowEdges = ref<any[]>([])

const nodeTypes = { topology: TopologyNode }
// Use the string-id signature — the object-options form was deprecated in
// @vue-flow/core v1.x and removed in v2.
const { fitView } = useVueFlow('service-topology')

// ─── Edge styles by kind ────────────────────────────────────────
// Token-driven so edges adapt to both light and aurora themes. SVG's
// stroke attribute resolves CSS vars the same way CSS properties do.
const edgeStyleMap: Record<string, Record<string, any>> = {
  'ingress-service':    { stroke: 'hsl(var(--ai))',      strokeWidth: 1.5 }, // agent/layer-7 ingress
  'service-deployment': { stroke: 'hsl(var(--info))',    strokeWidth: 1.5 }, // cluster-internal traffic
  'service-rollout':    { stroke: 'hsl(var(--warning))', strokeWidth: 2 },   // progressive delivery
  default:              { stroke: 'hsl(var(--muted-foreground) / 0.5)', strokeWidth: 1 },
}

function getEdgeStyle(sourceKind: string, targetKind: string) {
  return edgeStyleMap[`${sourceKind}-${targetKind}`] || edgeStyleMap.default
}

// ─── Auto-layout: left-to-right tiers ───────────────────────────
// Ingress (x=0) → Service (x=250) → Deployment/Rollout (x=500)
function layoutGraph(apiNodes: TopoNode[], apiEdges: TopoEdge[]) {
  const nodeMap = new Map(apiNodes.map(n => [n.id, n]))

  // Group by namespace+cluster, then by kind
  interface NsGroup { namespace: string; cluster: string; ingresses: TopoNode[]; services: TopoNode[]; workloads: TopoNode[] }
  const nsGroups = new Map<string, NsGroup>()

  for (const n of apiNodes) {
    const key = `${n.cluster_id}/${n.namespace}`
    if (!nsGroups.has(key)) {
      nsGroups.set(key, { namespace: n.namespace, cluster: n.cluster, ingresses: [], services: [], workloads: [] })
    }
    const g = nsGroups.get(key)!
    if (n.kind === 'ingress') g.ingresses.push(n)
    else if (n.kind === 'service') g.services.push(n)
    else g.workloads.push(n)
  }

  const nodes: any[] = []
  const edges: any[] = []
  const ROW_H = 80
  const COL_X = { ingress: 40, service: 280, workload: 540 }
  const ZONE_W = 740
  const ZONE_GAP_X = 40
  const ZONE_GAP_Y = 24
  // Namespaces are laid out in a 2-col grid so the whole topology fills
  // wide viewports instead of collapsing to a tall narrow strip under
  // Vue Flow's fit-view-on-init. When there's only 1 ns we fall back to
  // single column (no wasted gap).
  const NS_COLS = nsGroups.size > 1 ? 2 : 1
  // Track the height of the tallest zone in each row so the next row
  // starts below the tallest zone of the previous row.
  const rowHeights: number[] = []
  let nsIdx = 0

  for (const [key, group] of nsGroups) {
    const maxRows = Math.max(group.ingresses.length, group.services.length, group.workloads.length, 1)
    const groupHeight = maxRows * ROW_H + 60

    const colIdx = nsIdx % NS_COLS
    const rowIdx = Math.floor(nsIdx / NS_COLS)
    rowHeights[rowIdx] = Math.max(rowHeights[rowIdx] || 0, groupHeight)
    // Running Y = sum of previous row heights + their gaps.
    const groupY = rowHeights.slice(0, rowIdx).reduce((a, h) => a + h + ZONE_GAP_Y, 0)
    const groupX = colIdx * (ZONE_W + ZONE_GAP_X)
    nsIdx++

    // Zone background (non-interactive)
    nodes.push({
      id: `zone-${key}`,
      type: 'group',
      position: { x: groupX, y: groupY },
      draggable: false,
      selectable: false,
      connectable: false,
      focusable: false,
      style: {
        width: '740px',
        height: `${groupHeight}px`,
        // Token-driven zone surface: barely-visible fill + dashed border
        // using the panel + border tokens so both light and aurora themes
        // render cleanly.
        background: 'hsl(var(--panel) / 0.35)',
        border: '1px dashed hsl(var(--border) / 0.6)',
        borderRadius: '12px',
        pointerEvents: 'none',
      },
      data: { label: '' },
    })

    // Zone label (non-interactive)
    const labelId = `label-${key}`
    nodes.push({
      id: labelId,
      type: 'group',
      position: { x: groupX + 8, y: groupY + 6 },
      draggable: false,
      selectable: false,
      connectable: false,
      focusable: false,
      style: {
        width: 'auto', height: 'auto',
        background: 'transparent', border: 'none', pointerEvents: 'none',
        // Read the muted-foreground token so the label adapts to both Sky &
        // Lavender (light) and Aurora (dark). 0.85 alpha keeps it secondary
        // but clearly readable on dark base (#0A0B10).
        fontSize: '10px', fontWeight: '600',
        color: 'hsl(var(--muted-foreground) / 0.85)',
        textTransform: 'uppercase', letterSpacing: '0.1em',
      },
      data: { label: `${group.namespace} @ ${group.cluster}` },
    })

    const baseY = 36 // inside zone

    // Place ingresses
    group.ingresses.forEach((n, i) => {
      nodes.push({
        id: n.id,
        type: 'topology',
        draggable: false,
        position: { x: groupX + COL_X.ingress, y: groupY + baseY + i * ROW_H },
        data: { label: n.label, subtitle: n.subtitle, kind: n.kind, status: n.status, replicas: n.replicas, namespace: n.namespace, cluster: n.cluster },
      })
    })

    // Place services
    group.services.forEach((n, i) => {
      nodes.push({
        id: n.id,
        type: 'topology',
        draggable: false,
        position: { x: groupX + COL_X.service, y: groupY + baseY + i * ROW_H },
        data: { label: n.label, subtitle: n.subtitle, kind: n.kind, status: n.status, replicas: n.replicas, namespace: n.namespace, cluster: n.cluster },
      })
    })

    // Place workloads (deployments + rollouts)
    group.workloads.forEach((n, i) => {
      nodes.push({
        id: n.id,
        type: 'topology',
        draggable: false,
        position: { x: groupX + COL_X.workload, y: groupY + baseY + i * ROW_H },
        data: { label: n.label, subtitle: n.subtitle, kind: n.kind, status: n.status, replicas: n.replicas, namespace: n.namespace, cluster: n.cluster },
      })
    })

    // groupY for this iteration was computed from rowHeights at the top —
    // no accumulator update needed (rowHeights tracks the max per row).
  }

  // Build edges with styles
  for (const e of apiEdges) {
    const srcNode = nodeMap.get(e.source)
    const tgtNode = nodeMap.get(e.target)
    if (!srcNode || !tgtNode) continue

    const style = getEdgeStyle(srcNode.kind, tgtNode.kind)
    edges.push({
      id: e.id,
      source: e.source,
      target: e.target,
      animated: true,
      label: e.label || undefined,
      style,
    })
  }

  return { nodes, edges }
}

// ─── API ────────────────────────────────────────────────────────
async function fetchTopology(forceRefresh = false) {
  loading.value = true
  try {
    const url = forceRefresh ? '/api/topology?refresh=true' : '/api/topology'
    const data = await api.get<TopologyResponse>(url)
    const { nodes, edges } = layoutGraph(data.nodes, data.edges)
    flowNodes.value = nodes
    flowEdges.value = edges
    // Fit view after render
    nextTick(() => { setTimeout(() => fitView({ padding: 0.15 }), 200) })
  } catch {
    // silent — empty state shown
  } finally {
    loading.value = false
  }
}

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
          <h1 class="text-base font-semibold text-foreground">{{ t('topology.title') }}</h1>
          <p class="text-[11px] text-muted-foreground">{{ t('topology.subtitle') }}</p>
        </div>
      </div>

      <div class="flex items-center gap-4">
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
        :nodes-draggable="false"
        :nodes-connectable="false"
        :snap-to-grid="true"
        :snap-grid="[10, 10]"
        fit-view-on-init
        class="topology-flow"
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
          class="!bg-panel/$1 !backdrop-blur-md !border-border !rounded-lg"
        />
        <Controls class="!bg-panel/$1 !backdrop-blur-md !border-border !rounded-lg" />
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
