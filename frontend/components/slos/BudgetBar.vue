<script setup lang="ts">
import { computed } from 'vue'

/**
 * Error-budget progress bar.
 *
 * `remainingPct` is a plain percent value (not a ratio) with the same
 * semantics the backend snapshot returns — positive = budget left,
 * 0 = fully spent, negative = over-budget.
 *
 * Three-color scale plus a "critically breached" mode that softly pulses
 * so an at-a-glance scan of the SLO list surfaces breached items without
 * relying on text alone.
 */
const props = defineProps<{
  remainingPct: number
  consumedMinutes?: number | null
  totalMinutes?: number | null
}>()

const clampedRemaining = computed(() => Math.max(0, Math.min(100, props.remainingPct ?? 0)))

const color = computed(() => {
  const r = props.remainingPct ?? 0
  if (r < 0) return 'bg-destructive'
  if (r < 20) return 'bg-destructive/90'
  if (r < 50) return 'bg-warning'
  return 'bg-success'
})

const pulse = computed(() => (props.remainingPct ?? 0) < 0)

const displayPct = computed(() => {
  const r = props.remainingPct ?? 0
  return r < 0 ? `${r.toFixed(1)}%` : `${r.toFixed(1)}%`
})

const minutesLabel = computed(() => {
  const c = props.consumedMinutes
  const t = props.totalMinutes
  if (c == null || t == null) return null
  return `${c.toFixed(1)} / ${t.toFixed(1)} min`
})
</script>

<template>
  <div class="space-y-1">
    <div class="flex items-center justify-between text-[11px]">
      <span class="text-muted-foreground">{{ displayPct }}</span>
      <span v-if="minutesLabel" class="font-mono text-muted-foreground/70">{{ minutesLabel }}</span>
    </div>
    <div class="relative h-1.5 w-full overflow-hidden rounded-full bg-secondary">
      <div
        class="h-full transition-all duration-300"
        :class="[color, pulse ? 'animate-pulse' : '']"
        :style="{ width: `${clampedRemaining}%` }"
      />
    </div>
  </div>
</template>
