/**
 * Theme boot plugin (client-only).
 *
 * No-op now that useTheme() itself calls useHead({ htmlAttrs }) — Nuxt
 * emits <html data-theme="aurora"> during SSR, so the first paint already
 * has correct theme tokens (no flash-of-light on refresh).
 *
 * Kept as a file for documentation; can be deleted later.
 */
export default defineNuxtPlugin(() => {
  // intentionally empty
})
