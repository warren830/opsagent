<script setup lang="ts">
// Auth middleware is applied per-page via definePageMeta in pages/*.vue.
// Layouts can't set page meta (Vue compiler hint: only pages may call it).
const chatFullscreen = useState('chatFullscreen', () => false)
const mobileSidebarOpen = useState('mobileSidebarOpen', () => false)

// Theme + surface mode drive decoration intensity.
// useTheme applies <html data-theme="aurora">; useSurfaceMode toggles
// <html data-surface="focus"> based on the route.
const { theme } = useTheme()
useSurfaceMode()
</script>

<template>
  <div class="relative min-h-screen overflow-hidden">
    <!-- Decorative background layers -->
    <!-- Light (Sky & Lavender): full-viewport aurora + cursor glow -->
    <template v-if="theme === 'light'">
      <LayoutAuroraBackground />
      <LayoutCursorGlow />
    </template>
    <!-- Aurora (dark): 4-layer atmospheric stack
         z-20 : solid cold-black base
         z-10 : mesh (dot+triangle circuit texture) + halo (bottom aurora)
                + corner glows (top-left indigo, top-right teal)
         z-9  : vignette (edge darkening, above halo but below content)
         z-0+ : everything else sits over this composite -->
    <template v-else>
      <div class="fixed inset-0 -z-20 pointer-events-none bg-background" aria-hidden="true" />
      <LayoutMeshTexture />
      <LayoutAuroraHalo />
      <LayoutAuroraCornerGlow />
      <LayoutAuroraVignette />
    </template>

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
