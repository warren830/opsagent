<script setup lang="ts">
import { computed } from 'vue'
import type { HealthStatus } from '@/composables/useServicesOverview'

const props = withDefaults(defineProps<{
  status: HealthStatus
  size?: 'sm' | 'md'
  pulse?: boolean
}>(), {
  size: 'md',
  pulse: true,
})

const dimension = computed(() => (props.size === 'sm' ? 'h-1.5 w-1.5' : 'h-2 w-2'))

const classes = computed(() => {
  switch (props.status) {
    case 'healthy':
      return 'bg-success shadow-[0_0_4px_hsl(var(--success)/0.6)]'
    case 'warning':
      return 'bg-warning shadow-[0_0_4px_hsl(var(--warning)/0.6)]'
    case 'critical':
      return `bg-destructive shadow-[0_0_4px_hsl(var(--destructive)/0.8)]${props.pulse ? ' animate-pulse' : ''}`
    default:
      return 'bg-muted-foreground/40'
  }
})
</script>

<template>
  <span
    :class="['inline-block rounded-full shrink-0', dimension, classes]"
    :title="status"
    :aria-label="`health ${status}`"
  />
</template>
