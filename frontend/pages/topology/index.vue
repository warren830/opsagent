<script setup lang="ts">
import { VueFlow, useVueFlow } from '@vue-flow/core'
import { MiniMap } from '@vue-flow/minimap'
import { Controls } from '@vue-flow/controls'
import TopologyNode from '@/components/topology/TopologyNode.vue'

import '@vue-flow/core/dist/style.css'
import '@vue-flow/core/dist/theme-default.css'
import '@vue-flow/minimap/dist/style.css'
import '@vue-flow/controls/dist/style.css'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()

const nodeTypes = { topology: TopologyNode }

// ─── Layout constants (top-to-bottom, 3 tiers) ─────────────────
// Tier 1: AWS (y=0..200)    Tier 2: Cisco (y=300..440)    Tier 3: IDC (y=560..760)
const COL = { c1: 0, c2: 190, c3: 380, c4: 570, c5: 760 }

// ─── Zone backgrounds ──────────────────────────────────────────
const zoneNodes = [
  {
    id: 'zone-aws', type: 'group', position: { x: -30, y: -30 },
    style: { width: '960px', height: '230px', background: 'rgba(59,130,246,0.03)', border: '1px dashed rgba(59,130,246,0.15)', borderRadius: '16px' },
    data: { label: '' },
  },
  {
    id: 'zone-cisco', type: 'group', position: { x: 80, y: 280 },
    style: { width: '760px', height: '170px', background: 'rgba(249,115,22,0.03)', border: '1px dashed rgba(249,115,22,0.15)', borderRadius: '16px' },
    data: { label: '' },
  },
  {
    id: 'zone-idc', type: 'group', position: { x: -30, y: 530 },
    style: { width: '960px', height: '260px', background: 'rgba(16,185,129,0.03)', border: '1px dashed rgba(16,185,129,0.15)', borderRadius: '16px' },
    data: { label: '' },
  },
]

// ─── Tier 1: AWS Cloud ─────────────────────────────────────────
const awsNodes = [
  { id: 'vpc', type: 'topology', position: { x: COL.c1, y: 0 }, parentNode: 'zone-aws',
    data: { label: 'VPC', subtitle: '10.0.0.0/16', type: 'aws', status: 'healthy', icon: 'Cloud' } },
  { id: 'eks', type: 'topology', position: { x: COL.c2, y: 0 }, parentNode: 'zone-aws',
    data: { label: 'EKS Cluster', subtitle: '6 nodes · Graviton', type: 'aws', status: 'healthy', icon: 'Container' } },
  { id: 'rds', type: 'topology', position: { x: COL.c3, y: 0 }, parentNode: 'zone-aws',
    data: { label: 'Aurora PostgreSQL', subtitle: 'db.r6g.xlarge', type: 'aws', status: 'healthy', icon: 'Database' } },
  { id: 'opensearch', type: 'topology', position: { x: COL.c5, y: 0 }, parentNode: 'zone-aws',
    data: { label: 'OpenSearch', subtitle: '3 nodes · 2TB', type: 'aws', status: 'warning', icon: 'Search' } },
  { id: 'tgw', type: 'topology', position: { x: COL.c2, y: 110 }, parentNode: 'zone-aws',
    data: { label: 'Transit Gateway', subtitle: 'tgw-0a1b2c3d', type: 'aws', status: 'healthy', icon: 'Waypoints' } },
  { id: 'dx', type: 'topology', position: { x: COL.c3, y: 110 }, parentNode: 'zone-aws',
    data: { label: 'Direct Connect', subtitle: 'dxcon-abc · 10Gbps', type: 'aws', status: 'healthy', icon: 'Radio' } },
  { id: 'vpn-gw', type: 'topology', position: { x: COL.c4, y: 110 }, parentNode: 'zone-aws',
    data: { label: 'VPN Gateway', subtitle: 'vgw-9x8y7z', type: 'aws', status: 'healthy', icon: 'Shield' } },
]

