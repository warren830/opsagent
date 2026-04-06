<script setup lang="ts">
import { Zap } from 'lucide-vue-next'

definePageMeta({ layout: 'auth' })

const { t } = useI18n()
const authStore = useAuthStore()
const router = useRouter()

const username = ref('')
const password = ref('')
const error = ref('')
const loading = ref(false)

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
</script>

<template>
  <div class="w-full max-w-xs space-y-5 relative z-10">
    <!-- Logo + header -->
    <div class="text-center space-y-3">
      <div class="mx-auto h-10 w-10 rounded-lg bg-gradient-to-br from-primary to-amber-500 flex items-center justify-center shadow-lg shadow-primary/20">
        <Zap class="h-5 w-5 text-white" />
      </div>
      <div>
        <h1 class="text-lg font-semibold tracking-tight text-foreground">
          {{ t('auth.loginTitle') }}
        </h1>
        <p class="text-xs text-muted-foreground mt-0.5">
          {{ t('auth.loginDescription') }}
        </p>
      </div>
    </div>

    <!-- Login card -->
    <div class="rounded-lg border border-border/60 bg-card p-4 space-y-3">
      <form class="space-y-3" @submit.prevent="handleLogin">
        <div class="space-y-1.5">
          <label class="text-xs font-medium text-foreground" for="username">
            {{ t('auth.username') }}
          </label>
          <input
            id="username"
            v-model="username"
            type="text"
            class="flex h-8 w-full rounded border border-border/60 bg-secondary/50 px-2.5 py-1.5 text-xs ring-offset-background placeholder:text-muted-foreground/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 focus-visible:border-primary/50 transition-colors"
            :placeholder="t('auth.username')"
            required
            autocomplete="username"
          />
        </div>

        <div class="space-y-1.5">
          <label class="text-xs font-medium text-foreground" for="password">
            {{ t('auth.password') }}
          </label>
          <input
            id="password"
            v-model="password"
            type="password"
            class="flex h-8 w-full rounded border border-border/60 bg-secondary/50 px-2.5 py-1.5 text-xs ring-offset-background placeholder:text-muted-foreground/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 focus-visible:border-primary/50 transition-colors"
            :placeholder="t('auth.password')"
            required
            autocomplete="current-password"
          />
        </div>

        <!-- Error message -->
        <p v-if="error" class="text-[11px] text-destructive bg-destructive/10 rounded px-2.5 py-1.5">
          {{ error }}
        </p>

        <button
          type="submit"
          class="inline-flex items-center justify-center rounded text-xs font-medium transition-all duration-150 bg-primary text-primary-foreground hover:bg-primary/85 h-8 w-full shadow-sm shadow-primary/20 active:scale-[0.97]"
          :disabled="loading"
        >
          {{ loading ? t('common.loading') : t('auth.loginButton') }}
        </button>
      </form>
    </div>

    <!-- Theme & Language toggles -->
    <div class="flex justify-center gap-1.5">
      <LayoutThemeToggle />
      <LayoutLangSwitch />
    </div>
  </div>
</template>
