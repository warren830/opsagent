<script setup lang="ts">
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'

const props = withDefaults(defineProps<{
  open: boolean
  title: string
  description: string
  confirmText?: string
  variant?: 'default' | 'destructive'
}>(), {
  confirmText: undefined,
  variant: 'default',
})

const emit = defineEmits<{
  confirm: []
  cancel: []
}>()

const { t } = useI18n()
</script>

<template>
  <Dialog :open="props.open" @update:open="(val) => { if (!val) emit('cancel') }">
    <DialogContent class="max-w-sm">
      <DialogHeader>
        <DialogTitle>{{ props.title }}</DialogTitle>
        <DialogDescription>{{ props.description }}</DialogDescription>
      </DialogHeader>
      <DialogFooter class="gap-1.5">
        <Button variant="outline" size="sm" @click="emit('cancel')">
          {{ t('common.cancel') }}
        </Button>
        <Button
          :variant="props.variant === 'destructive' ? 'destructive' : 'default'"
          size="sm"
          @click="emit('confirm')"
        >
          {{ props.confirmText || t('common.confirm') }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