// ─── Tier 2: Cisco Network ─────────────────────────────────────
const ciscoNodes = [
  { id: 'cisco-dx-port', type: 'topology', position: { x: 30, y: 30 }, parentNode: 'zone-cisco',
    data: { label: 'DX Port', subtitle: 'xe-0/0/1 · 10G', type: 'cisco', status: 'healthy', icon: 'Radio' } },
  { id: 'cisco-router', type: 'topology', position: { x: 220, y: 30 }, parentNode: 'zone-cisco',
    data: { label: 'ISR 4451-X', subtitle: '192.168.1.1', type: 'cisco', status: 'healthy', icon: 'Network' } },
  { id: 'cisco-switch', type: 'topology', position: { x: 410, y: 30 }, parentNode: 'zone-cisco',
    data: { label: 'Nexus 9300', subtitle: '192.168.1.2', type: 'cisco', status: 'healthy', icon: 'MonitorCog' } },
  { id: 'cisco-fw', type: 'topology', position: { x: 590, y: 30 }, parentNode: 'zone-cisco',
    data: { label: 'ASA 5525-X', subtitle: '192.168.1.254', type: 'cisco', status: 'healthy', icon: 'Shield' } },
]

// ─── Tier 3: IDC Datacenter ────────────────────────────────────
const idcNodes = [
  { id: 'idc-core-sw', type: 'topology', position: { x: COL.c2 + 95, y: 30 }, parentNode: 'zone-idc',
    data: { label: 'Catalyst 9500', subtitle: '172.16.0.1 · Core', type: 'idc', status: 'healthy', icon: 'Network' } },
  { id: 'idc-app1', type: 'topology', position: { x: COL.c1, y: 140 }, parentNode: 'zone-idc',
    data: { label: 'App Server #1', subtitle: '172.16.1.10 · R750', type: 'idc', status: 'healthy', icon: 'Server' } },
  { id: 'idc-app2', type: 'topology', position: { x: COL.c2, y: 140 }, parentNode: 'zone-idc',
    data: { label: 'App Server #2', subtitle: '172.16.1.11 · R750', type: 'idc', status: 'healthy', icon: 'Server' } },
  { id: 'idc-app3', type: 'topology', position: { x: COL.c3, y: 140 }, parentNode: 'zone-idc',
    data: { label: 'App Server #3', subtitle: '172.16.1.12 · R750', type: 'idc', status: 'healthy', icon: 'Server' } },
  { id: 'idc-db', type: 'topology', position: { x: COL.c4, y: 140 }, parentNode: 'zone-idc',
    data: { label: 'Oracle RAC', subtitle: '172.16.2.10 · 2-node', type: 'idc', status: 'critical', icon: 'Database' } },
  { id: 'idc-storage', type: 'topology', position: { x: COL.c5, y: 30 }, parentNode: 'zone-idc',
    data: { label: 'NetApp AFF A400', subtitle: '172.16.3.10 · 96TB', type: 'idc', status: 'healthy', icon: 'HardDrive' } },
  { id: 'idc-backup', type: 'topology', position: { x: COL.c5, y: 140 }, parentNode: 'zone-idc',
    data: { label: 'Veeam Backup', subtitle: '172.16.4.10', type: 'idc', status: 'healthy', icon: 'Archive' } },
]

const initialNodes = [...zoneNodes, ...awsNodes, ...ciscoNodes, ...idcNodes]

