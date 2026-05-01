<script setup lang="ts">
import { type HTMLAttributes, computed } from 'vue'
import { SwitchRoot, SwitchThumb } from 'radix-vue'
import { cn } from '@/lib/utils'

const props = defineProps<{
  defaultChecked?: boolean
  checked?: boolean
  class?: HTMLAttributes['class']
  disabled?: boolean
}>()

const emits = defineEmits<{
  'update:checked': [value: boolean]
}>()

const checked = computed({
  get: () => props.checked,
  set: (val) => emits('update:checked', val!),
})
</script>

<template>
  <SwitchRoot
    v-model:checked="checked"
    :disabled="disabled"
    :class="cn('peer inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-gradient-to-r data-[state=checked]:from-primary data-[state=checked]:to-ai data-[state=unchecked]:bg-muted', props.class)"
  >
    <SwitchThumb class="pointer-events-none block h-4 w-4 rounded-full bg-foreground shadow-md ring-0 transition-transform data-[state=checked]:translate-x-4 data-[state=unchecked]:translate-x-0" />
  </SwitchRoot>
</template>
