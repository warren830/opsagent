<script setup lang="ts">
import { type HTMLAttributes, computed } from 'vue'
import { cn } from '@/lib/utils'

const props = defineProps<{
  defaultValue?: string | number
  modelValue?: string | number
  class?: HTMLAttributes['class']
}>()

const emits = defineEmits<{
  'update:modelValue': [value: string | number]
}>()

const modelValue = computed({
  get: () => props.modelValue,
  set: (val) => emits('update:modelValue', val!),
})
</script>

<template>
  <input
    v-model="modelValue"
    :class="cn('flex h-8 w-full rounded-lg border border-border bg-input/70 backdrop-blur-sm px-3 py-2 text-xs text-foreground placeholder:text-muted-foreground ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-0 focus-visible:border-ring focus-visible:bg-input transition-colors disabled:cursor-not-allowed disabled:opacity-50', props.class)"
  />
</template>
