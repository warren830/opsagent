<script setup lang="ts">
import { TooltipTrigger, type TooltipTriggerProps } from 'radix-vue'
defineProps<TooltipTriggerProps>()
</script>
<template>
  <TooltipTrigger v-bind="$props"><slot /></TooltipTrigger>
</template>
