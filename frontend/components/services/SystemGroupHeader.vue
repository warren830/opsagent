<script setup lang="ts">
import { ChevronRight } from 'lucide-vue-next'
import type { SystemSummary } from '@/composables/useServicesOverview'

defineProps<{
  system: { id: string | null, name: string, display_name?: string | null }
  collapsed?: boolean
  healthSummary?: SystemSummary['health_summary']
  componentCount?: number
}>()

const emit = defineEmits<{ (e: 'toggle'): void }>()
</script>

<template>
  <button
    type="button"
    class="flex items-center gap-2 w-full text-left mb-2 group"
    @click="emit('toggle')"
  >
    <ChevronRight
      class="h-3 w-3 text-muted-foreground transition-transform"
      :class="{ 'rotate-90': !collapsed }"
    />
    <h2 class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground group-hover:text-foreground transition-colors">
      {{ system.display_name || system.name }}
    </h2>
    <span v-if="typeof componentCount === 'number'" class="text-[10px] text-muted-foreground/60 tabular-nums">
      {{ componentCount }}
    </span>
    <div v-if="healthSummary" class="flex items-center gap-1.5 ml-1">
      <span v-if="healthSummary.critical > 0" class="flex items-center gap-0.5 text-[10px] text-destructive tabular-nums">
        <span class="h-1.5 w-1.5 rounded-full bg-destructive" />
        {{ healthSummary.critical }}
      </span>
      <span v-if="healthSummary.warning > 0" class="flex items-center gap-0.5 text-[10px] text-warning tabular-nums">
        <span class="h-1.5 w-1.5 rounded-full bg-warning" />
        {{ healthSummary.warning }}
      </span>
      <span v-if="healthSummary.healthy > 0" class="flex items-center gap-0.5 text-[10px] text-success tabular-nums">
        <span class="h-1.5 w-1.5 rounded-full bg-success" />
        {{ healthSummary.healthy }}
      </span>
    </div>
  </button>
</template>
