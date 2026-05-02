<script setup lang="ts">
/**
 * TopologyGraph — visualises a catalog entity's neighbourhood up to N hops.
 *
 * Uses VueFlow (same stack as /pages/topology) rather than ECharts so we
 * keep the bundle lean and get free pan/zoom/minimap/touch support. Node
 * styling follows the Aurora palette: each `kind` maps to a semantic
 * token colour so the graph adapts to light + dark automatically.
 */
import { ref, watch, nextTick, onMounted, computed } from 'vue'
import { VueFlow, useVueFlow, Position } from '@vue-flow/core'
import { MiniMap } from '@vue-flow/minimap'
import { Controls } from '@vue-flow/controls'
import { Loader2, Layers } from 'lucide-vue-next'

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
interface CatalogEntity {
  id: string
  kind: string
  name: string
  display_name: string | null
  lifecycle: string
  tags: string[]
}

interface CatalogRelation {
  id: string
  from_id: string
  to_id: string
  relation_type: string
}

interface EntityGraph {
  nodes: CatalogEntity[]
  edges: CatalogRelation[]
}

// ─── Kind palette (Aurora tokens) ───────────────────────────────
// Pink-purple System, teal Component, amber API, indigo Resource, muted Group.
// Each kind gets a matching border + translucent fill so the graph reads
// cleanly on the near-black base.
const kindStyle: Record<string, { bg: string; border: string; text: string }> = {
  system:    { bg: 'hsl(280 60% 70% / 0.18)', border: 'hsl(280 60% 70% / 0.85)', text: 'hsl(280 60% 88%)' },
  component: { bg: 'hsl(170 70% 60% / 0.18)', border: 'hsl(170 70% 60% / 0.85)', text: 'hsl(170 70% 85%)' },
  api:       { bg: 'hsl(35 90% 60% / 0.18)',  border: 'hsl(35 90% 60% / 0.85)',  text: 'hsl(35 90% 85%)' },
  resource:  { bg: 'hsl(235 75% 70% / 0.18)', border: 'hsl(235 75% 70% / 0.85)', text: 'hsl(235 75% 88%)' },
  group:     { bg: 'hsl(225 10% 55% / 0.18)', border: 'hsl(225 10% 55% / 0.85)', text: 'hsl(225 10% 85%)' },
}

function getKindStyle(kind: string) {
  return kindStyle[kind] || kindStyle.group
}

// ─── State ──────────────────────────────────────────────────────
const loading = ref(false)
const flowNodes = ref<any[]>([])
const flowEdges = ref<any[]>([])
const instanceId = `catalog-graph-${Math.random().toString(36).slice(2, 8)}`
const { fitView } = useVueFlow(instanceId)

const isEmpty = computed(() => !loading.value && flowNodes.value.length <= 1)

// ─── Layout: center outwards ────────────────────────────────────
// Simple radial-ish layout — center node fixed, neighbours distributed on
// two concentric rings based on hop distance. Good enough for 2-3 hop
// graphs; anything denser gets cleaned up by VueFlow's minimap/controls.
function layoutGraph(graph: EntityGraph, centerId: string) {
  const nodeMap = new Map(graph.nodes.map(n => [n.id, n]))

  // Compute hop distance from center via BFS.
  const adjacency = new Map<string, Set<string>>()
  for (const n of graph.nodes) adjacency.set(n.id, new Set())
  for (const e of graph.edges) {
    adjacency.get(e.from_id)?.add(e.to_id)
    adjacency.get(e.to_id)?.add(e.from_id)
  }
  const dist = new Map<string, number>()
  dist.set(centerId, 0)
  const queue: string[] = [centerId]
  while (queue.length > 0) {
    const cur = queue.shift()!
    const d = dist.get(cur)!
    for (const nb of adjacency.get(cur) || []) {
      if (!dist.has(nb)) {
        dist.set(nb, d + 1)
        queue.push(nb)
      }
    }
  }

  // Group nodes by hop distance so each ring places them evenly.
  const ringGroups = new Map<number, string[]>()
  for (const [id, d] of dist) {
    if (!ringGroups.has(d)) ringGroups.set(d, [])
    ringGroups.get(d)!.push(id)
  }

  const nodes: any[] = []
  const CENTER_X = 400
  const CENTER_Y = 280
  const RING_RADIUS = [0, 180, 340, 500, 660]

  for (const [ring, ids] of ringGroups) {
    const radius = RING_RADIUS[ring] ?? 660
    const count = ids.length
    ids.forEach((id, idx) => {
      const entity = nodeMap.get(id)
      if (!entity) return

      // Center goes dead centre; outer rings spread evenly around the circle.
      let x = CENTER_X
      let y = CENTER_Y
      if (ring > 0) {
        const angle = (idx / count) * Math.PI * 2 - Math.PI / 2
        x = CENTER_X + Math.cos(angle) * radius
        y = CENTER_Y + Math.sin(angle) * radius
      }

      const style = getKindStyle(entity.kind)
      const isCenter = id === centerId
      const size = isCenter ? 72 : 56

      nodes.push({
        id,
        type: 'default',
        position: { x: x - size / 2, y: y - size / 2 },
        sourcePosition: Position.Right,
        targetPosition: Position.Left,
        draggable: true,
        data: {
          label: entity.display_name || entity.name,
        },
        style: {
          background: style.bg,
          border: `1.5px solid ${style.border}`,
          color: style.text,
          borderRadius: '10px',
          width: `${size + 80}px`,
          minHeight: `${size}px`,
          fontSize: isCenter ? '12px' : '11px',
          fontWeight: isCenter ? '600' : '500',
          padding: '8px 10px',
          textAlign: 'center',
          boxShadow: isCenter ? `0 0 0 2px ${style.border}, 0 0 24px ${style.bg}` : 'none',
        },
      })
    })
  }

  const edges: any[] = graph.edges.map(e => ({
    id: e.id,
    source: e.from_id,
    target: e.to_id,
    label: e.relation_type.replace(/_/g, ' '),
    animated: true,
    style: { stroke: 'hsl(var(--primary) / 0.55)', strokeWidth: 1.2 },
    labelStyle: { fontSize: '9px', fill: 'hsl(var(--muted-foreground) / 0.85)' },
    labelBgStyle: { fill: 'hsl(var(--panel) / 0.9)' },
  }))

  return { nodes, edges }
}

