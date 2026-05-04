<script setup lang="ts">
/**
 * Shared card chrome used by every runtime-specific card. Handles:
 *  - Header row (runtime icon, display name, subtitle, health dot)
 *  - Footer (tags, system, owner, SLO/INC counts)
 *  - Click → navigate to /services/:id
 *
 * Individual cards (EksCard, Ec2Card, …) project only the middle section
 * through the default slot so each runtime can declare its own metrics.
 */
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { AlertOctagon, Target } from 'lucide-vue-next'
import { Badge } from '@/components/ui/badge'
import HealthIndicator from './HealthIndicator.vue'
import RuntimeIcon from './RuntimeIcon.vue'
import type { ComponentOverview } from '@/composables/useServicesOverview'

const props = defineProps<{
  component: ComponentOverview
  subtitle?: string
}>()

const router = useRouter()

const displayName = computed(() => props.component.display_name || props.component.name)
const runtimeKind = computed(() => props.component.runtime?.kind ?? null)

function lifecycleVariant(lc: string): 'success' | 'info' | 'secondary' | 'destructive' {
  switch (lc) {
    case 'production': return 'success'
    case 'experimental': return 'info'
    case 'deprecated': return 'secondary'
    case 'retired': return 'destructive'
    default: return 'secondary'
  }
}

function open() {
  router.push(`/services/${props.component.id}`)
}
</script>

<template>
  <article
    role="link"
    :tabindex="0"
    class="group rounded-lg border border-border/60 bg-panel/70 p-3.5 hover:border-primary/40 hover:shadow-[0_0_12px_hsl(var(--primary)/0.15)] transition-all cursor-pointer flex flex-col gap-2.5"
    :title="component.health_reason"
    @click="open"
    @keydown.enter="open"
  >
    <!-- Header: icon + name + health -->
    <div class="flex items-start justify-between gap-2">
      <div class="flex items-center gap-2 min-w-0">
        <RuntimeIcon :kind="runtimeKind" />
        <div class="min-w-0">
          <h3 class="text-sm font-medium text-foreground truncate">{{ displayName }}</h3>
          <p v-if="subtitle" class="text-[10px] text-muted-foreground/70 truncate mt-0.5">
            {{ subtitle }}
          </p>
          <p v-else-if="component.display_name" class="text-[10px] font-mono text-muted-foreground/60 truncate mt-0.5">
            {{ component.name }}
          </p>
        </div>
      </div>
      <HealthIndicator :status="component.health" class="mt-1" />
    </div>

    <!-- Runtime-specific middle -->
    <div class="border-t border-border/30 pt-2">
      <slot />
    </div>

    <!-- Tags -->
    <div v-if="component.tags.length > 0 || component.lifecycle" class="flex flex-wrap items-center gap-1">
      <Badge :variant="lifecycleVariant(component.lifecycle)" class="text-[9px]">
        {{ component.lifecycle }}
      </Badge>
      <Badge
        v-for="tag in component.tags.slice(0, 3)"
        :key="tag"
        variant="secondary"
        class="text-[9px] opacity-80"
      >
        {{ tag }}
      </Badge>
    </div>

    <!-- Footer: SLOs + incidents -->
    <div class="flex items-center justify-between gap-2 pt-1.5 border-t border-border/30 text-[11px] text-muted-foreground">
      <div class="flex items-center gap-1">
        <Target class="h-3 w-3" />
        <span class="tabular-nums">{{ component.slo_summary.total }}</span>
        <span class="text-[10px]">{{ $t('services.slosShort') }}</span>
      </div>
      <div
        class="flex items-center gap-1"
        :class="component.active_incident_count > 0 ? 'text-destructive' : ''"
      >
        <AlertOctagon class="h-3 w-3" />
        <span class="tabular-nums">{{ component.active_incident_count }}</span>
        <span class="text-[10px]">{{ $t('services.incidentsShort') }}</span>
      </div>
    </div>
  </article>
</template>
