<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Separator } from '@/components/ui/separator'

definePageMeta({ layout: 'auth' })

const { t } = useI18n()
const authStore = useAuthStore()
const router = useRouter()

const username = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)
const oauthLoading = ref('')

onMounted(async () => {
  await authStore.fetchProviders()
})

const showLocalForm = computed(() => authStore.providers?.local === true)
const showMicrosoft = computed(() => authStore.providers?.microsoft === true)
const showCognito = computed(() => authStore.providers?.cognito === true)
const showOAuth = computed(() => showMicrosoft.value || showCognito.value)
const showDivider = computed(() => showLocalForm.value && showOAuth.value)

async function handleLogin() {
  error.value = ''
  loading.value = true
  try {
    await authStore.login(username.value, password.value)
    router.push('/')
  } catch (e: any) {
    error.value = e.message || t('auth.loginError')
  } finally {
    loading.value = false
  }
}

async function handleOAuthLogin(provider: 'microsoft' | 'cognito') {
  error.value = ''
  oauthLoading.value = provider
  try {
    const api = useApi()
    const response = await api.get<{ auth_url: string; state: string }>(`/api/auth/${provider}/login`)
    sessionStorage.setItem('oauth_state', response.state)
    sessionStorage.setItem('oauth_provider', provider)
    window.location.href = response.auth_url
  } catch (e: any) {
    error.value = e.message || t('auth.oauthError')
    oauthLoading.value = ''
  }
}
</script>

<template>
  <div class="w-full max-w-[420px] space-y-6 px-4">
    <!-- Logo + title -->
    <div class="text-center space-y-3">
      <SharedBrandMark :size="80" class="mx-auto drop-shadow-md" />
      <h1 class="text-2xl font-semibold tracking-tight text-slate-900">{{ t('auth.loginTitle') }}</h1>
      <p class="text-sm text-slate-500">{{ t('auth.loginDescription') }}</p>
    </div>

    <!-- Glass card -->
    <div class="glass-panel p-8 space-y-6">

      <!-- OAuth buttons -->
      <div v-if="showOAuth" class="space-y-2.5">
        <Button
          v-if="showMicrosoft"
          variant="outline"
          class="w-full h-11 rounded-lg bg-white/70 border-slate-200 hover:bg-white text-slate-700 transition-all"
          :disabled="!!oauthLoading"
          @click="handleOAuthLogin('microsoft')"
        >
          <IconsMicrosoftIcon class="h-4 w-4 mr-2.5" />
          {{ oauthLoading === 'microsoft' ? t('common.loading') : t('auth.continueWithMicrosoft') }}
        </Button>
        <Button
          v-if="showCognito"
          variant="outline"
          class="w-full h-11 rounded-lg bg-white/70 border-slate-200 hover:bg-white text-slate-700 transition-all"
          :disabled="!!oauthLoading"
          @click="handleOAuthLogin('cognito')"
        >
          <IconsAwsIcon class="h-4 w-4 mr-2.5" />
          {{ oauthLoading === 'cognito' ? t('common.loading') : t('auth.continueWithCognito') }}
        </Button>
      </div>

      <!-- Divider -->
      <div v-if="showDivider" class="flex items-center gap-4">
        <Separator class="flex-1 bg-slate-200" />
        <span class="text-[10px] text-slate-400 uppercase tracking-[0.2em]">{{ t('auth.orContinueWith') }}</span>
        <Separator class="flex-1 bg-slate-200" />
      </div>

      <!-- Username/password form -->
      <form v-if="showLocalForm" class="space-y-5" @submit.prevent="handleLogin">
        <div class="space-y-2">
          <label class="text-[11px] font-medium text-slate-600 uppercase tracking-[0.15em]" for="username">{{ t('auth.username') }}</label>
          <Input
            id="username" v-model="username" type="text" required autocomplete="username"
            :placeholder="t('auth.username')"
            class="h-11 rounded-lg border-slate-200 bg-white/80 px-4 text-sm text-slate-900 placeholder:text-slate-400 focus:border-primary focus:bg-white transition-all"
          />
        </div>
        <div class="space-y-2">
          <label class="text-[11px] font-medium text-slate-600 uppercase tracking-[0.15em]" for="password">{{ t('auth.password') }}</label>
          <Input
            id="password" v-model="password" type="password" required autocomplete="current-password"
            :placeholder="t('auth.password')"
            class="h-11 rounded-lg border-slate-200 bg-white/80 px-4 text-sm text-slate-900 placeholder:text-slate-400 focus:border-primary focus:bg-white transition-all"
          />
        </div>

        <p v-if="error" class="text-xs text-red-600 bg-red-50 border border-red-200 rounded-lg px-3 py-2">{{ error }}</p>

        <Button
          type="submit" :disabled="loading"
          class="w-full h-11 rounded-lg text-sm font-semibold text-white bg-gradient-to-r from-sky-500 to-violet-500 hover:brightness-110 shadow-md shadow-sky-500/20 hover:shadow-sky-500/30 transition-all"
        >
          {{ loading ? t('common.loading') : t('auth.loginButton') }}
        </Button>
      </form>

      <p v-if="error && !showLocalForm" class="text-xs text-red-600">{{ error }}</p>
    </div>

    <!-- Language switch -->
    <div class="flex justify-center opacity-50 hover:opacity-80 transition-opacity">
      <LayoutLangSwitch />
    </div>
  </div>
</template>
