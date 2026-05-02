<!--
  AuroraCornerGlow — ambient top-corner atmosphere for the aurora theme.
  Top-right emits a teal bloom, top-left an indigo bloom. Together with
  AuroraHalo (bottom) they form a 3-point lighting rig that gives the
  background its "inside a Bedrock Agent Studio" feeling.

  Render only under [data-theme="aurora"]. Each glow is large (55vw/55vh)
  and heavily blurred so it reads as atmosphere, not a shape.
-->
<script setup lang="ts">
const { theme } = useTheme()
</script>

<template>
  <template v-if="theme === 'aurora'">
    <div class="aurora-corner-tl" aria-hidden="true" />
    <div class="aurora-corner-tr" aria-hidden="true" />
  </template>
</template>
