// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2024-11-01',
  devtools: { enabled: true },
  telemetry: false,

  app: {
    head: {
      link: [
        { rel: 'icon', type: 'image/png', href: '/logo-icon.png' },
      ],
    },
  },

  modules: [
    '@nuxtjs/tailwindcss',
    '@nuxtjs/i18n',
    '@pinia/nuxt',
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
    detectBrowserLanguage: {
      useCookie: true,
      cookieKey: 'i18n_locale',
      fallbackLocale: 'zh',
    },
  },

  // API proxy — forwards /api/* to the Rust backend.
  // In local dev, the backend runs on localhost:3080.
  // In production (EKS), set NUXT_BACKEND_URL=http://ops-backend:3080.
  routeRules: {
    '/api/**': {
      proxy: { to: `${process.env.NUXT_BACKEND_URL || 'http://localhost:3080'}/api/**` },
    },
    '/health': {
      proxy: { to: `${process.env.NUXT_BACKEND_URL || 'http://localhost:3080'}/health` },
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
    backendUrl: process.env.NUXT_BACKEND_URL || 'http://localhost:3080',
    public: {
      apiBase: process.env.NUXT_PUBLIC_API_BASE || '',
    },
  },

  // Note: shadcn-vue ui/ barrel files (index.ts) cause harmless duplicate
  // component name warnings. These are safe to ignore.

  // TypeScript
  typescript: {
    strict: true,
  },
})
