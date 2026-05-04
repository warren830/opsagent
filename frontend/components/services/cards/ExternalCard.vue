<script setup lang="ts">
import { computed } from 'vue'
import ServiceCardFrame from '../ServiceCardFrame.vue'
import type { ComponentOverview } from '@/composables/useServicesOverview'

const props = defineProps<{ component: ComponentOverview }>()

const rt = computed(() => props.component.runtime ?? {})
const detail = computed(() => props.component.runtime_detail ?? {})

const baseUrl = computed(() => rt.value.base_url as string | undefined)
const healthUrl = computed(() => (rt.value.health_url as string | undefined) ?? (detail.value.health_url as string | undefined))
const rttMs = computed(() => detail.value.rtt_ms as number | undefined)

const baseHost = computed(() => {
  const u = baseUrl.value
  if (!u) return null
  try {
    return new URL(u).host
  } catch {
    return u
  }
})

const rttClass = computed(() => {
  const v = rttMs.value
  if (v == null) return 'text-muted-foreground'
  if (v >= 2000) return 'text-destructive'
  if (v >= 500) return 'text-warning'
  return 'text-success'
})
</script>

<template>
  <ServiceCardFrame :component="component" :subtitle="baseHost ?? ''">
    <div class="space-y-1.5">
      <div v-if="baseUrl" class="text-[11px] font-mono text-muted-foreground/70 truncate" :title="baseUrl">
        {{ baseUrl }}
      </div>
      <div v-else class="text-[10px] text-muted-foreground/50 italic">
        {{ $t('services.external.noBaseUrl') }}
      </div>
      <div v-if="healthUrl" class="flex items-center justify-between text-[11px]">
        <span class="text-muted-foreground">{{ $t('services.external.rtt') }}</span>
        <span class="font-mono tabular-nums" :class="rttClass">
          <template v-if="rttMs != null">{{ rttMs }} ms</template>
          <template v-else>—</template>
        </span>
      </div>
    </div>
  </ServiceCardFrame>
</template>
