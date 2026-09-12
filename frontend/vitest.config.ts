import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath } from 'node:url'

export default defineConfig({
  // Lets specs mount real SFCs (see tests/components/services/*.mount.spec.ts)
  // so component wiring is exercised instead of mirrored. Not listed in
  // package.json: the plugin already ships pinned in package-lock.json as a
  // Nuxt/Vite transitive, so `npm ci` resolves it without installing anything.
  plugins: [vue()],
  test: {
    environment: 'happy-dom',
    setupFiles: ['./tests/setup.ts'],
    globals: true,
    include: ['tests/**/*.spec.ts'],
    coverage: {
      provider: 'v8',
      include: ['composables/**/*.ts', 'stores/**/*.ts'],
      exclude: ['**/*.d.ts'],
    },
  },
  resolve: {
    alias: {
      '~': fileURLToPath(new URL('.', import.meta.url)),
      '@': fileURLToPath(new URL('.', import.meta.url)),
    },
  },
})
