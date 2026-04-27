<script setup lang="ts">
import { type HTMLAttributes } from 'vue'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

const glassPanelVariants = cva(
  'border transition-shadow',
  {
    variants: {
      variant: {
        subtle:  'glass-panel-subtle',
        default: 'glass-panel',
        strong:  'glass-panel shadow-[0_12px_40px_rgba(100,140,200,0.16)]',
      },
      hover: {
        none: '',
        lift: 'glass-hover-lift',
      },
    },
    defaultVariants: {
      variant: 'default',
      hover: 'none',
    },
  },
)

type GlassPanelVariants = VariantProps<typeof glassPanelVariants>

defineProps<{
  variant?: NonNullable<GlassPanelVariants['variant']>
  hover?: NonNullable<GlassPanelVariants['hover']>
  as?: string
  class?: HTMLAttributes['class']
}>()
</script>

<template>
  <component
    :is="as || 'div'"
    :class="cn(glassPanelVariants({ variant, hover }), $props.class)"
  >
    <slot />
  </component>
</template>
