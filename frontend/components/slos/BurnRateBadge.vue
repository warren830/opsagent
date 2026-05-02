<script setup lang="ts">
import { computed } from 'vue'
import { Badge } from '@/components/ui/badge'

/**
 * Compact burn-rate badge — renders `—` when the rate is null/NaN so the
 * list table stays visually aligned while snapshots are still warming up.
 *
 * Thresholds mirror the MWMBR policy's severity bands:
 *  < 1x   healthy  (green)
 *  1–3x   elevated (warning)
 *  3–10x  hot      (destructive)
 *  > 10x  critical (destructive + pulse)
 */
const props = defineProps<{
  rate: number | null | undefined
  label?: string | null
}>()

const formatted = computed(() => {
  const r = props.rate
  if (r == null || !Number.isFinite(r)) return '—'
  if (r >= 100) return `${r.toFixed(0)}x`
  if (r >= 10) return `${r.toFixed(1)}x`
  return `${r.toFixed(2)}x`
})

const variant = computed<'secondary' | 'success' | 'warning' | 'destructive'>(() => {
  const r = props.rate
  if (r == null || !Number.isFinite(r)) return 'secondary'
  if (r < 1) return 'success'
  if (r < 3) return 'warning'
  return 'destructive'
})

const critical = computed(() => (props.rate ?? 0) >= 10)
</script>

<template>
  <Badge :variant="variant" :class="critical ? 'animate-pulse font-semibold' : ''">
    <span v-if="label" class="mr-1 text-[9px] opacity-70">{{ label }}</span>
    <span class="font-mono">{{ formatted }}</span>
  </Badge>
</template>
