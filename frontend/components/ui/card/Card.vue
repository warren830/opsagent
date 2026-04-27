<script setup lang="ts">
import { type HTMLAttributes } from 'vue'
import { cn } from '@/lib/utils'

defineProps<{ class?: HTMLAttributes['class'] }>()
</script>

<template>
  <div :class="cn('rounded-[14px] border border-white/85 bg-white/62 backdrop-blur-[18px] shadow-[0_4px_16px_rgba(100,140,200,0.08)] text-card-foreground transition-shadow duration-200 hover:shadow-[0_8px_28px_rgba(100,140,200,0.14)]', $props.class)">
    <slot />
  </div>
</template>
