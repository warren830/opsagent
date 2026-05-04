<script setup lang="ts">
/**
 * Thin dispatcher — picks the runtime-specific card. The kind→component
 * mapping is inline here so `cardRegistry.ts` stays Vue-free (and thus
 * testable without `@vitejs/plugin-vue`).
 */
import { computed } from 'vue'
import { resolveCardKind } from './cardRegistry'
import EksCard from './cards/EksCard.vue'
import Ec2Card from './cards/Ec2Card.vue'
import RdsCard from './cards/RdsCard.vue'
import LambdaCard from './cards/LambdaCard.vue'
import ExternalCard from './cards/ExternalCard.vue'
import GenericCard from './cards/GenericCard.vue'
import type { ComponentOverview } from '@/composables/useServicesOverview'

const props = defineProps<{ component: ComponentOverview }>()

const CardComponent = computed(() => {
  switch (resolveCardKind(props.component.runtime?.kind)) {
    case 'eks': return EksCard
    case 'ec2': return Ec2Card
    case 'rds': return RdsCard
    case 'lambda': return LambdaCard
    case 'external': return ExternalCard
    case 'generic':
    default:
      return GenericCard
  }
})
</script>

<template>
  <component :is="CardComponent" :component="component" />
</template>
