<script setup lang="ts">
/**
 * DependencyGraph — mini 1-2 hop dependency graph for a service detail page.
 *
 * Data: GET /api/catalog/entities/:id/graph?depth=2
 * Layout: dagre LR (left → right) so depends_on / consumes chains read as
 * columns; tight vertical packing for dense fan-outs.
 * Styling: reuses the Aurora palette from /components/catalog/TopologyGraph,
 * but with a richer node (icon + name + kind badge) and relation-typed edge
 * colours so the "what talks to what, and how" reading is instant.
 *
 * Constraints:
 *  - Fixed h-[480px] container so the graph doesn't push page layout.
 *  - Center node gets an outline ring + glow so the user instantly sees
 *    "this is the entity I'm looking at".
 *  - Non-center Components navigate via NuxtLink → /services/:id.
 *  - MiniMap enabled so dense 2-hop graphs stay navigable.
 */
import { ref, computed, onMounted, watch, nextTick } from 'vue'
import { VueFlow, useVueFlow, Position, Handle } from '@vue-flow/core'
import { MiniMap } from '@vue-flow/minimap'
import { Controls } from '@vue-flow/controls'
import { RefreshCw, Loader2, Layers, Box, Cloud, Plug, Users2, Package } from 'lucide-vue-next'
import dagre from '@dagrejs/dagre'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'

import '@vue-flow/core/dist/style.css'
import '@vue-flow/core/dist/theme-default.css'
import '@vue-flow/minimap/dist/style.css'
import '@vue-flow/controls/dist/style.css'

const props = withDefaults(defineProps<{
  entityId: string
  depth?: number
}>(), { depth: 2 })

const { t } = useI18n()
const api = useApi()
const router = useRouter()

// ─── Types ──────────────────────────────────────────────────────
interface EntityGraphNode {
  id: string
  name: string
  kind: 'component' | 'system' | 'api' | 'resource' | 'group' | string
  lifecycle?: string
  owner_group_id?: string | null
  tenant_id?: string
}

interface EntityGraphEdge {
  from_id: string
  to_id: string
  relation_type: 'owns' | 'provides' | 'consumes' | 'depends_on' | 'part_of' | 'deployed_on' | string
}

interface EntityGraph {
  nodes: EntityGraphNode[]
  edges: EntityGraphEdge[]
}

// ─── Kind palette (Aurora tokens) ───────────────────────────────
// Matches TopologyGraph.vue so navigating between catalog/graph and
// services/dependencies feels like one visual system.
const kindStyle: Record<string, { bg: string; border: string; text: string; icon: any }> = {
  system:    { bg: 'hsl(280 60% 70% / 0.18)', border: 'hsl(280 60% 70% / 0.85)', text: 'hsl(280 60% 88%)', icon: Layers },
  component: { bg: 'hsl(170 70% 60% / 0.18)', border: 'hsl(170 70% 60% / 0.85)', text: 'hsl(170 70% 85%)', icon: Box },
  api:       { bg: 'hsl(35 90% 60% / 0.18)',  border: 'hsl(35 90% 60% / 0.85)',  text: 'hsl(35 90% 85%)',  icon: Plug },
  resource:  { bg: 'hsl(235 75% 70% / 0.18)', border: 'hsl(235 75% 70% / 0.85)', text: 'hsl(235 75% 88%)', icon: Cloud },
  group:     { bg: 'hsl(225 10% 55% / 0.18)', border: 'hsl(225 10% 55% / 0.85)', text: 'hsl(225 10% 85%)', icon: Users2 },
}

function getKindStyle(kind: string) {
  return kindStyle[kind] || { ...kindStyle.group, icon: Package }
}

// ─── Edge palette by relation_type ──────────────────────────────
// Semantic colouring so the user can scan the graph and pattern-match
// "green = this exposes; indigo = this needs; amber = infra binding".
const edgeStyleByRelation: Record<string, { stroke: string; strokeWidth: number }> = {
  provides:    { stroke: 'hsl(var(--success) / 0.75)',          strokeWidth: 1.4 },
  depends_on:  { stroke: 'hsl(235 75% 70% / 0.8)',              strokeWidth: 1.4 },
  consumes:    { stroke: 'hsl(235 75% 70% / 0.8)',              strokeWidth: 1.4 },
  owns:        { stroke: 'hsl(var(--muted-foreground) / 0.55)', strokeWidth: 1.0 },
  part_of:     { stroke: 'hsl(var(--muted-foreground) / 0.55)', strokeWidth: 1.0 },
  deployed_on: { stroke: 'hsl(var(--warning) / 0.75)',          strokeWidth: 1.3 },
}

