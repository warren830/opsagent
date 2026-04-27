<script setup lang="ts">
withDefaults(defineProps<{
  size?: number | string
  class?: string
}>(), { size: 24 })
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 40 40"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="$props.class"
    aria-label="Ops"
  >
    <defs>
      <linearGradient id="ops-brand-grad" x1="0" y1="0" x2="40" y2="40" gradientUnits="userSpaceOnUse">
        <stop offset="0%" stop-color="#3b82f6" />
        <stop offset="60%" stop-color="#8b5cf6" />
        <stop offset="100%" stop-color="#c4b5fd" />
      </linearGradient>
    </defs>
    <!-- Infinity-style interlocking arcs -->
    <path
      d="M13 20 C13 14, 19 14, 20 20 C21 26, 27 26, 27 20 C27 14, 21 14, 20 20 C19 26, 13 26, 13 20 Z"
      stroke="url(#ops-brand-grad)"
      stroke-width="2.75"
      stroke-linecap="round"
      stroke-linejoin="round"
      fill="none"
    />
    <!-- Subtle core dot -->
    <circle cx="20" cy="20" r="1.6" fill="url(#ops-brand-grad)" />
  </svg>
</template>
