<script setup lang="ts">
import { computed } from 'vue'
import ServiceCardFrame from '../ServiceCardFrame.vue'
import type { ComponentOverview } from '@/composables/useServicesOverview'

const props = defineProps<{ component: ComponentOverview }>()

const rt = computed(() => props.component.runtime ?? {})
const detail = computed(() => props.component.runtime_detail ?? {})

const region = computed(() => rt.value.region as string | undefined)
const version = computed(() => (detail.value.version as string | undefined) ?? (detail.value.alias as string | undefined))
const memory = computed(() => detail.value.memory_mb as number | undefined)
const lastInvocation = computed(() => detail.value.last_invocation as string | undefined)
const errorRate = computed(() => detail.value.error_rate_pct as number | undefined)

const subtitle = computed(() => {
  const parts: string[] = []
  if (version.value) parts.push(version.value)
  if (region.value) parts.push(region.value)
  return parts.join(' · ')
})

const errorClass = computed(() => {
  const rate = errorRate.value
  if (rate == null) return 'text-muted-foreground'
  if (rate >= 5) return 'text-destructive'
  if (rate >= 1) return 'text-warning'
  return 'text-success'
})

const lastInvocationShort = computed(() => {
  const raw = lastInvocation.value
  if (!raw) return null
  try {
    const d = new Date(raw)
    const diff = Date.now() - d.getTime()
    const m = Math.floor(diff / 60_000)
    if (m < 1) return '<1m'
    if (m < 60) return `${m}m`
    const h = Math.floor(m / 60)
    if (h < 48) return `${h}h`
    return `${Math.floor(h / 24)}d`
  } catch {
    return raw
  }
})
</script>

<template>
  <ServiceCardFrame :component="component" :subtitle="subtitle">
    <div class="space-y-1.5">
      <div class="flex items-center justify-between text-[11px]">
        <span class="text-muted-foreground">{{ $t('services.lambda.memory') }}</span>
        <span class="font-mono tabular-nums text-foreground">
          <template v-if="memory != null">{{ memory }} MB</template>
          <template v-else>—</template>
        </span>
      </div>
      <div class="flex items-center justify-between text-[11px]">
        <span class="text-muted-foreground">{{ $t('services.lambda.lastInvocation') }}</span>
        <span class="font-mono tabular-nums text-muted-foreground/80">
          {{ lastInvocationShort ?? '—' }}
        </span>
      </div>
      <div class="flex items-center justify-between text-[11px]">
        <span class="text-muted-foreground">{{ $t('services.lambda.errorRate') }}</span>
        <span class="font-mono tabular-nums" :class="errorClass">
          <template v-if="errorRate != null">{{ errorRate.toFixed(2) }}%</template>
          <template v-else>—</template>
        </span>
      </div>
    </div>
  </ServiceCardFrame>
</template>