function getEdgeStyle(relType: string) {
  return edgeStyleByRelation[relType] || { stroke: 'hsl(var(--muted-foreground) / 0.45)', strokeWidth: 1.0 }
}

// ─── State ──────────────────────────────────────────────────────
const loading = ref(false)
const errored = ref(false)
const flowNodes = ref<any[]>([])
const flowEdges = ref<any[]>([])
const rawNodeCount = ref(0)

// Unique id per instance so multiple graphs on the same page don't clash.
const instanceId = `dep-graph-${Math.random().toString(36).slice(2, 8)}`
const { fitView } = useVueFlow(instanceId)

// Tiny computed helpers drive loading / empty / error state rendering.
const hasGraph = computed(() => flowNodes.value.length > 1)
const isEmpty = computed(() => !loading.value && !errored.value && flowNodes.value.length <= 1)

// Custom node types registered with VueFlow. Dependency nodes vs the
// generic default — lets us render a compact card with icon + badge.
const nodeTypes = { depnode: DepNode }

// ─── Dagre layout ───────────────────────────────────────────────
// Node dimensions reported to dagre must match the rendered card so
// edge anchoring lines up with the actual box.
const NODE_W = 220
const NODE_H = 56

function runDagre(
  nodes: EntityGraphNode[],
  edges: EntityGraphEdge[],
): Map<string, { x: number; y: number }> {
  const g = new dagre.graphlib.Graph()
  g.setGraph({
    rankdir: 'LR',
    nodesep: 18,      // tight vertical packing between nodes in same column
    ranksep: 110,     // generous horizontal gap so edge routing reads cleanly
    marginx: 24,
    marginy: 24,
  })
  g.setDefaultEdgeLabel(() => ({}))

  for (const n of nodes) g.setNode(n.id, { width: NODE_W, height: NODE_H })
  for (const e of edges) g.setEdge(e.from_id, e.to_id)
  dagre.layout(g)

  const positions = new Map<string, { x: number; y: number }>()
  for (const n of nodes) {
    const p = g.node(n.id)
    // dagre gives centre coords; VueFlow uses top-left anchor.
    positions.set(n.id, { x: p.x - NODE_W / 2, y: p.y - NODE_H / 2 })
  }
  return positions
}

// ─── Build VueFlow graph ────────────────────────────────────────
function buildGraph(graph: EntityGraph, centerId: string) {
  if (graph.nodes.length === 0) {
    return { nodes: [], edges: [] }
  }

  const positions = runDagre(graph.nodes, graph.edges)

  const vueFlowNodes = graph.nodes.map((n) => {
    const p = positions.get(n.id) || { x: 0, y: 0 }
    const isCenter = n.id === centerId
    return {
      id: n.id,
      type: 'depnode',
      position: p,
      draggable: true,
      sourcePosition: Position.Right,
      targetPosition: Position.Left,
      data: {
        name: n.name,
        kind: n.kind,
        lifecycle: n.lifecycle,
        isCenter,
      },
    }
  })

  const vueFlowEdges = graph.edges.map((e, idx) => {
    const style = getEdgeStyle(e.relation_type)
    return {
      id: `${e.from_id}->${e.to_id}:${e.relation_type}:${idx}`,
      source: e.from_id,
      target: e.to_id,
      type: 'smoothstep',
      label: e.relation_type.replace(/_/g, ' '),
      animated: e.relation_type === 'provides' || e.relation_type === 'depends_on' || e.relation_type === 'consumes',
      style,
      labelStyle: { fontSize: '9px', fill: 'hsl(var(--muted-foreground) / 0.9)' },
      labelBgStyle: { fill: 'hsl(var(--panel) / 0.92)', rx: 3 },
      labelBgPadding: [4, 2] as [number, number],
    }
  })

  return { nodes: vueFlowNodes, edges: vueFlowEdges }
}

// ─── API ────────────────────────────────────────────────────────
async function fetchGraph() {
  loading.value = true
  errored.value = false
  try {
    const graph = await api.get<EntityGraph>(
      `/api/catalog/entities/${props.entityId}/graph?depth=${props.depth}`,
    )
    rawNodeCount.value = graph.nodes.length
    const { nodes, edges } = buildGraph(graph, props.entityId)
    flowNodes.value = nodes
    flowEdges.value = edges
    // Give VueFlow a tick to mount before we frame the view.
    nextTick(() => {
      setTimeout(() => {
        try { fitView({ padding: 0.22, minZoom: 0.4, maxZoom: 1.4 }) } catch { /* noop */ }
      }, 120)
    })
  } catch {
    errored.value = true
    flowNodes.value = []
    flowEdges.value = []
  } finally {
    loading.value = false
  }
}

