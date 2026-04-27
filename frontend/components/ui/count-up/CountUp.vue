<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue'

const props = withDefaults(defineProps<{
  to: number
  duration?: number
  decimals?: number
  format?: (n: number) => string
}>(), {
  duration: 800,
  decimals: 0,
})

const displayed = ref(0)
let raf: number | null = null
let start = 0
let startTs = 0

function animate(to: number) {
  if (raf !== null) cancelAnimationFrame(raf)
  start = displayed.value
  startTs = performance.now()
  const frame = (now: number) => {
    const t = Math.min(1, (now - startTs) / props.duration)
    const eased = 1 - Math.pow(1 - t, 3)
    displayed.value = start + (to - start) * eased
    if (t < 1) raf = requestAnimationFrame(frame)
    else raf = null
  }
  raf = requestAnimationFrame(frame)
}

function formatted(n: number) {
  if (props.format) return props.format(n)
  return n.toLocaleString(undefined, {
    minimumFractionDigits: props.decimals,
    maximumFractionDigits: props.decimals,
  })
}

onMounted(() => animate(props.to))
watch(() => props.to, newVal => animate(newVal))
onUnmounted(() => { if (raf !== null) cancelAnimationFrame(raf) })
</script>

<template>
  <span class="tabular-nums">{{ formatted(displayed) }}</span>
</template>
