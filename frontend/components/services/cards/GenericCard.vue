<script setup lang="ts">
/**
 * Fallback card for components that have no runtime declared or whose
 * runtime.kind doesn't match any of the specialised renderers.
 *
 * We keep the middle section intentionally empty (per design §4.7) so the
 * card stays honest: nothing measured, nothing shown.
 */
import { computed } from 'vue'
import ServiceCardFrame from '../ServiceCardFrame.vue'
import type { ComponentOverview } from '@/composables/useServicesOverview'

const props = defineProps<{ component: ComponentOverview }>()

const subtitle = computed(() => props.component.description ?? '')
</script>

<template>
  <ServiceCardFrame :component="component" :subtitle="subtitle">
    <div class="text-[11px] text-muted-foreground/60 italic">
      {{ $t('services.generic.noRuntime') }}
    </div>
  </ServiceCardFrame>
</template>
