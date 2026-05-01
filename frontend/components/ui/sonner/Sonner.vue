<script setup lang="ts">
import { Toaster } from 'vue-sonner'
</script>

<template>
  <Toaster
    position="top-right"
    :toast-options="{
      classes: {
        toast: 'group toast flex gap-3 items-center rounded-xl border border-border/70 bg-popover/95 backdrop-blur-xl p-4 shadow-[0_8px_32px_rgb(0_0_0/0.4)] text-popover-foreground data-[type=success]:border-success/40 data-[type=error]:border-destructive/40 data-[type=warning]:border-warning/40',
        description: 'group-[.toast]:text-muted-foreground',
        actionButton: 'group-[.toast]:bg-primary group-[.toast]:text-primary-foreground',
        cancelButton: 'group-[.toast]:bg-muted group-[.toast]:text-muted-foreground',
      },
    }"
  />
</template>
