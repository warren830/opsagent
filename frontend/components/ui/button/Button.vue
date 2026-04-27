<script setup lang="ts">
import { type HTMLAttributes, computed } from 'vue'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded text-xs font-medium ring-offset-background transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sky-500/40 disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 active:scale-[0.97]',
  {
    variants: {
      variant: {
        default: 'text-white bg-gradient-to-r from-sky-500 to-violet-500 hover:brightness-110 shadow-md shadow-sky-500/25 hover:shadow-sky-500/40',
        destructive: 'text-white bg-gradient-to-r from-rose-500 to-red-500 hover:brightness-110 shadow-md shadow-rose-500/25',
        success: 'text-white bg-gradient-to-r from-emerald-500 to-teal-500 hover:brightness-110 shadow-md shadow-emerald-500/25',
        outline: 'border border-slate-200 bg-white/70 backdrop-blur-sm text-slate-700 hover:bg-white hover:border-slate-300 hover:text-slate-900',
        secondary: 'bg-white/60 border border-white/80 backdrop-blur-sm text-slate-700 hover:bg-white hover:text-slate-900',
        ghost: 'text-slate-600 hover:bg-slate-100 hover:text-slate-900',
        link: 'text-sky-600 underline-offset-4 hover:underline',
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
