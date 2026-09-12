<script setup lang="ts">
/**
 * Top-level grid view: orchestrates filtering, sorting and System grouping.
 *
 * Filter state lives in the route query (see docs/services-url-filters.md) so
 * a refresh or a shared link reproduces the exact view. Collapsed state is
 * persisted to localStorage per-tenant (simple string key; no user-scoping
 * since the app is single-session).
 */
import { computed, reactive, watch } from 'vue'
import ServiceCard from './ServiceCard.vue'
import ServiceFilterBar from './ServiceFilterBar.vue'
import SystemGroupHeader from './SystemGroupHeader.vue'
import { applyFilters, groupBySystem, sortComponents } from './cardRegistry'
import { useServiceFilterQuery } from '@/composables/useServiceFilterQuery'
import type {
  ComponentOverview,
  ServicesOverviewResponse,
  SystemSummary,
} from '@/composables/useServicesOverview'

const props = defineProps<{
  data: ServicesOverviewResponse | null
  loading: boolean
}>()

const STORAGE_KEY = 'services.collapsedSystems'

const allComponents = computed<ComponentOverview[]>(() => props.data?.components ?? [])
const allSystems = computed<SystemSummary[]>(() => props.data?.systems ?? [])

const { filters, setFilters } = useServiceFilterQuery({
  knownSystemIds: computed(() => allSystems.value.map(s => s.id)),
  // Until the overview payload lands we cannot tell a stale `?system=` id from
  // a valid one, so the selection is kept as-is.
  systemsReady: computed(() => props.data != null),
})

const collapsed = reactive<Record<string, boolean>>(loadCollapsed())

function loadCollapsed(): Record<string, boolean> {
  if (typeof window === 'undefined') return {}
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY)
    return raw ? JSON.parse(raw) : {}
  } catch {
    return {}
  }
}

watch(collapsed, (v) => {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(v))
  } catch {
    // storage disabled / quota — non-fatal.
  }
}, { deep: true })

const filtered = computed(() =>
  applyFilters(allComponents.value, {
    search: filters.value.search,
    systemId: filters.value.systemId,
    lifecycle: filters.value.lifecycle,
    runtime: filters.value.runtime,
    health: filters.value.health,
  }),
)

const sorted = computed(() => sortComponents(filtered.value, filters.value.sort as 'health' | 'name' | 'incidents'))

const grouped = computed(() => groupBySystem(sorted.value, allSystems.value))

function toggleSystem(id: string | null) {
  const key = id ?? '__ungrouped'
  collapsed[key] = !collapsed[key]
}

function isCollapsed(id: string | null): boolean {
  return !!collapsed[id ?? '__ungrouped']
}

const { t } = useI18n()
</script>

<template>
  <div class="space-y-5">
    <ServiceFilterBar
      :filters="filters"
      :systems="allSystems"
      :total="filtered.length"
      @update:filters="setFilters"
    />

    <!-- Loading skeletons -->
    <div v-if="loading && allComponents.length === 0" class="grid gap-3 grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
      <div v-for="n in 8" :key="n" class="h-36 rounded-lg bg-panel/50 animate-pulse" />
    </div>

    <!-- Empty -->
    <div
      v-else-if="filtered.length === 0"
      class="rounded-lg border border-dashed border-border/50 py-16 text-center space-y-2"
    >
      <p class="text-xs text-muted-foreground">{{ t('services.emptyTitle') }}</p>
      <p class="text-[11px] text-muted-foreground/60">{{ t('services.emptyHint') }}</p>
    </div>

    <!-- Grouped grid -->
    <div v-else class="space-y-6">
      <section v-for="g in grouped" :key="g.system.id ?? '__ungrouped'">
        <SystemGroupHeader
          :system="g.system.id == null
            ? { id: null, name: t('services.ungrouped'), display_name: null }
            : g.system"
          :collapsed="isCollapsed(g.system.id)"
          :health-summary="g.healthSummary"
          :component-count="g.components.length"
          @toggle="toggleSystem(g.system.id)"
        />
        <div
          v-if="!isCollapsed(g.system.id)"
          class="grid gap-3 grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4"
        >
          <ServiceCard
            v-for="c in g.components"
            :key="c.id"
            :component="c"
          />
        </div>
      </section>
    </div>
  </div>
</template>
