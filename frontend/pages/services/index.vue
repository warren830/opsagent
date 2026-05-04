<script setup lang="ts">
/**
 * Services v2 — multi-runtime card grid.
 *
 * Replaces the legacy flat list with a System-grouped, health-sorted grid
 * that dispatches to runtime-specific cards (EKS/EC2/RDS/Lambda/External/
 * Generic). Data comes from /api/services/overview (aggregated by backend
 * U2); if that endpoint isn't available yet the composable silently joins
 * /api/catalog/entities + SLOs + incidents client-side.
 */
import { onMounted, onUnmounted } from 'vue'
import { Layers, RefreshCw } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import ServiceCardGrid from '@/components/services/ServiceCardGrid.vue'
import { useServicesOverview } from '@/composables/useServicesOverview'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const { data, loading, source, fetchOnce, startPolling, stopPolling } = useServicesOverview()

onMounted(() => {
  startPolling()
})

onUnmounted(() => {
  stopPolling()
})
</script>

<template>
  <div class="space-y-5">
    <!-- Header -->
    <div class="flex items-center justify-between gap-3 flex-wrap">
      <div>
        <h1 class="text-base font-semibold text-foreground flex items-center gap-2">
          <Layers class="h-4 w-4 text-primary" />
          {{ t('services.title') }}
        </h1>
        <p class="text-[11px] text-muted-foreground mt-0.5">
          {{ t('services.subtitleV2') }}
        </p>
        <p v-if="source === 'fallback'" class="text-[10px] text-warning/80 mt-0.5">
          {{ t('services.fallbackHint') }}
        </p>
      </div>

      <Button variant="outline" size="sm" :disabled="loading" @click="fetchOnce">
        <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': loading }" />
        {{ t('common.refresh') }}
      </Button>
    </div>

    <ServiceCardGrid :data="data" :loading="loading" />
  </div>
</template>
