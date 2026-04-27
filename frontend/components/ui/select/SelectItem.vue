<script setup lang="ts">
import {
  SelectItem,
  SelectItemIndicator,
  type SelectItemProps,
  SelectItemText,
} from 'radix-vue'
import { Check } from 'lucide-vue-next'
import { cn } from '@/lib/utils'

const props = defineProps<SelectItemProps & { class?: string }>()
</script>

<template>
  <SelectItem
    v-bind="{ ...props, class: undefined }"
    :class="cn(
      'relative flex w-full cursor-default select-none items-center rounded-md py-1.5 pl-8 pr-2 text-xs text-slate-700 outline-none hover:bg-sky-50 hover:text-sky-700 focus:bg-sky-50 focus:text-sky-700 data-[state=checked]:bg-sky-100 data-[state=checked]:text-sky-700 data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
      props.class,
    )"
  >
    <span class="absolute left-1.5 flex h-3 w-3 items-center justify-center">
      <SelectItemIndicator>
        <Check class="h-2.5 w-2.5" />
      </SelectItemIndicator>
    </span>
    <SelectItemText>
      <slot />
    </SelectItemText>
  </SelectItem>
</template>
