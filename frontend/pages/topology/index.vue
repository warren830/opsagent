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
const { fitView } = useVueFlow({ id: 'service-topology' })

// ─── Edge styles by kind ────────────────────────────────────────
const edgeStyleMap: Record<string, Record<string, any>> = {
  'ingress-service': { stroke: '#a855f7', strokeWidth: 1.5 },
  'service-deployment': { stroke: '#3b82f6', strokeWidth: 1.5 },
  'service-rollout': { stroke: '#f97316', strokeWidth: 2 },
  default: { stroke: '#6b7280', strokeWidth: 1 },
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
  let groupY = 0

  for (const [key, group] of nsGroups) {
    const maxRows = Math.max(group.ingresses.length, group.services.length, group.workloads.length, 1)
    const groupHeight = maxRows * ROW_H + 60

    // Zone background (non-interactive)
    nodes.push({
      id: `zone-${key}`,
      type: 'group',
      position: { x: 0, y: groupY },
      draggable: false,
      selectable: false,
      connectable: false,
      focusable: false,
      style: {
        width: '740px',
        height: `${groupHeight}px`,
        background: 'rgba(100,140,200,0.04)',
        border: '1px dashed rgba(100,140,200,0.22)',
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
      position: { x: 8, y: groupY + 6 },
      draggable: false,
      selectable: false,
      connectable: false,
      focusable: false,
      style: {
        width: 'auto', height: 'auto',
        background: 'transparent', border: 'none', pointerEvents: 'none',
        fontSize: '9px', fontWeight: '600', color: 'rgba(71,85,105,0.55)',
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
        position: { x: COL_X.ingress, y: groupY + baseY + i * ROW_H },
        data: { label: n.label, subtitle: n.subtitle, kind: n.kind, status: n.status, replicas: n.replicas, namespace: n.namespace, cluster: n.cluster },
      })
    })

    // Place services
    group.services.forEach((n, i) => {
      nodes.push({
        id: n.id,
        type: 'topology',
        draggable: false,
        position: { x: COL_X.service, y: groupY + baseY + i * ROW_H },
        data: { label: n.label, subtitle: n.subtitle, kind: n.kind, status: n.status, replicas: n.replicas, namespace: n.namespace, cluster: n.cluster },
      })
    })

    // Place workloads (deployments + rollouts)
    group.workloads.forEach((n, i) => {
      nodes.push({
        id: n.id,
        type: 'topology',
        draggable: false,
        position: { x: COL_X.workload, y: groupY + baseY + i * ROW_H },
        data: { label: n.label, subtitle: n.subtitle, kind: n.kind, status: n.status, replicas: n.replicas, namespace: n.namespace, cluster: n.cluster },
      })
    })

    groupY += groupHeight + 20
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
  { color: 'bg-purple-400', label: 'Ingress' },
  { color: 'bg-blue-400', label: 'Service' },
  { color: 'bg-emerald-400', label: 'Deployment' },
  { color: 'bg-orange-400', label: 'Rollout' },
]

const statusLegend = [
  { color: 'bg-emerald-400', label: t('topology.healthy') },
  { color: 'bg-amber-400', label: t('topology.warning') },
  { color: 'bg-red-400 animate-pulse', label: t('topology.critical') },
]
</script>

<template>
  <div class="space-y-3">
    <!-- Header -->
    <div class="flex items-center justify-between flex-wrap gap-3">
      <div class="flex items-center gap-3">
        <div class="p-2 rounded-lg bg-blue-500/10 border border-blue-500/20">
          <Network class="h-5 w-5 text-sky-600" />
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
        <Badge variant="outline" class="text-[9px] px-1.5 py-0 border-violet-300 text-violet-600">Ingress</Badge>
        <span class="text-slate-400 text-[10px]">→</span>
        <Badge variant="outline" class="text-[9px] px-1.5 py-0 border-sky-300 text-sky-600">Service</Badge>
        <span class="text-slate-400 text-[10px]">→</span>
        <Badge variant="outline" class="text-[9px] px-1.5 py-0 border-emerald-300 text-emerald-600">Workload</Badge>
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
            const kind = n.data?.kind
            if (kind === 'ingress') return '#a855f7'
            if (kind === 'service') return '#3b82f6'
            if (kind === 'rollout') return '#f97316'
            if (kind === 'deployment') return '#10b981'
            return '#6b7280'
          }"
          :mask-color="'rgba(148,163,184,0.25)'"
          class="!bg-white/70 !backdrop-blur-md !border-slate-200 !rounded-lg"
        />
        <Controls class="!bg-white/70 !backdrop-blur-md !border-slate-200 !rounded-lg" />
      </VueFlow>
    </div>
  </div>
</template>

<style>
/* Vue Flow — light theme integration with Sky & Lavender palette */
.topology-flow .vue-flow__background {
  background: transparent;
}
.topology-flow .vue-flow__edge-path {
  stroke: #3b82f6;
  stroke-opacity: 0.45;
  filter: drop-shadow(0 0 1px rgba(59, 130, 246, 0.3));
}
.topology-flow .vue-flow__edge-text {
  font-size: 9px;
  fill: rgba(71, 85, 105, 0.75);         /* slate-600 */
}
.topology-flow .vue-flow__edge-textbg {
  fill: rgba(255, 255, 255, 0.85);
  rx: 3;
}
.topology-flow .vue-flow__controls-button {
  background: rgba(255, 255, 255, 0.85) !important;
  border-color: rgba(226, 232, 240, 1) !important;   /* slate-200 */
  color: rgba(51, 65, 85, 0.9) !important;           /* slate-700 */
  backdrop-filter: blur(6px);
}
.topology-flow .vue-flow__controls-button:hover {
  background: rgba(255, 255, 255, 0.95) !important;
  color: rgba(37, 99, 235, 1) !important;            /* sky-600 */
}
.topology-flow .vue-flow__controls-button svg {
  fill: currentColor;
}
.topology-flow .vue-flow__minimap {
  background: rgba(255, 255, 255, 0.7) !important;
  border: 1px solid rgba(226, 232, 240, 1) !important;
  backdrop-filter: blur(8px);
  border-radius: 8px;
}
.topology-flow .vue-flow__minimap-mask {
  fill: rgba(147, 197, 253, 0.25);        /* sky-300 tinted mask */
  stroke: rgba(59, 130, 246, 0.5);
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
