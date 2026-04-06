<script setup lang="ts">
import { CollapsibleContent } from 'radix-vue'
</script>

<template>
  <CollapsibleContent class="overflow-hidden data-[state=closed]:animate-collapsible-up data-[state=open]:animate-collapsible-down">
    <slot />
  </CollapsibleContent>
</template>
