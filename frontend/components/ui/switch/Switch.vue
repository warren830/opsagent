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
    :class="cn('peer inline-flex h-4 w-7 shrink-0 cursor-pointer items-center rounded-full border border-transparent transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-primary data-[state=unchecked]:bg-input', props.class)"
  >
    <SwitchThumb class="pointer-events-none block h-3 w-3 rounded-full bg-background shadow-sm ring-0 transition-transform data-[state=checked]:translate-x-3 data-[state=unchecked]:translate-x-0" />
  </SwitchRoot>
</template>
