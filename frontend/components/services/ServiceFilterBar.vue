<script setup lang="ts">
import { Search } from 'lucide-vue-next'
import { Input } from '@/components/ui/input'
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select'
import {
  DEFAULT_SERVICE_FILTERS,
  type ServiceFilters,
} from '@/composables/serviceFilterQuery'
import type { SystemSummary } from '@/composables/useServicesOverview'

const props = defineProps<{
  filters: ServiceFilters
  systems: SystemSummary[]
  total: number
}>()

const emit = defineEmits<{
  (e: 'update:filters', v: ServiceFilters): void
}>()

function update<K extends keyof ServiceFilters>(key: K, value: ServiceFilters[K]) {
  emit('update:filters', { ...props.filters, [key]: value })
}

const { t } = useI18n()
</script>

<template>
  <div class="flex flex-wrap items-center gap-2">
    <div class="relative flex-1 min-w-[200px] max-w-md">
      <Search class="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground/60" />
      <Input
        :model-value="filters.search"
        class="pl-7"
        :placeholder="t('services.searchPlaceholder')"
        @update:model-value="(v) => update('search', String(v ?? ''))"
      />
    </div>

    <Select
      :model-value="filters.systemId"
      @update:model-value="(v) => update('systemId', String(v ?? DEFAULT_SERVICE_FILTERS.systemId))"
    >
      <SelectTrigger class="w-40 h-8 text-xs">
        <SelectValue :placeholder="t('services.filterBySystem')" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="all">{{ t('services.allSystems') }}</SelectItem>
        <SelectItem v-for="s in systems" :key="s.id" :value="s.id">
          {{ s.display_name || s.name }}
        </SelectItem>
        <SelectItem value="none">{{ t('services.ungrouped') }}</SelectItem>
      </SelectContent>
    </Select>

    <Select
      :model-value="filters.lifecycle"
      @update:model-value="(v) => update('lifecycle', String(v ?? DEFAULT_SERVICE_FILTERS.lifecycle))"
    >
      <SelectTrigger class="w-36 h-8 text-xs">
        <SelectValue :placeholder="t('catalog.lifecycleAll')" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="all">{{ t('catalog.lifecycleAll') }}</SelectItem>
        <SelectItem value="production">production</SelectItem>
        <SelectItem value="experimental">experimental</SelectItem>
        <SelectItem value="deprecated">deprecated</SelectItem>
        <SelectItem value="retired">retired</SelectItem>
      </SelectContent>
    </Select>

    <Select
      :model-value="filters.runtime"
      @update:model-value="(v) => update('runtime', String(v ?? DEFAULT_SERVICE_FILTERS.runtime))"
    >
      <SelectTrigger class="w-36 h-8 text-xs">
        <SelectValue :placeholder="t('services.filterByRuntime')" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="all">{{ t('services.allRuntimes') }}</SelectItem>
        <SelectItem value="eks">EKS</SelectItem>
        <SelectItem value="ec2">EC2</SelectItem>
        <SelectItem value="rds">RDS</SelectItem>
        <SelectItem value="lambda">Lambda</SelectItem>
        <SelectItem value="external">External</SelectItem>
        <SelectItem value="generic">Generic</SelectItem>
      </SelectContent>
    </Select>

    <Select
      :model-value="filters.health"
      @update:model-value="(v) => update('health', String(v ?? DEFAULT_SERVICE_FILTERS.health))"
    >
      <SelectTrigger class="w-36 h-8 text-xs">
        <SelectValue :placeholder="t('services.filterByHealth')" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="all">{{ t('services.allHealth') }}</SelectItem>
        <SelectItem value="critical">{{ t('services.healthCritical') }}</SelectItem>
        <SelectItem value="warning">{{ t('services.healthWarning') }}</SelectItem>
        <SelectItem value="unknown">{{ t('services.healthUnknown') }}</SelectItem>
        <SelectItem value="healthy">{{ t('services.healthHealthy') }}</SelectItem>
      </SelectContent>
    </Select>

    <Select
      :model-value="filters.sort"
      @update:model-value="(v) => update('sort', String(v ?? DEFAULT_SERVICE_FILTERS.sort))"
    >
      <SelectTrigger class="w-36 h-8 text-xs">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="health">{{ t('services.sortHealth') }}</SelectItem>
        <SelectItem value="name">{{ t('services.sortName') }}</SelectItem>
        <SelectItem value="incidents">{{ t('services.sortIncidents') }}</SelectItem>
      </SelectContent>
    </Select>

    <span class="text-[11px] text-muted-foreground/70 tabular-nums ml-auto">
      {{ t('services.totalCount', { n: total }) }}
    </span>
  </div>
</template>
