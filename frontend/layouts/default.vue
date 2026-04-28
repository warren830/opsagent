<script setup lang="ts">
// Auth middleware is applied per-page via definePageMeta in pages/*.vue.
// Layouts can't set page meta (Vue compiler hint: only pages may call it).
const chatFullscreen = useState('chatFullscreen', () => false)
const mobileSidebarOpen = useState('mobileSidebarOpen', () => false)
</script>

<template>
  <div class="relative min-h-screen overflow-hidden">
    <!-- Decorative background layers -->
    <LayoutAuroraBackground />
    <LayoutCursorGlow />

    <!-- Floating Glass Islands grid -->
    <div class="relative z-10 flex h-screen gap-2.5 p-2.5">
      <!-- Mobile sidebar overlay (tap-to-close) -->
      <div
        v-if="mobileSidebarOpen && !chatFullscreen"
        class="fixed inset-0 z-30 bg-slate-900/30 backdrop-blur-sm md:hidden"
        @click="mobileSidebarOpen = false"
      />

      <!-- Island 1: Sidebar (hidden when chat fullscreen) -->
      <LayoutAppSidebar v-if="!chatFullscreen" />

      <!-- Middle column: Topbar + Content, stacked -->
      <div v-if="!chatFullscreen" class="flex min-w-0 flex-1 flex-col gap-2.5">
        <!-- Island 2: Topbar -->
        <LayoutAppHeader />

        <!-- Island 3: Content -->
        <main class="glass-panel min-w-0 flex-1 overflow-auto p-5">
          <slot />
        </main>
      </div>

      <!-- Island 4: Chat Panel -->
      <LayoutChatPanel class="hidden md:flex" :class="{ '!flex !flex-1': chatFullscreen }" />
    </div>
  </div>
</template>
