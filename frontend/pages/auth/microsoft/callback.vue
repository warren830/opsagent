<script setup lang="ts">
definePageMeta({ layout: 'auth' })

const { t } = useI18n()
const router = useRouter()
const route = useRoute()
const authStore = useAuthStore()

const error = ref('')
const loading = ref(true)

onMounted(async () => {
  try {
    const code = route.query.code as string
    const state = route.query.state as string
    const errorParam = route.query.error as string

    if (errorParam) {
      error.value = route.query.error_description as string || errorParam
      loading.value = false
      return
    }

    if (!code || !state) {
      error.value = t('auth.oauthMissingParams')
      loading.value = false
      return
    }

    // Validate state against sessionStorage
    const savedState = sessionStorage.getItem('oauth_state')
    if (savedState && savedState !== state) {
      error.value = t('auth.oauthStateMismatch')
      loading.value = false
      return
    }

    // Exchange code for tokens
    const api = useApi()
    const response = await api.post<{ user: any; token: string }>('/api/auth/microsoft/callback', {
      code,
      state,
    })

    // Clean up
    sessionStorage.removeItem('oauth_state')
    sessionStorage.removeItem('oauth_provider')

    // Set user and redirect
    authStore.setOAuthUser(response.user)
    router.push('/')
  } catch (e: any) {
    error.value = e.message || t('auth.oauthError')
    loading.value = false
  }
})
</script>

<template>
  <div class="w-full max-w-xs space-y-5 relative z-10 text-center">
    <div v-if="loading && !error" class="space-y-3">
      <div class="mx-auto h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      <p class="text-xs text-muted-foreground">{{ t('auth.oauthProcessing') }}</p>
    </div>

    <div v-if="error" class="space-y-3">
      <p class="text-xs text-destructive bg-destructive/10 rounded px-3 py-2">{{ error }}</p>
      <button
        class="inline-flex items-center justify-center rounded text-xs font-medium transition-all duration-150 bg-primary text-primary-foreground hover:bg-primary/85 h-8 px-4"
        @click="router.push('/login')"
      >
        {{ t('auth.backToLogin') }}
      </button>
    </div>
  </div>
</template>
