<!--
  MeshTexture — faint isometric triangle pattern across the full viewport.
  Adds structural rhythm without stealing attention (default 4% opacity,
  1% in focus mode). Renders only under Aurora dark theme.
-->
<script setup lang="ts">
const { theme } = useTheme()
</script>

<template>
  <div v-if="theme === 'aurora'" class="mesh-texture" aria-hidden="true" />
</template>
