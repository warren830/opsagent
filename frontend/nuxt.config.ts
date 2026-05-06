// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2024-11-01',
  devtools: { enabled: true },
  telemetry: false,

  app: {
    head: {
      link: [
        { rel: 'icon', type: 'image/png', href: '/logo-icon.png' },
        { rel: 'preconnect', href: 'https://fonts.googleapis.com' },
        { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: '' },
        {
          rel: 'stylesheet',
          href: 'https://fonts.googleapis.com/css2?family=Geist:wght@300;400;500;600;700&family=Geist+Mono:wght@400;500&display=swap',
        },
      ],
    },
  },

  modules: [
    '@nuxtjs/tailwindcss',
    '@nuxtjs/i18n',
    '@pinia/nuxt',
  ],

  // Components auto-import — limit to .vue files only. Without this, Nuxt
  // also registers shadcn-vue `components/ui/<name>/index.ts` barrel files
  // and collides with the sibling `.vue` component, producing 19 dev-mode
  // "Two component files resolving to the same name Ui<Name>" warnings.
  components: [
    { path: '~/components', extensions: ['vue'], pathPrefix: true },
  ],

  // i18n
  i18n: {
    locales: [
      { code: 'zh', name: '中文', file: 'zh.json' },
      { code: 'en', name: 'English', file: 'en.json' },
    ],
    defaultLocale: 'zh',
    langDir: '../i18n',
    strategy: 'no_prefix',
    // Disable browser Accept-Language detection so everyone starts on zh by
    // default (per project language policy). Users can still switch via the
    // header toggle; i18n persists the choice in the `i18n_locale` cookie.
    detectBrowserLanguage: false,
    // Silence the v10 deprecation notice for the v-t directive optimiser.
    // We don't rely on bundle-time translation-directive optimisation.
    bundle: {
      optimizeTranslationDirective: false,
    },
  },

  // API proxy — forwards /api/* to the Rust backend.
  // In local dev, the backend runs on localhost:8003 (matching .env BACKEND_PORT).
  // In production (EKS), set NUXT_BACKEND_URL=http://ops-backend:3080.
  routeRules: {
    '/api/**': {
      proxy: { to: `${process.env.NUXT_BACKEND_URL || 'http://localhost:8003'}/api/**` },
    },
    '/health': {
      proxy: { to: `${process.env.NUXT_BACKEND_URL || 'http://localhost:8003'}/health` },
    },
  },

  // Tailwind
  tailwindcss: {
    cssPath: '~/assets/css/tailwind.css',
    configPath: 'tailwind.config.ts',
  },

  // Runtime config
  runtimeConfig: {
    // Server-only: internal backend URL for SSR API calls
    backendUrl: process.env.NUXT_BACKEND_URL || 'http://localhost:8003',
    public: {
      apiBase: process.env.NUXT_PUBLIC_API_BASE || '',
    },
  },

  // TypeScript
  typescript: {
    strict: true,
  },

  // Root-cause fix for the intermittent `Failed to resolve import
  // "#app-manifest"` dev-server error. The virtual module is only needed
  // for prerender / redirect / cacheable routeRules — we only use routeRules
  // for proxy-to-backend, which doesn't require the manifest. Disabling it
  // means the module is never generated and can never go stale. Safe because
  // no `.vue` / composable calls `useAppManifest`, `getAppManifest`, or
  // `checkForUpdate` anywhere in the app code. Re-enable if we ever add
  // prerendered or redirecting routes.
  experimental: {
    appManifest: false,
  },
})
