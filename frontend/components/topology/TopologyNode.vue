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

// Token-driven node palette: subtle tinted surface + saturated icon + clear border.
// Adapts to both light (Sky & Lavender) and aurora (dark) themes via CSS vars.
const kindConfig: Record<string, { icon: any; border: string; tint: string; iconBg: string; iconFg: string }> = {
  ingress:    { icon: Globe,  border: 'border-ai/40',      tint: 'from-ai/10',      iconBg: 'bg-ai/15',       iconFg: 'text-ai' },
  service:    { icon: Unplug, border: 'border-info/40',    tint: 'from-info/10',    iconBg: 'bg-info/15',     iconFg: 'text-info' },
  deployment: { icon: Box,    border: 'border-success/40', tint: 'from-success/10', iconBg: 'bg-success/15',  iconFg: 'text-success' },
  rollout:    { icon: Rocket, border: 'border-warning/40', tint: 'from-warning/10', iconBg: 'bg-warning/15',  iconFg: 'text-warning' },
}

const statusDot: Record<string, string> = {
  healthy:  'bg-success shadow-emerald-500/50',
  warning:  'bg-warning shadow-amber-500/50',
  critical: 'bg-destructive shadow-red-500/50 animate-pulse',
  unknown:  'bg-muted shadow-slate-400/40',
}

const config = computed(() => kindConfig[props.data.kind] || kindConfig.deployment)
const dot = computed(() => statusDot[props.data.status] || statusDot.unknown)
</script>

<template>
  <div
    class="group relative rounded-lg border px-3 py-2 min-w-[130px] max-w-[180px] transition-all duration-200 hover:-translate-y-0.5 hover:shadow-lg cursor-pointer bg-panel/$1 backdrop-blur-md bg-gradient-to-br to-white/95 shadow-[0_4px_14px_rgba(100,140,200,0.08)]"
    :class="[config.border, config.tint]"
  >
    <!-- Handles (light, barely visible dots) -->
    <Handle type="target" :position="Position.Left" class="!w-2 !h-2 !bg-muted !border-border" />
    <Handle type="source" :position="Position.Right" class="!w-2 !h-2 !bg-muted !border-border" />
    <Handle type="target" :position="Position.Top" class="!w-2 !h-2 !bg-muted !border-border" />
    <Handle type="source" :position="Position.Bottom" class="!w-2 !h-2 !bg-muted !border-border" />

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
        <div class="text-[11px] font-semibold text-foreground truncate leading-tight">{{ data.label }}</div>
        <div v-if="data.subtitle" class="text-[9px] text-muted-foreground truncate leading-tight">{{ data.subtitle }}</div>
      </div>
    </div>

    <!-- Replicas tag -->
    <div v-if="data.replicas" class="mt-1 text-[8px] font-mono px-1 py-0.5 rounded bg-muted text-muted-foreground text-center tabular-nums">
      {{ data.replicas }}
    </div>
  </div>
</template>
