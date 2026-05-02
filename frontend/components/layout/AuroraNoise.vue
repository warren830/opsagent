<!--
  AuroraNoise — SVG feTurbulence film grain, tiled viewport-wide.

  The magic: mix-blend-mode: overlay. This makes the noise:
    - On dark areas, contribute subtle dark speckles
    - On bright halo areas, contribute subtle bright speckles
  So grain "bonds" with the underlying scene instead of sitting on top
  of it. Visually indistinguishable from camera sensor noise.

  Performance: the turbulence SVG is evaluated once per 240x240 tile
  (browser caches the tile image), NOT per frame. Zero animation.
-->
<script setup lang="ts">
const { theme } = useTheme()
</script>

<template>
  <div v-if="theme === 'aurora'" class="aurora-noise" aria-hidden="true" />
</template>
