<script setup lang="ts">
import { useMouse, useMediaQuery } from '@vueuse/core'
import { computed } from 'vue'

const { x, y } = useMouse({ type: 'page' })
const reduceMotion = useMediaQuery('(prefers-reduced-motion: reduce)')
const isTouch = useMediaQuery('(hover: none)')

const visible = computed(() => !reduceMotion.value && !isTouch.value)

const transform = computed(() => `translate(${x.value - 220}px, ${y.value - 220}px)`)
</script>

<template>
  <div
    v-if="visible"
    aria-hidden="true"
    class="fixed top-0 left-0 -z-10 pointer-events-none w-[440px] h-[440px] rounded-full opacity-60 mix-blend-multiply transition-[opacity] duration-300"
    :style="{
      transform,
      background: 'radial-gradient(circle, rgba(147,197,253,0.25), rgba(196,181,253,0.1) 40%, transparent 70%)',
      willChange: 'transform',
    }"
  />
</template>
