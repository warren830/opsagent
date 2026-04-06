<script setup lang="ts">
definePageMeta({ middleware: 'auth' })

const chatFullscreen = useState('chatFullscreen', () => false)
</script>

<template>
  <div class="h-screen flex flex-col overflow-hidden bg-background">
    <LayoutAppHeader />
    <div class="flex flex-1 min-h-0">
      <LayoutAppSidebar v-if="!chatFullscreen" />
      <main v-if="!chatFullscreen" class="flex-1 overflow-auto min-w-0">
        <div class="p-5">
          <slot />
        </div>
      </main>
      <LayoutChatPanel />
    </div>
  </div>
</template>