// ─── API ────────────────────────────────────────────────────────
async function fetchGraph() {
  loading.value = true
  try {
    const graph = await api.get<EntityGraph>(
      `/api/catalog/entities/${props.entityId}/graph?depth=${props.depth}`,
    )
    const { nodes, edges } = layoutGraph(graph, props.entityId)
    flowNodes.value = nodes
    flowEdges.value = edges
    nextTick(() => { setTimeout(() => fitView({ padding: 0.2 }), 150) })
  } catch {
    flowNodes.value = []
    flowEdges.value = []
  } finally {
    loading.value = false
  }
}

onMounted(fetchGraph)
watch(() => [props.entityId, props.depth], fetchGraph)

// ─── Click-through navigation ──────────────────────────────────
function onNodeClick(_event: unknown, node: { id: string }) {
  if (node.id === props.entityId) return
  router.push(`/catalog/${node.id}`)
}
</script>

<template>
  <div class="relative w-full" style="height: 520px;">
    <!-- Loading overlay -->
    <div
      v-if="loading"
      class="absolute inset-0 flex items-center justify-center z-10 bg-panel/40"
    >
      <div class="flex items-center gap-2 text-xs text-muted-foreground">
        <Loader2 class="h-4 w-4 animate-spin" />
        {{ t('catalog.graphLoading') }}
      </div>
    </div>

    <!-- Empty state -->
    <div
      v-else-if="isEmpty"
      class="flex flex-col items-center justify-center h-full gap-2 text-xs text-muted-foreground"
    >
      <Layers class="h-8 w-8 text-muted-foreground/40" />
      <p>{{ t('catalog.graphEmpty', { depth }) }}</p>
    </div>

    <VueFlow
      v-else
      :id="instanceId"
      :nodes="flowNodes"
      :edges="flowEdges"
      :default-viewport="{ x: 0, y: 0, zoom: 0.9 }"
      :min-zoom="0.2"
      :max-zoom="2.5"
      :nodes-connectable="false"
      fit-view-on-init
      class="catalog-graph"
      @node-click="onNodeClick"
    >
      <MiniMap
        pannable
        zoomable
        class="!bg-panel/80 !border !border-border !rounded-lg"
      />
      <Controls class="!bg-panel/80 !border !border-border !rounded-lg" />
    </VueFlow>
  </div>
</template>

<style>
.catalog-graph .vue-flow__background { background: transparent; }
.catalog-graph .vue-flow__edge-path { stroke-opacity: 0.55; }
.catalog-graph .vue-flow__edge-text {
  font-size: 9px;
  fill: hsl(var(--muted-foreground) / 0.85);
}
.catalog-graph .vue-flow__edge-textbg {
  fill: hsl(var(--panel) / 0.9);
  rx: 3;
}
.catalog-graph .vue-flow__node {
  cursor: pointer;
  transition: transform 120ms ease;
}
.catalog-graph .vue-flow__node:hover {
  transform: translateY(-1px);
}
</style>
