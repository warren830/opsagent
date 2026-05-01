<!--
  AuroraHalo — bottom ~35vh gradient band (pink-purple → teal → indigo).
  Signature brand motif for Aurora dark theme.

  Only renders under [data-theme="aurora"]. In focus mode the .aurora-halo
  utility reads --halo-dim and fades automatically (handled in CSS).
-->
<script setup lang="ts">
const { theme } = useTheme()
</script>

<template>
  <div v-if="theme === 'aurora'" class="aurora-halo" aria-hidden="true" />
</template>
