/**
 * useTheme — theme state + persistence for Ops.
 *
 * Themes:
 *   - 'light'  : Sky & Lavender (default, existing)
 *   - 'aurora' : AWS Q Developer / Bedrock Agent Studio dark
 *
 * Persistence: cookie (SSR-safe) mirrored to localStorage (cross-tab sync
 * via @vueuse/core's useStorage under the hood would also work, but cookie
 * wins because Nuxt can read it during SSR and avoid flash-of-wrong-theme).
 *
 * Mount pattern:
 *   const { theme, setTheme } = useTheme()
 *   // reactively applies <html data-theme="...">
 */

export type ThemeId = 'light' | 'aurora'

const COOKIE_KEY = 'ops.theme'
const DEFAULT_THEME: ThemeId = 'light'

export const useTheme = () => {
  // Cookie is the source of truth — survives SSR and page reload.
  const cookie = useCookie<ThemeId>(COOKIE_KEY, {
    default: () => DEFAULT_THEME,
    sameSite: 'lax',
    maxAge: 60 * 60 * 24 * 365, // 1 year
  })

  const theme = computed<ThemeId>(() => cookie.value ?? DEFAULT_THEME)

  const setTheme = (next: ThemeId) => {
    cookie.value = next
    applyToDocument(next)
  }

  const toggleTheme = () => {
    setTheme(theme.value === 'aurora' ? 'light' : 'aurora')
  }

  // Emit <html data-theme="..."> during SSR so the initial HTML already
  // carries the correct theme attribute. This is what eliminates the
  // flash-of-light-theme on refresh — without it, the browser paints the
  // light-theme tokens for one frame before client hydration swaps in the
  // cookie-driven theme.
  //
  // Using useHead with htmlAttrs merges the attribute into Nuxt's rendered
  // <html>. For the light default we omit the attribute entirely (keeps the
  // HTML clean and lets :root apply naturally).
  useHead({
    htmlAttrs: computed(() =>
      theme.value === 'aurora' ? { 'data-theme': 'aurora' } : {},
    ),
  })

  // Client-side keep-in-sync: when setTheme runs, useHead will queue the
  // attribute update, but we also mirror it directly on document to make
  // same-tick style application guaranteed (useHead uses a microtask tick).
  if (import.meta.client) {
    watchEffect(() => applyToDocument(theme.value))
  }

  return { theme, setTheme, toggleTheme }
}

function applyToDocument(theme: ThemeId) {
  if (typeof document === 'undefined') return
  const root = document.documentElement
  if (theme === 'light') {
    root.removeAttribute('data-theme')
  } else {
    root.setAttribute('data-theme', theme)
  }
}