onMounted(fetchGraph)

watch(() => [props.entityId, props.depth], () => {
  fetchGraph()
})

// ─── Interactivity ──────────────────────────────────────────────
// Click a non-center node. Components → deep-link to /services/:id.
// Other kinds → jump to the catalog detail page so we don't invent a
// new navigation surface.
function onNodeClick(payload: { event: MouseEvent; node: { id: string; data?: any } }) {
  const { node } = payload
  if (node.id === props.entityId) return
  const kind = node.data?.kind
  if (kind === 'component') {
    router.push(`/services/${node.id}`)
  } else {
    router.push(`/catalog/${node.id}`)
  }
}
</script>

<script lang="ts">
/**
 * DepNode — local custom node renderer. Compact card: icon + name + kind
 * badge, with an outline ring + glow on the center node so it reads as
 * "this is where I am" at a glance.
 *
 * Kept in this file (not a separate .vue) to honour the 4-file limit the
 * dispatch brief enforces.
 */
import { defineComponent, computed, h } from 'vue'
import { Handle, Position } from '@vue-flow/core'
import { Layers, Box, Cloud, Plug, Users2, Package } from 'lucide-vue-next'

const iconMap: Record<string, any> = {
  system: Layers,
  component: Box,
  api: Plug,
  resource: Cloud,
  group: Users2,
}

const nodeKindStyle: Record<string, { bg: string; border: string; text: string }> = {
  system:    { bg: 'hsl(280 60% 70% / 0.14)', border: 'hsl(280 60% 70% / 0.8)', text: 'hsl(280 60% 88%)' },
  component: { bg: 'hsl(170 70% 60% / 0.14)', border: 'hsl(170 70% 60% / 0.8)', text: 'hsl(170 70% 85%)' },
  api:       { bg: 'hsl(35 90% 60% / 0.14)',  border: 'hsl(35 90% 60% / 0.8)',  text: 'hsl(35 90% 85%)' },
  resource:  { bg: 'hsl(235 75% 70% / 0.14)', border: 'hsl(235 75% 70% / 0.8)', text: 'hsl(235 75% 88%)' },
  group:     { bg: 'hsl(225 10% 55% / 0.14)', border: 'hsl(225 10% 55% / 0.8)', text: 'hsl(225 10% 85%)' },
}

export const DepNode = defineComponent({
  name: 'DepNode',
  props: {
    data: {
      type: Object as () => {
        name: string
        kind: string
        lifecycle?: string
        isCenter?: boolean
      },
      required: true,
    },
  },
  setup(props) {
    const style = computed(() => nodeKindStyle[props.data.kind] || nodeKindStyle.group)
    const Icon = computed(() => iconMap[props.data.kind] || Package)
    return { style, Icon }
  },
  render() {
    const isCenter = !!this.data.isCenter
    const style = this.style
    // Outer container: flex card with subtle tint + border. Center node
    // gets a bright ring (double border trick via boxShadow + inset).
    return h(
      'div',
      {
        class: ['dep-node', isCenter ? 'dep-node-center' : ''],
        style: {
          background: style.bg,
          border: `1.5px solid ${style.border}`,
          color: style.text,
          borderRadius: '10px',
          padding: '8px 12px',
          width: `${220}px`,
          minHeight: `${56}px`,
          fontSize: '12px',
          boxShadow: isCenter
            ? `0 0 0 2px ${style.border}, 0 0 20px ${style.bg}`
            : 'none',
          display: 'flex',
          alignItems: 'center',
          gap: '10px',
        },
      },
      [
        // Invisible handles on all 4 sides so dagre LR and manual drag both
        // route edges sensibly.
        h(Handle, { type: 'target', position: Position.Left, class: 'dep-node-handle' }),
        h(Handle, { type: 'source', position: Position.Right, class: 'dep-node-handle' }),
        // Icon chip
        h(
          'div',
          {
            style: {
              flexShrink: 0,
              width: '28px',
              height: '28px',
              borderRadius: '6px',
              background: style.border,
              opacity: 0.18,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
            },
          },
        ),
        h(
          'div',
          {
            style: {
              flexShrink: 0,
              position: 'absolute',
              left: '18px',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              width: '24px',
              height: '24px',
              color: style.border,
            },
          },
          [h(this.Icon, { size: 16 })],
        ),
        // Text block
        h(
          'div',
          {
            style: {
              minWidth: 0,
              flex: 1,
              marginLeft: '22px',
            },
          },
          [
            h(
              'div',
              {
                style: {
                  fontSize: isCenter ? '12px' : '11.5px',
                  fontWeight: isCenter ? 600 : 500,
                  lineHeight: '1.2',
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                },
              },
              this.data.name,
            ),
            h(
              'div',
              {
                style: {
                  marginTop: '2px',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '6px',
                },
              },
              [
                h(
                  'span',
                  {
                    style: {
                      fontSize: '9px',
                      textTransform: 'uppercase',
                      letterSpacing: '0.08em',
                      opacity: 0.85,
                    },
                  },
                  this.data.kind,
                ),
                isCenter
                  ? h(
                      'span',
                      {
                        class: 'dep-node-center-pill',
                        style: {
                          fontSize: '9px',
                          padding: '1px 6px',
                          borderRadius: '999px',
                          background: 'hsl(var(--primary) / 0.2)',
                          color: 'hsl(var(--primary))',
                          fontWeight: 600,
                        },
                      },
                      '',
                    )
                  : null,
              ],
            ),
          ],
        ),
      ],
    )
  },
})

