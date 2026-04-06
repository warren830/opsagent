<script setup lang="ts">
import { type HTMLAttributes } from 'vue'
import { TooltipContent, TooltipPortal } from 'radix-vue'
import { cn } from '@/lib/utils'

defineProps<{
  class?: HTMLAttributes['class']
  sideOffset?: number
}>()
</script>

<template>
  <TooltipPortal>
    <TooltipContent
      :side-offset="sideOffset ?? 4"
      :class="cn('z-50 overflow-hidden rounded-md border bg-popover px-3 py-1.5 text-sm text-popover-foreground shadow-md animate-in fade-in-0 zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95', $props.class)"
    >
      <slot />
    </TooltipContent>
  </TooltipPortal>
</template>
