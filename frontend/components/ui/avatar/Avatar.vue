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
  <span :class="cn('relative flex h-10 w-10 shrink-0 overflow-hidden rounded-full ring-1 ring-white/80', $props.class)">
    <img v-if="src" :src="src" class="aspect-square h-full w-full" alt="" />
    <span v-else class="flex h-full w-full items-center justify-center rounded-full bg-gradient-to-br from-sky-400 to-violet-500 text-[11px] font-semibold text-white">
      {{ fallback }}
    </span>
  </span>
</template>
