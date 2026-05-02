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
    <!-- Aurora (dark): cinematic atmosphere with Telemetry Topology
         design language. Each layer is individually subtle (4-15%
         contribution); together they produce filmic depth.

         z-20  : solid cold-black base (#0A0B10)
         z-10  : mesh (triangle+dot circuit grid)
         z-10  : hexgrid (breathing cluster-node field, K8s heritage)
         z-10  : halo (bottom aurora band, 4-layer radial)
         z-10  : halo wisps (turbulence — small spots, reverse drift)
         z-10  : corner glows (top-left indigo, top-right teal)
         z-9   : vignette (edge darkening)
         z-8   : noise (film grain, mix-blend overlay)
         z-0+  : app content -->
    <template v-else>
      <div class="fixed inset-0 -z-20 pointer-events-none bg-background" aria-hidden="true" />
      <LayoutMeshTexture />
      <!-- LayoutAuroraHexgrid removed — pending redesign -->
      <LayoutAuroraHalo />
      <LayoutAuroraHaloWisps />
      <LayoutAuroraCornerGlow />
      <LayoutAuroraVignette />
      <LayoutAuroraNoise />
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
