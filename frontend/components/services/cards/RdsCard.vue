<script setup lang="ts">
import { computed } from 'vue'
import ServiceCardFrame from '../ServiceCardFrame.vue'
import type { ComponentOverview } from '@/composables/useServicesOverview'

const props = defineProps<{ component: ComponentOverview }>()

const rt = computed(() => props.component.runtime ?? {})
const detail = computed(() => props.component.runtime_detail ?? {})

const engine = computed(() => (rt.value.engine as string | undefined) ?? (detail.value.engine as string | undefined))
const version = computed(() => detail.value.engine_version as string | undefined)
const multiAz = computed(() => detail.value.multi_az as boolean | undefined)
const connectionCount = computed(() => detail.value.connection_count as number | undefined)
const region = computed(() => rt.value.region as string | undefined)

const subtitle = computed(() => {
  const parts: string[] = []
  if (engine.value) parts.push(engine.value)
  if (version.value) parts.push(`v${version.value}`)
  return parts.join(' · ') || (region.value ?? '')
})
</script>

<template>
  <ServiceCardFrame :component="component" :subtitle="subtitle">
    <div class="space-y-1.5">
      <div class="flex items-center justify-between text-[11px]">
        <span class="text-muted-foreground">{{ $t('services.rds.connections') }}</span>
        <span class="font-mono tabular-nums text-foreground">
          {{ connectionCount ?? '—' }}
        </span>
      </div>
      <div class="flex items-center justify-between text-[11px]">
        <span class="text-muted-foreground">{{ $t('services.rds.multiAz') }}</span>
        <span
          class="font-mono uppercase tracking-wider text-[10px]"
          :class="multiAz ? 'text-success' : 'text-muted-foreground'"
        >
          {{ multiAz == null ? '—' : (multiAz ? $t('common.yes') : $t('common.no')) }}
        </span>
      </div>
      <div v-if="region" class="flex items-center justify-between text-[11px]">
        <span class="text-muted-foreground">{{ $t('services.region') }}</span>
        <span class="font-mono text-muted-foreground/70">{{ region }}</span>
      </div>
    </div>
  </ServiceCardFrame>
</template>
