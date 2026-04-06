import { createConfigForNuxt } from '@nuxt/eslint-config/flat'

export default createConfigForNuxt({
  features: {
    tooling: true,
  },
}).append(
  // shadcn-vue generated components — relaxed rules
  {
    files: ['components/ui/**/*.vue'],
    rules: {
      '@typescript-eslint/no-import-type-side-effects': 'off',
      '@typescript-eslint/no-unused-vars': 'off',
      'vue/require-default-prop': 'off',
    },
  },
  // Project-wide rules
  {
    rules: {
      'vue/multi-word-component-names': 'off',
      'vue/html-self-closing': 'off',
      'vue/no-v-html': 'warn',
      'vue/attributes-order': 'warn',
      '@typescript-eslint/no-explicit-any': 'warn',
    },
  },
)
