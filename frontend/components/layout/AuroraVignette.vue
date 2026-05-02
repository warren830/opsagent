<!--
  AuroraVignette — subtle viewport-edge darkening that gives the page
  cinematic depth. Sits above halo/mesh (z-index -9) but below content
  (z-index 10+). Barely visible by itself; it's the "final 5%" layer that
  makes the composite background feel deliberate rather than flat.
-->
<script setup lang="ts">
const { theme } = useTheme()
</script>

<template>
  <div v-if="theme === 'aurora'" class="aurora-vignette" aria-hidden="true" />
</template>