// ─── Edges ─────────────────────────────────────────────────────
const initialEdges = [
  // === AWS internal (horizontal, blue) ===
  { id: 'e-vpc-eks', source: 'vpc', target: 'eks', animated: true, style: { stroke: '#3b82f6', strokeWidth: 1.5 } },
  { id: 'e-eks-rds', source: 'eks', target: 'rds', animated: true, style: { stroke: '#3b82f6', strokeWidth: 1.5 } },
  { id: 'e-eks-os', source: 'eks', target: 'opensearch', animated: true, style: { stroke: '#3b82f680', strokeWidth: 1 }, label: 'logs' },
  { id: 'e-vpc-tgw', source: 'vpc', target: 'tgw', animated: true, style: { stroke: '#3b82f6', strokeWidth: 1.5 } },
  { id: 'e-tgw-dx', source: 'tgw', target: 'dx', animated: true, style: { stroke: '#3b82f6', strokeWidth: 2 } },
  { id: 'e-tgw-vpn', source: 'tgw', target: 'vpn-gw', style: { stroke: '#3b82f650', strokeWidth: 1, strokeDasharray: '6 4' } },

  // === AWS → Cisco (vertical, cross-zone, orange) ===
  { id: 'e-dx-ciscoDx', source: 'dx', target: 'cisco-dx-port', animated: true, style: { stroke: '#f59e0b', strokeWidth: 2.5 }, label: '10 Gbps DX' },
  { id: 'e-vpn-ciscoRouter', source: 'vpn-gw', target: 'cisco-router', style: { stroke: '#f59e0b60', strokeWidth: 1.5, strokeDasharray: '8 4' }, label: 'IPSec VPN (backup)' },

  // === Cisco internal (horizontal, orange) ===
  { id: 'e-cdx-cr', source: 'cisco-dx-port', target: 'cisco-router', animated: true, style: { stroke: '#f97316', strokeWidth: 2 } },
  { id: 'e-cr-cs', source: 'cisco-router', target: 'cisco-switch', animated: true, style: { stroke: '#f97316', strokeWidth: 1.5 } },
  { id: 'e-cs-cf', source: 'cisco-switch', target: 'cisco-fw', animated: true, style: { stroke: '#f97316', strokeWidth: 1.5 } },

  // === Cisco → IDC (vertical, cross-zone, green) ===
  { id: 'e-cf-idcCore', source: 'cisco-fw', target: 'idc-core-sw', animated: true, style: { stroke: '#10b981', strokeWidth: 2.5 }, label: '40 Gbps fiber' },

  // === IDC internal (green) ===
  { id: 'e-core-app1', source: 'idc-core-sw', target: 'idc-app1', animated: true, style: { stroke: '#10b981', strokeWidth: 1.5 } },
  { id: 'e-core-app2', source: 'idc-core-sw', target: 'idc-app2', animated: true, style: { stroke: '#10b981', strokeWidth: 1.5 } },
  { id: 'e-core-app3', source: 'idc-core-sw', target: 'idc-app3', animated: true, style: { stroke: '#10b981', strokeWidth: 1.5 } },
  { id: 'e-core-db', source: 'idc-core-sw', target: 'idc-db', animated: true, style: { stroke: '#10b98180', strokeWidth: 1 } },
  { id: 'e-core-storage', source: 'idc-core-sw', target: 'idc-storage', animated: true, style: { stroke: '#10b98180', strokeWidth: 1 } },
  { id: 'e-db-storage', source: 'idc-db', target: 'idc-storage', style: { stroke: '#10b98140', strokeWidth: 1, strokeDasharray: '4 4' }, label: 'SAN' },
  { id: 'e-storage-backup', source: 'idc-storage', target: 'idc-backup', style: { stroke: '#10b98140', strokeWidth: 1, strokeDasharray: '4 4' }, label: 'backup' },
]

// ─── Vue Flow ──────────────────────────────────────────────────
const { fitView } = useVueFlow({ id: 'topology' })

onMounted(() => {
  setTimeout(() => fitView({ padding: 0.12 }), 150)
})

const legend = [
  { color: 'bg-blue-400', label: 'AWS Cloud' },
  { color: 'bg-orange-400', label: 'Cisco Network' },
  { color: 'bg-emerald-400', label: 'IDC Datacenter' },
]

const statusLegend = [
  { color: 'bg-emerald-400', label: 'Healthy' },
  { color: 'bg-amber-400', label: 'Warning' },
  { color: 'bg-red-400 animate-pulse', label: 'Critical' },
]
</script>

