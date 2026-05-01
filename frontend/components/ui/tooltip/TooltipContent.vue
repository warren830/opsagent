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
      :class="cn('z-50 overflow-hidden rounded-md bg-popover/95 border border-border/70 backdrop-blur-md px-2.5 py-1 text-[11px] text-popover-foreground shadow-lg animate-in fade-in-0 zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2', $props.class)"
    >
      <slot />
    </TooltipContent>
  </TooltipPortal>
</template>
