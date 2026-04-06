<script setup lang="ts">
import { SelectIcon, SelectTrigger, type SelectTriggerProps } from 'radix-vue'
import { ChevronDown } from 'lucide-vue-next'
import { cn } from '@/lib/utils'

const props = defineProps<SelectTriggerProps & { class?: string }>()
</script>

<template>
  <SelectTrigger
    v-bind="props"
    :class="cn(
      'flex h-7 w-full items-center justify-between rounded border border-border/60 bg-secondary/50 px-2 py-1 text-[11px] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 transition-colors disabled:cursor-not-allowed disabled:opacity-50 [&>span]:truncate',
      props.class,
    )"
  >
    <slot />
    <SelectIcon as-child>
      <ChevronDown class="h-3 w-3 shrink-0 opacity-50" />
    </SelectIcon>
  </SelectTrigger>
</template>