<template>
  <div class="space-y-3">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-base font-semibold text-foreground">{{ t('topology.title') }}</h1>
        <p class="text-[11px] text-muted-foreground">AWS Cloud → Cisco Network → IDC Datacenter</p>
      </div>
      <!-- Legend -->
      <div class="flex items-center gap-4">
        <div v-for="item in legend" :key="item.label" class="flex items-center gap-1.5">
          <span class="h-2 w-2 rounded-full" :class="item.color" />
          <span class="text-[10px] text-muted-foreground">{{ item.label }}</span>
        </div>
        <div class="h-3 w-px bg-border/50" />
        <div v-for="item in statusLegend" :key="item.label" class="flex items-center gap-1.5">
          <span class="h-2 w-2 rounded-full shadow-[0_0_4px]" :class="item.color" />
          <span class="text-[10px] text-muted-foreground">{{ item.label }}</span>
        </div>
      </div>
    </div>

    <!-- Topology Canvas -->
    <div class="relative rounded-lg border border-border/60 bg-card/30 overflow-hidden" style="height: calc(100vh - 140px)">
      <!-- Zone tier labels -->
      <div class="absolute top-3 left-3 z-10 flex flex-col gap-1 pointer-events-none">
        <span class="text-[10px] font-bold uppercase tracking-widest text-blue-400/50 bg-blue-500/10 px-2 py-0.5 rounded w-fit">▲ AWS Cloud</span>
      </div>
      <div class="absolute top-[42%] left-3 z-10 pointer-events-none">
        <span class="text-[10px] font-bold uppercase tracking-widest text-orange-400/50 bg-orange-500/10 px-2 py-0.5 rounded">■ Cisco Network</span>
      </div>
      <div class="absolute bottom-3 left-3 z-10 pointer-events-none">
        <span class="text-[10px] font-bold uppercase tracking-widest text-emerald-400/50 bg-emerald-500/10 px-2 py-0.5 rounded">▼ IDC Datacenter</span>
      </div>

      <VueFlow
        id="topology"
        :nodes="initialNodes"
        :edges="initialEdges"
        :node-types="nodeTypes"
        :default-viewport="{ x: 80, y: 20, zoom: 0.78 }"
        :min-zoom="0.3"
        :max-zoom="2"
        :snap-to-grid="true"
        :snap-grid="[10, 10]"
        fit-view-on-init
        class="topology-flow"
      >
        <MiniMap
          :node-color="(n: any) => {
            const type = n.data?.type
            if (type === 'aws') return '#3b82f6'
            if (type === 'cisco') return '#f97316'
            if (type === 'idc') return '#10b981'
            return '#6b7280'
          }"
          :mask-color="'rgba(0,0,0,0.7)'"
          class="!bg-black/40 !border-border/30 !rounded-lg"
        />
        <Controls class="!bg-black/40 !border-border/30 !rounded-lg" />
      </VueFlow>
    </div>
  </div>
</template>

<style>
/* Vue Flow dark theme overrides */
.topology-flow .vue-flow__background {
  background: transparent;
}
.topology-flow .vue-flow__edge-path {
  filter: drop-shadow(0 0 2px currentColor);
}
.topology-flow .vue-flow__edge-text {
  font-size: 9px;
  fill: rgba(255,255,255,0.4);
}
.topology-flow .vue-flow__edge-textbg {
  fill: rgba(17, 18, 23, 0.8);
  rx: 3;
}
.topology-flow .vue-flow__controls-button {
  background: rgba(17, 18, 23, 0.8) !important;
  border-color: rgba(255,255,255,0.1) !important;
  color: rgba(255,255,255,0.5) !important;
}
.topology-flow .vue-flow__controls-button:hover {
  background: rgba(30, 32, 40, 0.9) !important;
  color: rgba(255,255,255,0.8) !important;
}
.topology-flow .vue-flow__minimap {
  background: rgba(0,0,0,0.5) !important;
}
/* Animated edge glow */
.topology-flow .vue-flow__edge.animated path {
  animation: edgeGlow 3s ease-in-out infinite;
}
@keyframes edgeGlow {
  0%, 100% { filter: drop-shadow(0 0 1px currentColor); opacity: 0.8; }
  50% { filter: drop-shadow(0 0 4px currentColor); opacity: 1; }
}
</style>
