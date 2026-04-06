<script setup lang="ts">
import { type HTMLAttributes } from 'vue'
import { cn } from '@/lib/utils'

defineProps<{ class?: HTMLAttributes['class'] }>()
</script>

<template>
  <div :class="cn('rounded border border-border/60 bg-card text-card-foreground transition-all duration-150', $props.class)">
    <slot />
  </div>
</template>
