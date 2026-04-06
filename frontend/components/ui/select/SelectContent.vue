<script setup lang="ts">
import {
  SelectContent,
  type SelectContentProps,
  SelectPortal,
  SelectViewport,
} from 'radix-vue'
import { cn } from '@/lib/utils'

const props = withDefaults(defineProps<SelectContentProps & { class?: string }>(), {
  position: 'popper',
  sideOffset: 4,
})
</script>

<template>
  <SelectPortal>
    <SelectContent
      v-bind="{ ...props, class: undefined }"
      :class="cn(
        'relative z-50 max-h-56 min-w-[8rem] overflow-hidden rounded-md border border-border/60 bg-card text-foreground shadow-md data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=top]:slide-in-from-bottom-2',
        props.position === 'popper' && 'data-[side=bottom]:translate-y-1 data-[side=top]:-translate-y-1',
        props.class,
      )"
    >
      <SelectViewport
        :class="cn(
          'p-1',
          props.position === 'popper' && 'h-[var(--radix-select-trigger-height)] w-full min-w-[var(--radix-select-trigger-width)]',
        )"
      >
        <slot />
      </SelectViewport>
    </SelectContent>
  </SelectPortal>
</template>
