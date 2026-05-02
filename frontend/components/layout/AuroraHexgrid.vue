<!--
  AuroraHexgrid — "Telemetry Topology" design language for Ops.

  Hand-picked field of 14 hexagons scattered across the viewport.
  Semantic: hexagon == cluster/pod (K8s heritage). Three size tiers
  (small/medium/large), mixed stroke-only vs faint-fill variants, 3
  color accents (teal + indigo echo the halo palette, rest neutral).

  Each hex pulses slowly (4.8s cycle) with a unique animation-delay so
  the field never syncs. Effect reads as "live infra breathing" rather
  than a mechanical animation loop.

  Positions picked to avoid the central content zone (where the app's
  panels sit) and weighted toward the upper half — where our current
  composition has empty space that needed filling.

  SVG viewBox is 1600×900 (16:9). preserveAspectRatio="xMidYMid slice"
  crops to fit any aspect ratio without distorting hex geometry.
-->
<script setup lang="ts">
const { theme } = useTheme()

type HexVariant = 'stroke' | 'fill' | 'dot'
interface Hex {
  cx: number
  cy: number
  r: number
  variant: HexVariant
  color: 'neutral' | 'teal' | 'indigo'
  /** stagger delay seconds */
  delay: number
  /** pulse min opacity */
  opMin: number
  /** pulse max opacity */
  opMax: number
  /** rotate deg (subtle variance) */
  rot?: number
}

// Pointy-top hexagon points around (0, 0) with radius r.
// sqrt(3)/2 ≈ 0.866025.
function hexPoints(cx: number, cy: number, r: number): string {
  const w = r * 0.8660254
  return [
    [cx, cy - r],
    [cx + w, cy - r * 0.5],
    [cx + w, cy + r * 0.5],
    [cx, cy + r],
    [cx - w, cy + r * 0.5],
    [cx - w, cy - r * 0.5],
  ].map((p) => p.join(',')).join(' ')
}

// 14 hexagons. Positions hand-tuned, not random — the asymmetry is
// intentional (more upper-half, avoids center content zone).
const hexes: Hex[] = [
  // Top row — dense
  { cx: 120,  cy: 100, r: 52, variant: 'stroke', color: 'neutral', delay: 0.0, opMin: 0.05, opMax: 0.14 },
  { cx: 310,  cy: 60,  r: 28, variant: 'fill',   color: 'neutral', delay: 1.2, opMin: 0.02, opMax: 0.06 },
  { cx: 540,  cy: 110, r: 34, variant: 'stroke', color: 'teal',    delay: 2.4, opMin: 0.06, opMax: 0.18 },
  { cx: 1050, cy: 90,  r: 68, variant: 'dot',    color: 'neutral', delay: 3.0, opMin: 0.05, opMax: 0.13, rot: 8 },
  { cx: 1280, cy: 50,  r: 26, variant: 'stroke', color: 'neutral', delay: 0.6, opMin: 0.04, opMax: 0.11 },
  { cx: 1480, cy: 160, r: 42, variant: 'fill',   color: 'indigo',  delay: 1.8, opMin: 0.04, opMax: 0.12 },

  // Mid row — sparse, edges only
  { cx: 70,   cy: 380, r: 38, variant: 'stroke', color: 'neutral', delay: 3.6, opMin: 0.04, opMax: 0.10 },
  { cx: 1550, cy: 440, r: 78, variant: 'dot',    color: 'teal',    delay: 1.0, opMin: 0.05, opMax: 0.13, rot: -4 },
  { cx: 1420, cy: 620, r: 34, variant: 'stroke', color: 'neutral', delay: 2.2, opMin: 0.04, opMax: 0.10 },

  // Bottom zone — fewer, poking into halo
  { cx: 180,  cy: 780, r: 48, variant: 'stroke', color: 'neutral', delay: 0.4, opMin: 0.03, opMax: 0.08 },
  { cx: 430,  cy: 850, r: 36, variant: 'fill',   color: 'neutral', delay: 2.8, opMin: 0.02, opMax: 0.06 },
  { cx: 820,  cy: 820, r: 62, variant: 'dot',    color: 'neutral', delay: 1.6, opMin: 0.04, opMax: 0.10, rot: 12 },
  { cx: 1180, cy: 780, r: 30, variant: 'stroke', color: 'indigo',  delay: 3.4, opMin: 0.05, opMax: 0.14 },

  // Accent: one small teal hex near chat area
  { cx: 1350, cy: 290, r: 22, variant: 'fill',   color: 'teal',    delay: 4.0, opMin: 0.04, opMax: 0.11 },
]

function colorVar(c: Hex['color']): string {
  if (c === 'teal') return 'hsl(var(--aurora-teal))'
  if (c === 'indigo') return 'hsl(var(--aurora-indigo))'
  return 'hsl(0 0% 100%)'
}
</script>

<template>
  <div v-if="theme === 'aurora'" class="aurora-hexgrid" aria-hidden="true">
    <svg
      viewBox="0 0 1600 900"
      preserveAspectRatio="xMidYMid slice"
      xmlns="http://www.w3.org/2000/svg"
    >
      <template v-for="(h, i) in hexes" :key="i">
        <!-- Stroke-only -->
        <polygon
          v-if="h.variant === 'stroke'"
          class="hex-shape"
          :points="hexPoints(h.cx, h.cy, h.r)"
          :stroke="colorVar(h.color)"
          stroke-width="1"
          fill="none"
          :transform="h.rot ? `rotate(${h.rot} ${h.cx} ${h.cy})` : undefined"
          :style="{
            '--delay': `${h.delay}s`,
            '--op-min': h.opMin,
            '--op-max': h.opMax,
          }"
        />
        <!-- Filled (very low alpha) -->
        <polygon
          v-else-if="h.variant === 'fill'"
          class="hex-shape"
          :points="hexPoints(h.cx, h.cy, h.r)"
          :fill="colorVar(h.color)"
          stroke="none"
          :transform="h.rot ? `rotate(${h.rot} ${h.cx} ${h.cy})` : undefined"
          :style="{
            '--delay': `${h.delay}s`,
            '--op-min': h.opMin,
            '--op-max': h.opMax,
          }"
        />
        <!-- Stroke + center dot -->
        <g
          v-else
          class="hex-shape"
          :transform="h.rot ? `rotate(${h.rot} ${h.cx} ${h.cy})` : undefined"
          :style="{
            '--delay': `${h.delay}s`,
            '--op-min': h.opMin,
            '--op-max': h.opMax,
          }"
        >
          <polygon
            :points="hexPoints(h.cx, h.cy, h.r)"
            :stroke="colorVar(h.color)"
            stroke-width="1"
            fill="none"
          />
          <circle
            :cx="h.cx"
            :cy="h.cy"
            :r="Math.max(1.5, h.r * 0.05)"
            :fill="colorVar(h.color)"
          />
        </g>
      </template>
    </svg>
  </div>
</template>
