<script setup lang="ts">
import { Toaster } from 'vue-sonner'
</script>

<template>
  <Toaster
    position="top-right"
    :toast-options="{
      classes: {
        toast: 'group toast flex gap-3 items-center rounded-xl border border-white/85 bg-white/90 backdrop-blur-xl p-4 shadow-[0_8px_32px_rgba(100,140,200,0.18)] text-slate-900 data-[type=success]:border-emerald-200 data-[type=error]:border-red-200 data-[type=warning]:border-amber-200',
        description: 'group-[.toast]:text-muted-foreground',
        actionButton: 'group-[.toast]:bg-primary group-[.toast]:text-primary-foreground',
        cancelButton: 'group-[.toast]:bg-muted group-[.toast]:text-muted-foreground',
      },
    }"
  />
</template>
