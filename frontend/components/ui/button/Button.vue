<script setup lang="ts">
import { type HTMLAttributes, computed } from 'vue'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded text-xs font-medium ring-offset-background transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 active:scale-[0.97]',
  {
    variants: {
      variant: {
        default: 'text-primary-foreground bg-gradient-to-r from-primary to-ai hover:brightness-110 shadow-md shadow-primary/25 hover:shadow-primary/40',
        destructive: 'text-destructive-foreground bg-gradient-to-r from-destructive to-destructive/80 hover:brightness-110 shadow-md shadow-destructive/25',
        success: 'text-success-foreground bg-gradient-to-r from-success to-success/70 hover:brightness-110 shadow-md shadow-success/25',
        outline: 'border border-border bg-panel/70 backdrop-blur-sm text-muted-foreground hover:bg-panel hover:border-border hover:text-foreground',
        secondary: 'bg-panel/60 border border-border/80 backdrop-blur-sm text-muted-foreground hover:bg-panel hover:text-foreground',
        ghost: 'text-muted-foreground hover:bg-muted hover:text-foreground',
        link: 'text-primary underline-offset-4 hover:underline',
      },
      size: {
        default: 'h-8 px-3 py-1.5',
        sm: 'h-7 rounded px-2.5 text-[11px]',
        lg: 'h-9 rounded px-5',
        icon: 'h-8 w-8',
        'icon-sm': 'h-6 w-6',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  },
)

type ButtonVariants = VariantProps<typeof buttonVariants>

const props = withDefaults(defineProps<{
  variant?: NonNullable<ButtonVariants['variant']>
  size?: NonNullable<ButtonVariants['size']>
  as?: string
  class?: HTMLAttributes['class']
  disabled?: boolean
}>(), {
  as: 'button',
})

const delegatedProps = computed(() => {
  const { class: _, ...rest } = props
  return rest
})
</script>

<template>
  <component
    :is="as"
    :class="cn(buttonVariants({ variant, size }), props.class)"
    :disabled="disabled"
  >
    <slot />
  </component>
</template>
