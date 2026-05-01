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

  // Apply <html data-theme="..."> reactively, client-only.
  // SSR renders `data-theme` via a plugin on initial paint (see plugins/theme.client.ts).
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
