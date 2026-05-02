<!--
  AuroraStars — scattered bright dots (32 white + 2 teal + 2 indigo),
  tiled 800x600px. Gives the upper viewport "depth of space" without
  any shapes to read as content.

  Positions and radii are hand-picked (not random), weighted asymmetrically
  toward top-right so the composition has a diagonal rhythm instead of
  uniform scatter. Two colored stars (teal + indigo) echo the halo stops,
  tying the top half of the viewport to the bottom through color.
-->
<script setup lang="ts">
const { theme } = useTheme()
</script>

<template>
  <div v-if="theme === 'aurora'" class="aurora-stars" aria-hidden="true" />
</template>
