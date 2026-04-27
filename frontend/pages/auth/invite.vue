<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Lock, CheckCircle, AlertTriangle } from 'lucide-vue-next'

definePageMeta({ layout: 'auth' })

const { t } = useI18n()
const router = useRouter()
const route = useRoute()
const api = useApi()

const state = ref<'validating' | 'ready' | 'submitting' | 'success' | 'expired' | 'error'>('validating')
const email = ref('')
const password = ref('')
const confirmPassword = ref('')
const errorMsg = ref('')

const token = computed(() => route.query.token as string)

onMounted(async () => {
  if (!token.value) {
    state.value = 'error'
    errorMsg.value = t('invite.invalidToken')
    return
  }
  try {
    const resp = await api.get<{ email: string; username: string }>(`/api/auth/invite/${token.value}`)
    email.value = resp.email
    state.value = 'ready'
  } catch (e: any) {
    if (e.message?.includes('expired')) {
      state.value = 'expired'
    } else {
      state.value = 'error'
      errorMsg.value = e.message || t('invite.invalidToken')
    }
  }
})

async function submit() {
  errorMsg.value = ''
  if (password.value.length < 8) {
    errorMsg.value = t('invite.passwordMinLength')
    return
  }
  if (password.value !== confirmPassword.value) {
    errorMsg.value = t('invite.passwordMismatch')
    return
  }
  state.value = 'submitting'
  try {
    await api.post(`/api/auth/invite/${token.value}/redeem`, { password: password.value })
    state.value = 'success'
    setTimeout(() => router.push('/login'), 2000)
  } catch (e: any) {
    state.value = 'ready'
    errorMsg.value = e.message || t('common.error')
  }
}
</script>

<template>
  <div class="w-full max-w-xs space-y-5 relative z-10">
    <!-- Validating -->
    <div v-if="state === 'validating'" class="text-center space-y-3">
      <div class="mx-auto h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      <p class="text-xs text-muted-foreground">{{ t('common.loading') }}</p>
    </div>

    <!-- Expired -->
    <div v-else-if="state === 'expired'" class="text-center space-y-3">
      <AlertTriangle class="mx-auto h-8 w-8 text-warning" />
      <p class="text-xs text-muted-foreground">{{ t('invite.expired') }}</p>
      <Button size="sm" variant="outline" @click="router.push('/login')">
        {{ t('auth.backToLogin') }}
      </Button>
    </div>

    <!-- Error -->
    <div v-else-if="state === 'error'" class="text-center space-y-3">
      <AlertTriangle class="mx-auto h-8 w-8 text-destructive" />
      <p class="text-xs text-destructive bg-destructive/10 rounded px-3 py-2">{{ errorMsg }}</p>
      <Button size="sm" variant="outline" @click="router.push('/login')">
        {{ t('auth.backToLogin') }}
      </Button>
    </div>

    <!-- Success -->
    <div v-else-if="state === 'success'" class="text-center space-y-3">
      <CheckCircle class="mx-auto h-8 w-8 text-emerald-500" />
      <p class="text-sm font-medium text-foreground">{{ t('invite.success') }}</p>
      <p class="text-xs text-muted-foreground">{{ t('invite.redirecting') }}</p>
    </div>

    <!-- Set password form -->
    <template v-else>
      <div class="text-center space-y-1">
        <h1 class="text-base font-semibold text-foreground">{{ t('invite.title') }}</h1>
        <p class="text-xs text-muted-foreground">{{ t('invite.description') }}</p>
      </div>

      <form class="space-y-3" @submit.prevent="submit">
        <div class="space-y-1.5">
          <label class="text-xs font-medium text-muted-foreground">{{ t('user.email') }}</label>
          <Input :model-value="email" disabled class="opacity-60" />
        </div>

        <div class="space-y-1.5">
          <label class="text-xs font-medium">{{ t('invite.setPassword') }}</label>
          <Input v-model="password" type="password" :placeholder="t('invite.setPassword')" autofocus />
        </div>

        <div class="space-y-1.5">
          <label class="text-xs font-medium">{{ t('invite.confirmPassword') }}</label>
          <Input v-model="confirmPassword" type="password" :placeholder="t('invite.confirmPassword')" />
        </div>

        <p v-if="errorMsg" class="text-xs text-destructive bg-destructive/10 rounded px-3 py-2">{{ errorMsg }}</p>

        <Button type="submit" class="w-full" size="sm" :disabled="state === 'submitting'">
          <Lock class="h-3.5 w-3.5" />
          {{ state === 'submitting' ? t('common.loading') : t('invite.setPassword') }}
        </Button>
      </form>
    </template>
  </div>
</template>
