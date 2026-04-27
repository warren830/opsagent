<script setup lang="ts">
import { type HTMLAttributes } from 'vue'
import { cn } from '@/lib/utils'
defineProps<{ class?: HTMLAttributes['class'] }>()
</script>
<template>
  <div :class="cn('animate-pulse rounded-md bg-gradient-to-r from-slate-100 via-white to-slate-100 bg-[length:200%_100%]', $props.class)" />
</template>
