<script setup lang="ts">
import { Handle, Position } from '@vue-flow/core'
import { Globe, Unplug, Box, Rocket } from 'lucide-vue-next'

const props = defineProps<{
  data: {
    label: string
    subtitle?: string
    kind: string        // "ingress" | "service" | "deployment" | "rollout"
    status: string      // "healthy" | "warning" | "critical" | "unknown"
    replicas?: string
    namespace: string
    cluster: string
  }
}>()

// Light-theme node palette: subtle tinted surface + saturated icon + clear border
const kindConfig: Record<string, { icon: any; border: string; tint: string; iconBg: string; iconFg: string }> = {
  ingress:    { icon: Globe,  border: 'border-violet-300',  tint: 'from-violet-50/80',   iconBg: 'bg-violet-100',  iconFg: 'text-violet-600' },
  service:    { icon: Unplug, border: 'border-sky-300',     tint: 'from-sky-50/80',      iconBg: 'bg-sky-100',     iconFg: 'text-sky-600' },
  deployment: { icon: Box,    border: 'border-emerald-300', tint: 'from-emerald-50/80',  iconBg: 'bg-emerald-100', iconFg: 'text-emerald-600' },
  rollout:    { icon: Rocket, border: 'border-amber-300',   tint: 'from-amber-50/80',    iconBg: 'bg-amber-100',   iconFg: 'text-amber-600' },
}

const statusDot: Record<string, string> = {
  healthy:  'bg-emerald-500 shadow-emerald-500/50',
  warning:  'bg-amber-500 shadow-amber-500/50',
  critical: 'bg-red-500 shadow-red-500/50 animate-pulse',
  unknown:  'bg-slate-400 shadow-slate-400/40',
}

const config = computed(() => kindConfig[props.data.kind] || kindConfig.deployment)
const dot = computed(() => statusDot[props.data.status] || statusDot.unknown)
</script>

<template>
  <div
    class="group relative rounded-lg border px-3 py-2 min-w-[130px] max-w-[180px] transition-all duration-200 hover:-translate-y-0.5 hover:shadow-lg cursor-pointer bg-white/85 backdrop-blur-md bg-gradient-to-br to-white/95 shadow-[0_4px_14px_rgba(100,140,200,0.08)]"
    :class="[config.border, config.tint]"
  >
    <!-- Handles (light, barely visible dots) -->
    <Handle type="target" :position="Position.Left" class="!w-2 !h-2 !bg-slate-300 !border-white" />
    <Handle type="source" :position="Position.Right" class="!w-2 !h-2 !bg-slate-300 !border-white" />
    <Handle type="target" :position="Position.Top" class="!w-2 !h-2 !bg-slate-300 !border-white" />
    <Handle type="source" :position="Position.Bottom" class="!w-2 !h-2 !bg-slate-300 !border-white" />

    <!-- Status dot -->
    <div
      class="absolute -top-1 -right-1 h-2.5 w-2.5 rounded-full shadow-[0_0_6px]"
      :class="dot"
    />

    <!-- Content -->
    <div class="flex items-center gap-2">
      <div class="shrink-0 rounded-md p-1" :class="config.iconBg">
        <component :is="config.icon" class="h-3.5 w-3.5" :class="config.iconFg" />
      </div>
      <div class="min-w-0 flex-1">
        <div class="text-[11px] font-semibold text-slate-900 truncate leading-tight">{{ data.label }}</div>
        <div v-if="data.subtitle" class="text-[9px] text-slate-500 truncate leading-tight">{{ data.subtitle }}</div>
      </div>
    </div>

    <!-- Replicas tag -->
    <div v-if="data.replicas" class="mt-1 text-[8px] font-mono px-1 py-0.5 rounded bg-slate-100 text-slate-500 text-center tabular-nums">
      {{ data.replicas }}
    </div>
  </div>
</template>
