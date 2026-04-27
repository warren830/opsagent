<script setup lang="ts">
definePageMeta({ middleware: 'auth' })

const chatFullscreen = useState('chatFullscreen', () => false)
const mobileSidebarOpen = useState('mobileSidebarOpen', () => false)
</script>

<template>
  <div class="h-screen flex flex-col overflow-hidden bg-background">
    <LayoutAppHeader />
    <div class="flex flex-1 min-h-0">
      <!-- Mobile sidebar overlay -->
      <div
        v-if="mobileSidebarOpen && !chatFullscreen"
        class="fixed inset-0 z-30 bg-slate-900/30 md:hidden"
        @click="mobileSidebarOpen = false"
      />
      <LayoutAppSidebar v-if="!chatFullscreen" />
      <main v-if="!chatFullscreen" class="flex-1 overflow-auto min-w-0">
        <div class="p-5">
          <slot />
        </div>
      </main>
      <LayoutChatPanel class="hidden md:flex" :class="{ '!flex': chatFullscreen }" />
    </div>
  </div>
</template>
