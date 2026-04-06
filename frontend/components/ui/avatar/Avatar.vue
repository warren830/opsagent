<script setup lang="ts">
import { type HTMLAttributes } from 'vue'
import { cn } from '@/lib/utils'

defineProps<{
  class?: HTMLAttributes['class']
  fallback?: string
  src?: string
}>()
</script>

<template>
  <span :class="cn('relative flex h-10 w-10 shrink-0 overflow-hidden rounded-full', $props.class)">
    <img v-if="src" :src="src" class="aspect-square h-full w-full" alt="" />
    <span v-else class="flex h-full w-full items-center justify-center rounded-full bg-muted text-sm font-medium">
      {{ fallback }}
    </span>
  </span>
</template>