// Helper used inside the MiniMap callback — solid fill colour per kind,
// separate from the ring/bg palette above because MiniMap wants a single
// opaque colour it can render at ~10% scale.
export function nodeKindStyleResolve(kind: string): string {
  switch (kind) {
    case 'system':    return 'hsl(280 60% 70%)'
    case 'component': return 'hsl(170 70% 60%)'
    case 'api':       return 'hsl(35 90% 60%)'
    case 'resource':  return 'hsl(235 75% 70%)'
    case 'group':     return 'hsl(225 10% 55%)'
    default:          return 'hsl(225 10% 55%)'
  }
}
</script>

<template>
  <div class="dep-graph-container">
    <!-- Header: title + refresh + count -->
    <div class="flex items-center justify-between gap-2 px-1 pb-2">
      <div class="flex items-center gap-2 text-[11px] text-muted-foreground">
        <span class="uppercase tracking-wider font-semibold">{{ t('services.dependenciesGraph.title') }}</span>
        <Badge v-if="rawNodeCount > 0" variant="secondary" class="text-[9px]">
          {{ t('services.dependenciesGraph.nodeCount', { n: rawNodeCount }) }}
        </Badge>
      </div>

      <div class="flex items-center gap-1.5">
        <!-- Tiny legend so the user can decode edge colour at a glance -->
        <div class="hidden md:flex items-center gap-2 text-[10px]">
          <span class="inline-flex items-center gap-1">
            <span class="h-0.5 w-4" style="background: hsl(var(--success) / 0.75)" />
            <span class="text-muted-foreground">{{ t('services.dependenciesGraph.legendProvides') }}</span>
          </span>
          <span class="inline-flex items-center gap-1">
            <span class="h-0.5 w-4" style="background: hsl(235 75% 70% / 0.8)" />
            <span class="text-muted-foreground">{{ t('services.dependenciesGraph.legendDepends') }}</span>
          </span>
          <span class="inline-flex items-center gap-1">
            <span class="h-0.5 w-4" style="background: hsl(var(--warning) / 0.75)" />
            <span class="text-muted-foreground">{{ t('services.dependenciesGraph.legendDeployed') }}</span>
          </span>
          <span class="inline-flex items-center gap-1">
            <span class="h-0.5 w-4" style="background: hsl(var(--muted-foreground) / 0.55)" />
            <span class="text-muted-foreground">{{ t('services.dependenciesGraph.legendOwns') }}</span>
          </span>
        </div>
        <Button
          variant="ghost"
          size="sm"
          class="h-7 w-7 p-0"
          :disabled="loading"
          @click="fetchGraph"
        >
          <RefreshCw class="h-3 w-3" :class="{ 'animate-spin': loading }" />
        </Button>
      </div>
    </div>

    <!-- Canvas: fixed 480px so the tab doesn't push the page layout -->
    <div class="relative rounded-lg bg-panel/50 overflow-hidden dep-graph-canvas">
      <!-- Loading state: skeleton node placeholders with shimmer -->
      <div
        v-if="loading && !hasGraph"
        class="absolute inset-0 flex items-center justify-center gap-6 px-10"
      >
        <div
          v-for="n in 3"
          :key="n"
          class="h-14 w-44 rounded-md bg-muted/30 animate-pulse"
          :style="{ animationDelay: `${(n - 1) * 150}ms` }"
        />
      </div>

      <!-- Error state: concise + retry -->
      <div
        v-else-if="errored"
        class="absolute inset-0 flex flex-col items-center justify-center gap-2 p-6 text-center"
      >
        <Layers class="h-8 w-8 text-muted-foreground/40" />
        <p class="text-xs text-muted-foreground">{{ t('services.dependenciesGraph.error') }}</p>
        <Button variant="outline" size="sm" class="mt-1 h-7" @click="fetchGraph">
          <RefreshCw class="h-3 w-3" />
          {{ t('services.dependenciesGraph.retry') }}
        </Button>
      </div>

      <!-- Empty state: friendly pointer to the inferer -->
      <div
        v-else-if="isEmpty"
        class="absolute inset-0 flex flex-col items-center justify-center gap-2 p-8 text-center"
      >
        <Layers class="h-8 w-8 text-muted-foreground/40" />
        <p class="text-xs text-muted-foreground max-w-md leading-relaxed">
          {{ t('services.dependenciesGraph.empty') }}
        </p>
      </div>

      <!-- Graph -->
      <VueFlow
        v-else
        :id="instanceId"
        :nodes="flowNodes"
        :edges="flowEdges"
        :node-types="nodeTypes"
        :default-viewport="{ x: 0, y: 0, zoom: 0.9 }"
        :min-zoom="0.2"
        :max-zoom="2.0"
        :nodes-connectable="false"
        :pan-on-drag="true"
        :zoom-on-scroll="true"
        :fit-view-on-init="false"
        class="dep-graph"
        @node-click="onNodeClick"
      >
        <MiniMap
          :node-color="(n: any) => {
            // MiniMap renders to canvas → resolve the actual colour now.
            const kind = n.data?.kind
            const s = nodeKindStyleResolve(kind)
            return s
          }"
          :mask-color="'hsl(var(--muted-foreground) / 0.2)'"
          pannable
          zoomable
          class="!bg-panel/80 !border !border-border !rounded-lg"
        />
        <Controls
          :show-interactive="false"
          class="!bg-panel/80 !border !border-border !rounded-lg"
        />
      </VueFlow>
    </div>
  </div>
