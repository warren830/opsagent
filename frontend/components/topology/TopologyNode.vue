<script setup lang="ts">
import { Handle, Position } from '@vue-flow/core'
import {
  Cloud, Server, Database, Shield, Network, HardDrive, Radio,
  Container, Search, Waypoints, MonitorCog, Cpu, Archive,
} from 'lucide-vue-next'

const props = defineProps<{
  data: {
    label: string
    subtitle?: string
    type: 'aws' | 'cisco' | 'idc'
    status: 'healthy' | 'warning' | 'critical'
    icon: string
    bandwidth?: string
  }
}>()

const iconMap: Record<string, any> = {
  Cloud, Server, Database, Shield, Network, HardDrive, Radio,
  Container, Search, Waypoints, MonitorCog, Cpu, Archive,
}

const icon = computed(() => iconMap[props.data.icon] || Server)

const colorMap = {
  aws: { border: 'border-blue-500/40', glow: 'shadow-blue-500/20', bg: 'bg-blue-500/5', text: 'text-blue-400', dot: 'bg-blue-400' },
  cisco: { border: 'border-orange-500/40', glow: 'shadow-orange-500/20', bg: 'bg-orange-500/5', text: 'text-orange-400', dot: 'bg-orange-400' },
  idc: { border: 'border-emerald-500/40', glow: 'shadow-emerald-500/20', bg: 'bg-emerald-500/5', text: 'text-emerald-400', dot: 'bg-emerald-400' },
}

const statusColor = {
  healthy: 'bg-emerald-400 shadow-emerald-400/60',
  warning: 'bg-amber-400 shadow-amber-400/60',
  critical: 'bg-red-400 shadow-red-400/60 animate-pulse',
}

const colors = computed(() => colorMap[props.data.type])
const statusDot = computed(() => statusColor[props.data.status])
</script>

<template>
  <div
    class="group relative rounded-lg border px-3 py-2 min-w-[140px] transition-all duration-300 hover:scale-105 cursor-pointer"
    :class="[
      colors.border,
      colors.bg,
      `shadow-lg ${colors.glow}`,
      'hover:shadow-xl',
    ]"
    style="background: rgba(17, 18, 23, 0.9); backdrop-filter: blur(8px);"
  >
    <!-- Handles -->
    <Handle type="target" :position="Position.Left" class="!w-2 !h-2 !bg-white/20 !border-white/30" />
    <Handle type="source" :position="Position.Right" class="!w-2 !h-2 !bg-white/20 !border-white/30" />
    <Handle type="target" :position="Position.Top" class="!w-2 !h-2 !bg-white/20 !border-white/30" />
    <Handle type="source" :position="Position.Bottom" class="!w-2 !h-2 !bg-white/20 !border-white/30" />

    <!-- Status dot -->
    <div
      class="absolute -top-1 -right-1 h-2.5 w-2.5 rounded-full shadow-[0_0_6px]"
      :class="statusDot"
    />

    <!-- Content -->
    <div class="flex items-center gap-2">
      <div class="shrink-0 rounded-md p-1" :class="colors.bg">
        <component :is="icon" class="h-4 w-4" :class="colors.text" />
      </div>
      <div class="min-w-0">
        <div class="text-[11px] font-semibold text-foreground truncate leading-tight">{{ data.label }}</div>
        <div v-if="data.subtitle" class="text-[9px] text-muted-foreground/70 truncate font-mono leading-tight">{{ data.subtitle }}</div>
      </div>
    </div>

    <!-- Bandwidth tag -->
    <div v-if="data.bandwidth" class="mt-1 text-[8px] font-mono px-1 py-0.5 rounded bg-white/5 text-muted-foreground/50 text-center">
      {{ data.bandwidth }}
    </div>
  </div>
</template>
