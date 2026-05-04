<script setup lang="ts">
import { computed } from 'vue'
import ServiceCardFrame from '../ServiceCardFrame.vue'
import type { ComponentOverview } from '@/composables/useServicesOverview'

const props = defineProps<{ component: ComponentOverview }>()

const rt = computed(() => props.component.runtime ?? {})
const detail = computed(() => props.component.runtime_detail ?? {})

const cluster = computed(() => rt.value.cluster as string | undefined)
const namespace = computed(() => rt.value.namespace as string | undefined)
const workload = computed(() => rt.value.workload as string | undefined)
const podReady = computed(() => detail.value.pod_ready as number | undefined)
const podDesired = computed(() => detail.value.pod_desired as number | undefined)
const image = computed(() => detail.value.image as string | undefined)

const subtitle = computed(() => {
  const parts: string[] = []
  if (cluster.value) parts.push(cluster.value)
  if (namespace.value) parts.push(namespace.value)
  return parts.join(' · ')
})

const podClass = computed(() => {
  const r = podReady.value
  const d = podDesired.value
  if (r == null || d == null) return 'text-muted-foreground'
  if (r === 0) return 'text-destructive'
  if (r < d) return 'text-warning'
  return 'text-success'
})
</script>

<template>
  <ServiceCardFrame :component="component" :subtitle="subtitle">
    <div class="space-y-1.5">
      <div class="flex items-center gap-2 text-[11px]">
        <span class="text-muted-foreground">{{ $t('services.eks.pods') }}</span>
        <span class="font-mono tabular-nums" :class="podClass">
          <template v-if="podReady != null && podDesired != null">
            {{ podReady }}/{{ podDesired }}
          </template>
          <template v-else>—</template>
        </span>
        <span v-if="workload" class="text-[10px] text-muted-foreground/60 font-mono truncate ml-auto">
          {{ workload }}
        </span>
      </div>
      <div v-if="image" class="text-[10px] font-mono text-muted-foreground/70 truncate" :title="image">
        {{ image }}
      </div>
      <div v-else class="text-[10px] text-muted-foreground/50 italic">
        {{ $t('services.noImage') }}
      </div>
    </div>
  </ServiceCardFrame>
</template>