</template>

<style>
/* Fixed height — the whole point is the graph stays inside the tab.
   Kept UNSCOPED on purpose: the `.dep-graph .vue-flow__*` selectors below
   reach into VueFlow's internal DOM (edges, controls, minimap, handles).
   Scoping would append a `[data-v-hash]` attribute and kill the reach,
   reverting VueFlow to its default bright-light theme. */
.dep-graph-container .dep-graph-canvas {
  height: 480px;
}

.dep-graph .vue-flow__background { background: transparent; }

.dep-graph .vue-flow__edge-path {
  stroke-opacity: 0.85;
}
.dep-graph .vue-flow__edge-text {
  font-size: 9px;
  fill: hsl(var(--muted-foreground) / 0.9);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
.dep-graph .vue-flow__edge-textbg {
  fill: hsl(var(--panel) / 0.92);
  rx: 3;
}

.dep-graph .vue-flow__node {
  cursor: pointer;
  transition: transform 120ms ease;
}
.dep-graph .vue-flow__node:hover {
  transform: translateY(-1px);
}

.dep-graph .vue-flow__controls-button {
  background: hsl(var(--panel) / 0.85) !important;
  border-color: hsl(var(--border)) !important;
  color: hsl(var(--foreground) / 0.85) !important;
}
.dep-graph .vue-flow__controls-button:hover {
  background: hsl(var(--panel) / 0.95) !important;
  color: hsl(var(--primary)) !important;
}
.dep-graph .vue-flow__controls-button svg {
  fill: currentColor;
}

.dep-graph .vue-flow__minimap {
  background: hsl(var(--panel) / 0.85) !important;
  border: 1px solid hsl(var(--border)) !important;
  border-radius: 8px;
}
.dep-graph .vue-flow__minimap-mask {
  fill: hsl(var(--primary) / 0.18);
  stroke: hsl(var(--primary) / 0.45);
  stroke-width: 2;
}

.dep-graph .dep-node-handle {
  width: 6px !important;
  height: 6px !important;
  background: hsl(var(--muted) / 0.8) !important;
  border: 1px solid hsl(var(--border)) !important;
}

/* Provides/depends/consumes edges get a slow glow so the user can spot
   the "live runtime-ish" arrows without a noisy blink. */
.dep-graph .vue-flow__edge.animated path {
  animation: depEdgePulse 2.6s ease-in-out infinite;
}
@keyframes depEdgePulse {
  0%, 100% { stroke-opacity: 0.55; }
  50%      { stroke-opacity: 0.95; }
}
</style>
