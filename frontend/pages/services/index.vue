<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { Search, Layers, Target, AlertOctagon, RefreshCw } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const router = useRouter()

// ───────────────────────── Types ─────────────────────────

interface CatalogEntity {
  id: string
  kind: string
  name: string
  display_name: string | null
  description: string | null
  lifecycle: string
  owner_group_id: string | null
  system_id: string | null
  tags: string[]
  spec: Record<string, unknown>
  updated_at: string
}

interface Slo {
  id: string
  component_id: string | null
  enabled: boolean
}

interface BudgetSnapshot {
  slo_id: string
  budget_remaining_pct: number
  burn_rate_1h: number | null
  burn_rate_6h: number | null
}

interface Incident {
  id: string
  affected_component_ids: string[]
  resolved_at: string | null
  closed_at: string | null
  severity: string
}

// ───────────────────────── State ─────────────────────────

const loading = ref(true)
const entities = ref<CatalogEntity[]>([])
const systems = ref<CatalogEntity[]>([])
const slos = ref<Slo[]>([])
const budgetMap = ref<Record<string, BudgetSnapshot>>({})
const incidents = ref<Incident[]>([])

const search = ref('')
const lifecycleFilter = ref<string>('all')
const systemFilter = ref<string>('all')

// ───────────────────────── Derived ───────────────────────

const sloCountByComponent = computed(() => {
  const map = new Map<string, number>()
  for (const s of slos.value) {
    if (!s.component_id || !s.enabled) continue
    map.set(s.component_id, (map.get(s.component_id) || 0) + 1)
  }
  return map
})

const worstBurnByComponent = computed(() => {
  const map = new Map<string, number>()
  for (const s of slos.value) {
    if (!s.component_id) continue
    const snap = budgetMap.value[s.id]
    if (!snap) continue
    const rates = [snap.burn_rate_1h, snap.burn_rate_6h].filter(
      (v): v is number => typeof v === 'number' && Number.isFinite(v),
    )
    for (const r of rates) {
      const prev = map.get(s.component_id)
      if (prev === undefined || r > prev) map.set(s.component_id, r)
    }
  }
  return map
})

const activeIncidentsByComponent = computed(() => {
  const map = new Map<string, number>()
  for (const inc of incidents.value) {
    if (inc.resolved_at || inc.closed_at) continue
    for (const cid of inc.affected_component_ids) {
      map.set(cid, (map.get(cid) || 0) + 1)
    }
  }
  return map
})

const filtered = computed(() => {
  const q = search.value.trim().toLowerCase()
  return entities.value.filter((e) => {
    if (lifecycleFilter.value !== 'all' && e.lifecycle !== lifecycleFilter.value) return false
    if (systemFilter.value !== 'all' && e.system_id !== systemFilter.value) return false
    if (!q) return true
    return (
      e.name.toLowerCase().includes(q)
      || (e.display_name || '').toLowerCase().includes(q)
      || (e.description || '').toLowerCase().includes(q)
      || e.tags.some(tag => tag.toLowerCase().includes(q))
    )
  })
})

function lifecycleVariant(lc: string): 'success' | 'info' | 'secondary' | 'destructive' {
  switch (lc) {
    case 'production': return 'success'
    case 'experimental': return 'info'
    case 'deprecated': return 'secondary'
    case 'retired': return 'destructive'
    default: return 'secondary'
  }
}

function systemName(id: string | null): string {
  if (!id) return ''
  const sys = systems.value.find(s => s.id === id)
  return sys?.display_name || sys?.name || ''
}

function healthTone(id: string): 'success' | 'warning' | 'destructive' | 'muted' {
  const inc = activeIncidentsByComponent.value.get(id) || 0
  if (inc > 0) return 'destructive'
  const burn = worstBurnByComponent.value.get(id)
  if (burn != null) {
    if (burn >= 3) return 'destructive'
    if (burn >= 1) return 'warning'
    return 'success'
  }
  return 'muted'
}

// ───────────────────────── Loaders ───────────────────────

