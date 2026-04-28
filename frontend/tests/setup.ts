/**
 * Vitest setup — stubs the Nuxt auto-imports that composables rely on.
 * Tests override these via `vi.stubGlobal` or by mocking per-spec.
 */
import { vi } from 'vitest'
import { defineStore, setActivePinia, createPinia } from 'pinia'

// Provide sane defaults for Nuxt globals used by composables/stores.
vi.stubGlobal('useRuntimeConfig', () => ({
  public: { apiBase: '' },
  backendUrl: '',
}))
vi.stubGlobal('useRequestHeaders', () => ({}))

// Reset Pinia for each test file.
setActivePinia(createPinia())

// Re-export defineStore so stores can `import { defineStore } from 'pinia'`
// under test as usual. Nothing special — just documented import path.
export { defineStore }
