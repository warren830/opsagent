<script setup lang="ts">
import { LogOut, Menu } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Avatar } from '@/components/ui/avatar'

const authStore = useAuthStore()
const { t } = useI18n()
const mobileSidebarOpen = useState('mobileSidebarOpen', () => false)

async function handleLogout() {
  await authStore.logout()
  navigateTo('/login')
}
</script>

<template>
  <header class="sticky top-0 z-50 w-full border-b border-border/60 bg-card/80 backdrop-blur-xl">
    <div class="flex h-12 items-center px-4 gap-3">
      <!-- Logo -->
      <NuxtLink to="/" class="flex items-center gap-2.5 font-semibold text-base hover:opacity-90 transition-opacity">
        <img src="/logo-icon.png" alt="Ops" class="h-7 w-7" />
        <span class="text-foreground">{{ t('app.name') }}</span>
      </NuxtLink>

      <!-- Mobile menu toggle -->
      <Button
        variant="ghost"
        size="sm"
        class="md:hidden h-8 w-8 p-0 text-muted-foreground"
        @click="mobileSidebarOpen = !mobileSidebarOpen"
      >
        <Menu class="h-4 w-4" />
      </Button>

      <!-- Spacer -->
      <div class="flex-1" />

      <!-- User info -->
      <div v-if="authStore.user" class="flex items-center gap-2">
        <Avatar :fallback="authStore.user.username.charAt(0).toUpperCase()" class="h-6 w-6 text-[10px] bg-secondary text-secondary-foreground" />
        <span class="text-xs text-muted-foreground hidden md:inline">
          {{ authStore.user.username }}
        </span>
      </div>

      <!-- Theme toggle -->
      <LayoutThemeToggle />

      <!-- Language switch -->
      <LayoutLangSwitch />

      <!-- Logout -->
      <Button
        v-if="authStore.isAuthenticated"
        variant="ghost"
        size="sm"
        class="text-muted-foreground hover:text-foreground h-8 px-2 text-xs"
        @click="handleLogout"
      >
        <LogOut class="h-3.5 w-3.5 mr-1" />
        {{ t('auth.logout') }}
      </Button>
    </div>
  </header>
</template>
