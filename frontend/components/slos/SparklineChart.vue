<script setup lang="ts">
import { computed } from 'vue'

/**
 * Tiny dependency-free sparkline / line chart renderer. We deliberately
 * avoid pulling chartjs/echarts/etc. — the SLO pages only need two
 * single-series charts (SLI %, budget %) and a dense sparkline behaves
 * better at `text-xs` sizes than a heavyweight canvas chart.
 *
 * Coordinates are drawn into a viewBox so the SVG scales to any container
 * width without reflow math.
 */
const props = defineProps<{
  points: Array<[number, number]>   // [unix-seconds-or-any-x, y]
  height?: number
  width?: number
  yMin?: number
  yMax?: number
  referenceY?: number                // optional horizontal reference (e.g. target %)
  referenceLabel?: string | null
  color?: string
  filled?: boolean
}>()

const H = computed(() => props.height ?? 160)
const W = computed(() => props.width ?? 600)

const bounds = computed(() => {
  if (!props.points.length) return { xMin: 0, xMax: 1, yMin: 0, yMax: 1 }
  const xs = props.points.map((p) => p[0])
  const ys = props.points.map((p) => p[1]).filter((v) => Number.isFinite(v))
  const yMin = props.yMin ?? (ys.length ? Math.min(...ys) : 0)
  const yMax = props.yMax ?? (ys.length ? Math.max(...ys) : 1)
  const pad = yMax === yMin ? 1 : (yMax - yMin) * 0.05
  return {
    xMin: Math.min(...xs),
    xMax: Math.max(...xs),
    yMin: yMin - pad,
    yMax: yMax + pad,
  }
})

function sx(x: number): number {
  const { xMin, xMax } = bounds.value
  if (xMax === xMin) return W.value / 2
  return ((x - xMin) / (xMax - xMin)) * (W.value - 8) + 4
}
function sy(y: number): number {
  const { yMin, yMax } = bounds.value
  if (yMax === yMin) return H.value / 2
  return H.value - 4 - ((y - yMin) / (yMax - yMin)) * (H.value - 8)
}

const path = computed(() => {
  const pts = props.points.filter((p) => Number.isFinite(p[1]))
  if (!pts.length) return ''
  return pts
    .map((p, i) => `${i === 0 ? 'M' : 'L'} ${sx(p[0]).toFixed(2)} ${sy(p[1]).toFixed(2)}`)
    .join(' ')
})

const areaPath = computed(() => {
  const pts = props.points.filter((p) => Number.isFinite(p[1]))
  if (!pts.length) return ''
  const first = pts[0]
  const last = pts[pts.length - 1]
  const top = pts
    .map((p, i) => `${i === 0 ? 'M' : 'L'} ${sx(p[0]).toFixed(2)} ${sy(p[1]).toFixed(2)}`)
    .join(' ')
  return `${top} L ${sx(last[0]).toFixed(2)} ${H.value - 4} L ${sx(first[0]).toFixed(2)} ${H.value - 4} Z`
})

const refY = computed(() => (props.referenceY != null ? sy(props.referenceY) : null))
const stroke = computed(() => props.color || 'currentColor')
</script>

<template>
  <svg
    :viewBox="`0 0 ${W} ${H}`"
    preserveAspectRatio="none"
    class="w-full text-primary"
    :style="{ height: `${H}px` }"
  >
    <!-- baseline grid -->
    <line
      x1="0"
      :y1="H - 4"
      :x2="W"
      :y2="H - 4"
      stroke="currentColor"
      stroke-opacity="0.15"
      stroke-width="0.5"
    />

    <!-- reference line (e.g. SLO target) -->
    <line
      v-if="refY !== null"
      x1="0"
      :y1="refY ?? 0"
      :x2="W"
      :y2="refY ?? 0"
      stroke="hsl(var(--destructive))"
      stroke-opacity="0.6"
      stroke-dasharray="3 3"
      stroke-width="0.75"
    />
    <text
      v-if="refY !== null && referenceLabel"
      :x="W - 4"
      :y="(refY ?? 0) - 3"
      text-anchor="end"
      class="fill-destructive"
      style="font-size: 9px"
    >
      {{ referenceLabel }}
    </text>

    <!-- filled area -->
    <path v-if="filled && areaPath" :d="areaPath" :fill="stroke" fill-opacity="0.12" />

    <!-- main line -->
    <path v-if="path" :d="path" fill="none" :stroke="stroke" stroke-width="1.25" vector-effect="non-scaling-stroke" />

    <!-- empty state -->
    <text
      v-if="!points.length"
      :x="W / 2"
      :y="H / 2"
      text-anchor="middle"
      class="fill-muted-foreground"
      style="font-size: 10px"
    >
      no data
    </text>
  </svg>
</template>
