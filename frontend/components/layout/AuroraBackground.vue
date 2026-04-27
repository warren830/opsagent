<script setup lang="ts">
// Fixed full-viewport gradient-halo layer.
// Sits at z-index -10 so ALL content is above; layout islands at z-10+.
//
// Uses the --aurora-* CSS vars defined in assets/css/tailwind.css :root,
// plus the aurora-drift keyframe registered in tailwind.config.ts.
// motion-safe: only animates if prefers-reduced-motion is NOT set.
</script>

<template>
  <div aria-hidden="true" class="fixed inset-0 -z-10 pointer-events-none overflow-hidden bg-white">
    <div
      class="absolute inset-0 motion-safe:animate-[aurora-drift_45s_ease-in-out_infinite]"
      :style="{
        background: `
          radial-gradient(ellipse 50% 40% at 12% 10%, var(--aurora-sky), transparent 55%),
          radial-gradient(ellipse 55% 45% at 88% 22%, var(--aurora-lavender), transparent 55%),
          radial-gradient(ellipse 60% 50% at 50% 100%, var(--aurora-mint), transparent 55%)
        `,
      }"
    />
  </div>
</template>
