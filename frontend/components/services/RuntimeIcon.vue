<script setup lang="ts">
import { computed } from 'vue'
import {
  Container, Server, Database, FunctionSquare, Globe, Box,
} from 'lucide-vue-next'
import type { RuntimeKind } from '@/composables/useServicesOverview'

const props = withDefaults(defineProps<{
  kind: RuntimeKind | null
  size?: 'sm' | 'md'
}>(), {
  size: 'md',
})

// Explicit per-runtime palette — matches design spec §4.7.
const palette: Record<RuntimeKind, { icon: unknown, color: string, bg: string }> = {
  eks:      { icon: Container,       color: 'text-info',     bg: 'bg-info/10' },
  ec2:      { icon: Server,          color: 'text-orange-400', bg: 'bg-orange-400/10' },
  rds:      { icon: Database,        color: 'text-purple-400', bg: 'bg-purple-400/10' },
  lambda:   { icon: FunctionSquare,  color: 'text-yellow-400', bg: 'bg-yellow-400/10' },
  external: { icon: Globe,           color: 'text-muted-foreground', bg: 'bg-muted/40' },
  generic:  { icon: Box,             color: 'text-muted-foreground', bg: 'bg-muted/40' },
}

const config = computed(() => palette[props.kind ?? 'generic'] ?? palette.generic)
const iconSize = computed(() => (props.size === 'sm' ? 'h-3 w-3' : 'h-3.5 w-3.5'))
const boxSize = computed(() => (props.size === 'sm' ? 'h-5 w-5' : 'h-6 w-6'))
</script>

<template>
  <div
    class="rounded-md flex items-center justify-center shrink-0"
    :class="[config.bg, boxSize]"
  >
    <component :is="config.icon" :class="[config.color, iconSize]" />
  </div>
</template>
