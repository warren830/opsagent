/**
 * Theme boot plugin (client-only).
 *
 * Runs once on client hydration. Reads the theme cookie (set by useTheme)
 * and applies <html data-theme="..."> before Vue mounts components, so there's
 * no visual swap between SSR first paint and client hydration.
 *
 * SSR note: the <html data-theme> attribute is ALSO injected server-side via
 * the `html` attrs API in nuxt.config.ts (see theme boot attrs there) when
 * possible. This plugin is the client-side safety net.
 */
export default defineNuxtPlugin(() => {
  const { theme } = useTheme()
  // useTheme's watchEffect already handles applying to document on client.
  // Touching `theme.value` here forces the watcher to register immediately.
  void theme.value
})
