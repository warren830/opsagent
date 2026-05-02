<!--
  AuroraHaloWisps — 5 small colored spots inside the halo band to break
  the smooth gradient. Drifts at 67s (vs halo's 45s) in reverse direction,
  so the two animations never sync — the halo always looks "turbulent"
  rather than "linearly sliding".

  Stacked ABOVE the main halo (z-index is the same -10 but DOM order
  matters for paint). Heavy blur (24px) keeps wisps from reading as
  sharp blobs — they blend into the halo as variance.
-->
<script setup lang="ts">
const { theme } = useTheme()
</script>

<template>
  <div v-if="theme === 'aurora'" class="aurora-halo-wisps" aria-hidden="true" />
</template>
