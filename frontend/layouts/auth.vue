<script setup lang="ts">
// Auth layout — Sky & Lavender aurora background.
// No canvas animation; CSS-only for performance and simplicity.
</script>

<template>
  <div class="aurora-bg min-h-screen flex items-center justify-center relative overflow-hidden">
    <!-- Decorative soft glow layers for extra depth -->
    <div
      aria-hidden="true"
      class="pointer-events-none absolute inset-0"
      style="background:
        radial-gradient(ellipse 50% 40% at 25% 30%, rgba(147,197,253,0.25), transparent 70%),
        radial-gradient(ellipse 55% 45% at 75% 70%, rgba(196,181,253,0.22), transparent 70%);"
    />
    <!-- Content -->
    <div class="relative z-10 w-full flex justify-center">
      <slot />
    </div>
  </div>
</template>
