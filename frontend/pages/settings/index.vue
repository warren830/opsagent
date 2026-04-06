<script setup lang="ts">
import { ref } from 'vue'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'

definePageMeta({ middleware: 'auth' })

const { t, locale } = useI18n()
const api = useApi()
const colorMode = useColorMode()

const activeTab = ref<'general' | 'password'>('general')

// General
const selectedLocale = ref(locale.value)
const selectedTheme = ref(colorMode.preference)

function saveGeneral() {
  locale.value = selectedLocale.value
  colorMode.preference = selectedTheme.value
  toast.success(t('common.success'))
}

// Password
const passwordForm = ref({
  current_password: '',
  new_password: '',
  confirm_password: '',
})
const savingPassword = ref(false)

async function changePassword() {
  if (passwordForm.value.new_password !== passwordForm.value.confirm_password) {
    toast.error('Passwords do not match')
    return
  }
  savingPassword.value = true
  try {
    await api.put('/api/auth/change-password', {
      current_password: passwordForm.value.current_password,
      new_password: passwordForm.value.new_password,
    })
    toast.success(t('common.success'))
    passwordForm.value = { current_password: '', new_password: '', confirm_password: '' }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    savingPassword.value = false
  }
}
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('settings.title') }}</h1>
    </div>

    <!-- Tabs -->
    <div class="flex items-center gap-1.5">
      <Button
        size="sm"
        :variant="activeTab === 'general' ? 'default' : 'outline'"
        @click="activeTab = 'general'"
      >
        {{ t('settings.general') }}
      </Button>
      <Button
        size="sm"
        :variant="activeTab === 'password' ? 'default' : 'outline'"
        @click="activeTab = 'password'"
      >
        {{ t('auth.changePassword') }}
      </Button>
    </div>

    <!-- General Tab -->
    <div v-if="activeTab === 'general'" class="rounded border border-border/60 bg-card p-4 max-w-lg">
      <form class="space-y-3" @submit.prevent="saveGeneral">
        <div class="space-y-1.5">
          <label class="text-xs font-medium">Language</label>
          <Select v-model="selectedLocale">
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="zh">中文</SelectItem>
              <SelectItem value="en">English</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div class="space-y-1.5">
          <label class="text-xs font-medium">Theme</label>
          <Select v-model="selectedTheme">
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="dark">{{ t('theme.dark') }}</SelectItem>
              <SelectItem value="light">{{ t('theme.light') }}</SelectItem>
              <SelectItem value="system">{{ t('theme.system') }}</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div class="pt-1">
          <Button type="submit" size="sm">{{ t('common.save') }}</Button>
        </div>
      </form>
    </div>

    <!-- Password Tab -->
    <div v-if="activeTab === 'password'" class="rounded border border-border/60 bg-card p-4 max-w-lg">
      <form class="space-y-3" @submit.prevent="changePassword">
        <div class="space-y-1.5">
          <label class="text-xs font-medium">{{ t('auth.currentPassword') }}</label>
          <Input v-model="passwordForm.current_password" type="password" :placeholder="t('auth.currentPassword')" required />
        </div>

        <div class="space-y-1.5">
          <label class="text-xs font-medium">{{ t('auth.newPassword') }}</label>
          <Input v-model="passwordForm.new_password" type="password" :placeholder="t('auth.newPassword')" required />
        </div>

        <div class="space-y-1.5">
          <label class="text-xs font-medium">{{ t('auth.confirmPassword') }}</label>
          <Input v-model="passwordForm.confirm_password" type="password" :placeholder="t('auth.confirmPassword')" required />
        </div>

        <div class="pt-1">
          <Button type="submit" size="sm" :disabled="savingPassword">
            {{ savingPassword ? t('common.loading') : t('common.save') }}
          </Button>
        </div>
      </form>
    </div>
  </div>
</template>
