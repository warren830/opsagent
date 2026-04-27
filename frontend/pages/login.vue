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
  <div class="w-full max-w-[420px] space-y-8 relative z-10 px-4">
    <!-- Logo + header -->
    <div class="text-center space-y-3">
      <img src="/logo-icon.png" alt="Ops" class="mx-auto h-36 w-auto drop-shadow-2xl" />
      <h1 class="text-2xl font-bold text-white">{{ t('auth.loginTitle') }}</h1>
      <p class="text-sm text-white/90">{{ t('auth.loginDescription') }}</p>
    </div>

    <!-- Card — ultra-transparent floating glass -->
    <div class="rounded-2xl border border-white/[0.04] bg-white/[0.015] backdrop-blur-sm p-9 space-y-6">

      <!-- OAuth -->
      <div v-if="showOAuth" class="space-y-2.5">
        <Button
          v-if="showMicrosoft"
          variant="outline"
          class="w-full h-11 rounded-xl border-white/[0.05] bg-white/[0.02] hover:bg-white/[0.05] text-white/90 hover:text-white transition-all duration-200 active:scale-[0.98]"
          :disabled="!!oauthLoading"
          @click="handleOAuthLogin('microsoft')"
        >
          <IconsMicrosoftIcon class="h-4 w-4 mr-2.5" />
          {{ oauthLoading === 'microsoft' ? t('common.loading') : t('auth.continueWithMicrosoft') }}
        </Button>
        <Button
          v-if="showCognito"
          variant="outline"
          class="w-full h-11 rounded-xl border-white/[0.05] bg-white/[0.02] hover:bg-white/[0.05] text-white/90 hover:text-white transition-all duration-200 active:scale-[0.98]"
          :disabled="!!oauthLoading"
          @click="handleOAuthLogin('cognito')"
        >
          <IconsAwsIcon class="h-4 w-4 mr-2.5" />
          {{ oauthLoading === 'cognito' ? t('common.loading') : t('auth.continueWithCognito') }}
        </Button>
      </div>

      <!-- Divider -->
      <div v-if="showDivider" class="flex items-center gap-4">
        <Separator class="flex-1 bg-white/[0.04]" />
        <span class="text-[10px] text-white/40 uppercase tracking-[0.2em]">{{ t('auth.orContinueWith') }}</span>
        <Separator class="flex-1 bg-white/[0.04]" />
      </div>

      <!-- Form -->
      <form v-if="showLocalForm" class="space-y-5" @submit.prevent="handleLogin">
        <div class="space-y-2">
          <label class="text-[11px] font-medium text-white/90 uppercase tracking-[0.15em]" for="username">{{ t('auth.username') }}</label>
          <Input
            id="username" v-model="username" type="text" required autocomplete="username"
            :placeholder="t('auth.username')"
            class="h-11 rounded-xl border-white/[0.05] bg-white/[0.02] px-4 text-sm text-white placeholder:text-white/30 focus:border-primary/30 focus:bg-white/[0.04] transition-all duration-200"
          />
        </div>
        <div class="space-y-2">
          <label class="text-[11px] font-medium text-white/90 uppercase tracking-[0.15em]" for="password">{{ t('auth.password') }}</label>
          <Input
            id="password" v-model="password" type="password" required autocomplete="current-password"
            :placeholder="t('auth.password')"
            class="h-11 rounded-xl border-white/[0.05] bg-white/[0.02] px-4 text-sm text-white placeholder:text-white/30 focus:border-primary/30 focus:bg-white/[0.04] transition-all duration-200"
          />
        </div>

        <p v-if="error" class="text-xs text-red-400/80 bg-red-500/5 border border-red-500/10 rounded-lg px-3 py-2">{{ error }}</p>

        <Button
          type="submit" :disabled="loading"
          class="w-full h-11 rounded-xl text-sm font-semibold text-white bg-gradient-to-r from-primary to-orange-500 hover:brightness-110 shadow-lg shadow-primary/20 hover:shadow-primary/35 transition-all duration-200 active:scale-[0.97]"
        >
          {{ loading ? t('common.loading') : t('auth.loginButton') }}
        </Button>
      </form>

      <p v-if="error && !showLocalForm" class="text-xs text-red-400/80">{{ error }}</p>
    </div>

    <!-- Bottom toggles -->
    <div class="flex justify-center gap-2 opacity-30 hover:opacity-70 transition-opacity">
      <LayoutLangSwitch />
    </div>
  </div>
</template>
