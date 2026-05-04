<script setup lang="ts">
import { computed } from 'vue'
import ServiceCardFrame from '../ServiceCardFrame.vue'
import type { ComponentOverview } from '@/composables/useServicesOverview'

const props = defineProps<{ component: ComponentOverview }>()

const rt = computed(() => props.component.runtime ?? {})
const detail = computed(() => props.component.runtime_detail ?? {})

const instanceId = computed(() => (rt.value.instance_id as string | undefined) ?? (detail.value.instance_id as string | undefined))
const region = computed(() => rt.value.region as string | undefined)
const ami = computed(() => detail.value.ami as string | undefined)
const state = computed(() => (detail.value.state as string | undefined) ?? 'unknown')

const subtitle = computed(() => region.value ?? '')

const stateClass = computed(() => {
  switch (state.value) {
    case 'running': return 'text-success'
    case 'stopped': return 'text-muted-foreground'
    case 'pending':
    case 'stopping': return 'text-warning'
    case 'terminated': return 'text-destructive'
    default: return 'text-muted-foreground'
  }
})
</script>

<template>
  <ServiceCardFrame :component="component" :subtitle="subtitle">
    <div class="space-y-1.5">
      <div class="flex items-center justify-between text-[11px]">
        <span class="text-muted-foreground">{{ $t('services.ec2.instance') }}</span>
        <span class="font-mono tabular-nums text-foreground truncate max-w-[140px]">
          {{ instanceId || '—' }}
        </span>
      </div>
      <div v-if="ami" class="flex items-center justify-between text-[11px]">
        <span class="text-muted-foreground">{{ $t('services.ec2.ami') }}</span>
        <span class="font-mono text-muted-foreground/70 truncate max-w-[140px]" :title="ami">
          {{ ami }}
        </span>
      </div>
      <div class="flex items-center justify-between text-[11px]">
        <span class="text-muted-foreground">{{ $t('services.ec2.state') }}</span>
        <span class="font-mono uppercase tracking-wider text-[10px]" :class="stateClass">
          {{ state }}
        </span>
      </div>
    </div>
  </ServiceCardFrame>
</template>