async function load() {
  loading.value = true
  try {
    const all = await api.get<CatalogEntity[]>('/api/catalog/entities')
    entities.value = all.filter(e => e.kind === 'component')
    systems.value = all.filter(e => e.kind === 'system')

    // Load SLO summary; tolerate failure.
    try {
      slos.value = await api.get<Slo[]>('/api/slos?include_disabled=true')
    } catch {
      slos.value = []
    }

    // Fetch budgets in parallel — missing snapshots are expected while SLOs warm up.
    const pairs = await Promise.all(
      slos.value.map(async (s) => {
        try {
          const snap = await api.get<BudgetSnapshot>(`/api/slos/${s.id}/budget`)
          return [s.id, snap] as const
        } catch {
          return [s.id, null] as const
        }
      }),
    )
    const bm: Record<string, BudgetSnapshot> = {}
    for (const [id, snap] of pairs) {
      if (snap) bm[id] = snap
    }
    budgetMap.value = bm

    // Active incidents only; we just need counts per component.
    try {
      incidents.value = await api.get<Incident[]>('/api/incidents/active')
    } catch {
      incidents.value = []
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(load)

function openService(e: CatalogEntity) {
  router.push(`/services/${e.id}`)
}
</script>

<template>
  <div class="space-y-5">
    <!-- Header -->
    <div class="flex items-center justify-between gap-3 flex-wrap">
      <div>
        <h1 class="text-base font-semibold text-foreground flex items-center gap-2">
          <Layers class="h-4 w-4 text-primary" />
          {{ t('services.title') }}
        </h1>
        <p class="text-[11px] text-muted-foreground mt-0.5">
          {{ t('services.subtitle') }}
        </p>
      </div>

      <Button variant="outline" size="sm" :disabled="loading" @click="load">
        <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': loading }" />
        {{ t('common.refresh') }}
      </Button>
    </div>

    <!-- Filters -->
    <div class="flex flex-wrap items-center gap-2">
      <div class="relative flex-1 min-w-[200px] max-w-md">
        <Search class="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground/60" />
        <Input
          v-model="search"
          class="pl-7"
          :placeholder="t('services.searchPlaceholder')"
        />
      </div>
      <Select v-model="lifecycleFilter">
        <SelectTrigger class="w-40 h-8 text-xs">
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
      <Select v-model="systemFilter">
        <SelectTrigger class="w-44 h-8 text-xs">
          <SelectValue :placeholder="t('services.filterBySystem')" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="all">{{ t('services.allSystems') }}</SelectItem>
          <SelectItem v-for="s in systems" :key="s.id" :value="s.id">
            {{ s.display_name || s.name }}
          </SelectItem>
        </SelectContent>
      </Select>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      <div v-for="n in 6" :key="n" class="h-32 rounded-md bg-panel/50 animate-pulse" />
    </div>

    <!-- Empty -->
    <div
      v-else-if="filtered.length === 0"
      class="rounded-lg border border-dashed border-border/50 py-16 text-center space-y-2"
    >
      <Layers class="h-10 w-10 mx-auto text-muted-foreground/40" />
      <p class="text-xs text-muted-foreground">{{ t('services.emptyTitle') }}</p>
      <p class="text-[11px] text-muted-foreground/60">{{ t('services.emptyHint') }}</p>
    </div>

    <!-- Grid -->
    <div v-else class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      <article
        v-for="e in filtered"
        :key="e.id"
        class="group rounded-md bg-panel/60 hover:bg-panel/80 p-4 transition-colors cursor-pointer space-y-2.5"
        @click="openService(e)"
      >
        <!-- Header -->
        <div class="flex items-start justify-between gap-2">
          <div class="min-w-0">
            <h2 class="text-sm font-medium truncate">
              {{ e.display_name || e.name }}
            </h2>
            <p v-if="e.display_name" class="text-[10px] text-muted-foreground/70 font-mono truncate mt-0.5">
              {{ e.name }}
            </p>
          </div>
          <span
            class="h-2 w-2 rounded-full mt-1.5 shrink-0"
            :class="{
              'bg-success shadow-[0_0_6px_rgba(34,197,94,0.6)]': healthTone(e.id) === 'success',
              'bg-warning shadow-[0_0_6px_rgba(245,158,11,0.6)]': healthTone(e.id) === 'warning',
              'bg-destructive animate-pulse shadow-[0_0_6px_rgba(239,68,68,0.8)]': healthTone(e.id) === 'destructive',
              'bg-muted-foreground/40': healthTone(e.id) === 'muted',
            }"
            :title="t('services.healthDot')"
          />
        </div>

        <!-- Description -->
        <p v-if="e.description" class="text-[11px] text-muted-foreground line-clamp-2 leading-relaxed">
          {{ e.description }}
        </p>

        <!-- Metadata row -->
        <div class="flex flex-wrap items-center gap-1.5">
          <Badge :variant="lifecycleVariant(e.lifecycle)" class="text-[9px]">
            {{ e.lifecycle }}
          </Badge>
          <Badge v-if="e.system_id && systemName(e.system_id)" variant="secondary" class="text-[9px]">
            {{ systemName(e.system_id) }}
          </Badge>
          <Badge
            v-for="tag in e.tags.slice(0, 2)"
            :key="tag"
            variant="secondary"
            class="text-[9px] opacity-80"
          >
            {{ tag }}
          </Badge>
        </div>

        <!-- Stats bar -->
        <div class="flex items-center justify-between gap-2 pt-1.5 border-t border-border/30">
          <div class="flex items-center gap-1 text-[11px] text-muted-foreground">
            <Target class="h-3 w-3" />
            <span class="tabular-nums">{{ sloCountByComponent.get(e.id) || 0 }}</span>
            <span class="text-[10px]">{{ t('services.slosShort') }}</span>
          </div>
          <div
            class="flex items-center gap-1 text-[11px]"
            :class="(activeIncidentsByComponent.get(e.id) || 0) > 0 ? 'text-destructive' : 'text-muted-foreground/70'"
          >
            <AlertOctagon class="h-3 w-3" />
            <span class="tabular-nums">{{ activeIncidentsByComponent.get(e.id) || 0 }}</span>
            <span class="text-[10px]">{{ t('services.incidentsShort') }}</span>
          </div>
        </div>
      </article>
    </div>
  </div>
</template>
