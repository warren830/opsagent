<script setup lang="ts">
import { type HTMLAttributes, computed } from 'vue'
import { cn } from '@/lib/utils'

const props = defineProps<{
  modelValue?: string
  class?: HTMLAttributes['class']
}>()

const emits = defineEmits<{
  'update:modelValue': [value: string]
}>()

const modelValue = computed({
  get: () => props.modelValue,
  set: (val) => emits('update:modelValue', val!),
})
</script>

<template>
  <textarea
    v-model="modelValue"
    :class="cn('flex min-h-[60px] w-full rounded border border-border/60 bg-secondary/50 px-2.5 py-1.5 text-xs ring-offset-background placeholder:text-muted-foreground/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 focus-visible:border-primary/50 disabled:cursor-not-allowed disabled:opacity-50 transition-colors', props.class)"
  />
</template>
