<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { Plus, Search, RefreshCw } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from '@/components/ui/dialog'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { Switch } from '@/components/ui/switch'
import BudgetBar from '@/components/slos/BudgetBar.vue'
import BurnRateBadge from '@/components/slos/BurnRateBadge.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const router = useRouter()

// ───────────────────────── Types ──────────────────────────

interface Slo {
  id: string
  tenant_id: string
  component_id: string | null
  name: string
  description: string | null
  sli_type: string
  good_events_query: string
  total_events_query: string
  objective_pct: number
  window_days: number
  burn_rate_policy: string
  labels: Record<string, unknown>
  enabled: boolean
  recording_rules_hash: string | null
  created_at: string
  updated_at: string
}

interface BudgetSnapshot {
  id: string
  slo_id: string
  captured_at: string
  sli_achieved_pct: number
  budget_total_minutes: number
  budget_consumed_minutes: number
  budget_remaining_pct: number
  burn_rate_1h: number | null
  burn_rate_6h: number | null
  burn_rate_24h: number | null
  burn_rate_3d: number | null
}

interface CatalogEntity {
  id: string
  name: string
  display_name: string | null
  kind: string
}

interface PreviewResult {
  query: string
  window_days: number
  step: string
  start: number
  end: number
  prometheus: {
    result?: Array<{ values?: Array<[number, string]> }>
  }
}

// ───────────────────────── State ──────────────────────────

const slos = ref<Slo[]>([])
const budgets = ref<Record<string, BudgetSnapshot | null>>({})
const components = ref<CatalogEntity[]>([])
const loading = ref(true)
const saving = ref(false)

const search = ref('')
const componentFilter = ref<string>('all')
const includeDisabled = ref(false)

const showFormDialog = ref(false)
const editing = ref<Slo | null>(null)

type FormState = {
  name: string
  description: string
  component_id: string
  sli_type: string
  good_events_query: string
  total_events_query: string
  objective_pct: number
  window_days: number
  enabled: boolean
}

const form = ref<FormState>(emptyForm())
const previewLoading = ref(false)
const previewSummary = ref<string | null>(null)

function emptyForm(): FormState {
  return {
    name: '',
    description: '',
    component_id: '',
    sli_type: 'availability',
    good_events_query: 'sum(rate(http_requests_total{status!~"5.."}[5m]))',
    total_events_query: 'sum(rate(http_requests_total[5m]))',
    objective_pct: 99.9,
    window_days: 28,
    enabled: true,
  }
}

// ───────────────────────── Derived ────────────────────────

const filtered = computed(() => {
  const q = search.value.trim().toLowerCase()
  return slos.value.filter((s) => {
    if (componentFilter.value !== 'all' && s.component_id !== componentFilter.value) return false
    if (!q) return true
    return (
      s.name.toLowerCase().includes(q) ||
      (s.description || '').toLowerCase().includes(q) ||
      s.sli_type.includes(q)
    )
  })
})

function componentLabel(id: string | null): string {
  if (!id) return '-'
  const c = components.value.find((x) => x.id === id)
  if (!c) return id.slice(0, 8)
  return c.display_name || c.name
}

function snapshotFor(id: string): BudgetSnapshot | null {
  return budgets.value[id] ?? null
}

// ───────────────────────── Loaders ────────────────────────

async function fetchSlos() {
  loading.value = true
  try {
    const list = await api.get<Slo[]>(
      `/api/slos?include_disabled=${includeDisabled.value ? 'true' : 'false'}`,
    )
    slos.value = list
    // Fetch budget snapshots in parallel — a 404 means "no snapshot yet" so
    // we swallow it and leave the row's budget columns empty.
    const pairs = await Promise.all(
      list.map(async (s) => {
        try {
          const snap = await api.get<BudgetSnapshot>(`/api/slos/${s.id}/budget`)
          return [s.id, snap] as const
        } catch {
          return [s.id, null] as const
        }
      }),
    )
    budgets.value = Object.fromEntries(pairs)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

async function fetchComponents() {
  try {
    const rows = await api.get<CatalogEntity[]>('/api/catalog/entities')
    components.value = rows.filter((r) => r.kind === 'component' || r.kind === 'system')
  } catch {
    // Catalog backend may not be populated yet — silently fall back to an
    // empty list so the SLO page still renders.
    components.value = []
  }
}

onMounted(async () => {
  await Promise.all([fetchSlos(), fetchComponents()])
})

// ───────────────────────── Create / edit ──────────────────

function openCreate() {
  editing.value = null
  form.value = emptyForm()
  previewSummary.value = null
  showFormDialog.value = true
}

// (edit is currently invoked from the detail page's header; re-enter here
// when we wire inline row actions.)

async function save() {
  saving.value = true
  try {
    const payload = {
      name: form.value.name,
      description: form.value.description || null,
      component_id: form.value.component_id || null,
      sli_type: form.value.sli_type,
      good_events_query: form.value.good_events_query,
      total_events_query: form.value.total_events_query,
      objective_pct: Number(form.value.objective_pct),
      window_days: Number(form.value.window_days),
      enabled: form.value.enabled,
    }
    if (editing.value) {
      await api.put(`/api/slos/${editing.value.id}`, payload)
      toast.success(t('slo.updated'))
    } else {
      await api.post('/api/slos', payload)
      toast.success(t('slo.created'))
    }
    showFormDialog.value = false
    await fetchSlos()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function testPreview() {
  if (!form.value.good_events_query || !form.value.total_events_query) {
    toast.warning(t('slo.previewQueryRequired'))
    return
  }
  previewLoading.value = true
  previewSummary.value = null
  try {
    const r = await api.post<PreviewResult>('/api/slos/preview', {
      good_events_query: form.value.good_events_query,
      total_events_query: form.value.total_events_query,
      window_days: Number(form.value.window_days),
      step: '1h',
    })
    // Shape: Prometheus matrix response. Average the first series as the
    // quick-feedback value the user cares about (full time series lives on
    // the detail page).
    const values = r.prometheus?.result?.[0]?.values ?? []
    if (!values.length) {
      previewSummary.value = t('slo.previewNoData')
      return
    }
    const nums = values
      .map(([, v]) => Number(v))
      .filter((v) => Number.isFinite(v))
    const avg = nums.reduce((a, b) => a + b, 0) / Math.max(1, nums.length)
    previewSummary.value = t('slo.previewSummary', {
      points: values.length,
      avg: (avg * 100).toFixed(3),
    })
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
    previewSummary.value = null
  } finally {
    previewLoading.value = false
  }
}

function goToDetail(s: Slo) {
  router.push(`/slos/${s.id}`)
}

function refresh() {
  fetchSlos()
}
</script>

<template>
  <div class="space-y-5">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-base font-semibold text-foreground">{{ t('slo.title') }}</h1>
        <p class="text-[11px] text-muted-foreground/70 mt-0.5">{{ t('slo.subtitle') }}</p>
      </div>
      <div class="flex items-center gap-2">
        <Button variant="outline" size="sm" :disabled="loading" @click="refresh">
          <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': loading }" />
          {{ t('common.refresh') }}
        </Button>
        <Button size="sm" @click="openCreate">
          <Plus class="h-3.5 w-3.5" />
          {{ t('slo.newSlo') }}
        </Button>
      </div>
    </div>

    <!-- Filters -->
    <div class="flex flex-wrap items-center gap-2">
      <div class="relative flex-1 min-w-[200px] max-w-sm">
        <Search class="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground/60" />
        <Input v-model="search" class="pl-7" :placeholder="t('slo.searchPlaceholder')" />
      </div>
      <Select v-model="componentFilter">
        <SelectTrigger class="w-48">
          <SelectValue :placeholder="t('slo.filterByComponent')" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="all">{{ t('slo.allComponents') }}</SelectItem>
          <SelectItem v-for="c in components" :key="c.id" :value="c.id">
            {{ c.display_name || c.name }}
          </SelectItem>
        </SelectContent>
      </Select>
      <div class="flex items-center gap-2 rounded-md border border-border/60 bg-panel/60 px-2 py-1.5">
        <Switch v-model="includeDisabled" @update:model-value="fetchSlos" />
        <span class="text-[11px] text-muted-foreground">{{ t('slo.includeDisabled') }}</span>
      </div>
    </div>

    <!-- Table -->
    <div class="rounded border border-border/60 bg-card overflow-hidden">
      <div class="overflow-x-auto">
        <table class="w-full text-xs">
          <thead>
            <tr class="border-b border-border/60 bg-secondary/30">
              <th class="h-9 px-3 text-left font-medium text-muted-foreground uppercase tracking-wider text-[11px]">
                {{ t('slo.col.name') }}
              </th>
              <th class="h-9 px-3 text-left font-medium text-muted-foreground uppercase tracking-wider text-[11px]">
                {{ t('slo.col.component') }}
              </th>
              <th class="h-9 px-3 text-left font-medium text-muted-foreground uppercase tracking-wider text-[11px]">
                {{ t('slo.col.target') }}
              </th>
              <th class="h-9 px-3 text-left font-medium text-muted-foreground uppercase tracking-wider text-[11px]">
                {{ t('slo.col.sli') }}
              </th>
              <th class="h-9 px-3 text-left font-medium text-muted-foreground uppercase tracking-wider text-[11px] min-w-[160px]">
                {{ t('slo.col.budget') }}
              </th>
              <th class="h-9 px-3 text-left font-medium text-muted-foreground uppercase tracking-wider text-[11px]">
                {{ t('slo.col.burn1h') }}
              </th>
              <th class="h-9 px-3 text-left font-medium text-muted-foreground uppercase tracking-wider text-[11px]">
                {{ t('slo.col.burn6h') }}
              </th>
              <th class="h-9 px-3 text-left font-medium text-muted-foreground uppercase tracking-wider text-[11px]">
                {{ t('slo.col.enabled') }}
              </th>
            </tr>
          </thead>
          <tbody>
            <template v-if="loading">
              <tr v-for="i in 5" :key="`sk-${i}`" class="border-b border-border/40">
                <td colspan="8" class="px-3 py-4 text-center text-muted-foreground/40 text-[11px]">
                  {{ t('common.loading') }}
                </td>
              </tr>
            </template>
            <tr
              v-for="slo in filtered"
              v-else
              :key="slo.id"
              class="border-b border-border/40 hover:bg-accent/50 cursor-pointer transition-colors"
              @click="goToDetail(slo)"
            >
              <td class="px-3 py-2">
                <div class="font-medium text-foreground">{{ slo.name }}</div>
                <div class="text-[10px] text-muted-foreground/60 uppercase tracking-wider">
                  {{ slo.sli_type }}
                </div>
              </td>
              <td class="px-3 py-2">
                <NuxtLink
                  v-if="slo.component_id"
                  :to="`/catalog/${slo.component_id}`"
                  class="text-primary hover:underline"
                  @click.stop
                >
                  {{ componentLabel(slo.component_id) }}
                </NuxtLink>
                <span v-else class="text-muted-foreground">—</span>
              </td>
              <td class="px-3 py-2 font-mono">{{ slo.objective_pct.toFixed(3) }}%</td>
              <td class="px-3 py-2 font-mono">
                <span v-if="snapshotFor(slo.id)">
                  {{ snapshotFor(slo.id)!.sli_achieved_pct.toFixed(3) }}%
                </span>
                <span v-else class="text-muted-foreground/40">—</span>
              </td>
              <td class="px-3 py-2">
                <BudgetBar
                  v-if="snapshotFor(slo.id)"
                  :remaining-pct="snapshotFor(slo.id)!.budget_remaining_pct"
                  :consumed-minutes="snapshotFor(slo.id)!.budget_consumed_minutes"
                  :total-minutes="snapshotFor(slo.id)!.budget_total_minutes"
                />
                <span v-else class="text-muted-foreground/40 text-[11px]">{{ t('slo.noSnapshot') }}</span>
              </td>
              <td class="px-3 py-2">
                <BurnRateBadge :rate="snapshotFor(slo.id)?.burn_rate_1h ?? null" />
              </td>
              <td class="px-3 py-2">
                <BurnRateBadge :rate="snapshotFor(slo.id)?.burn_rate_6h ?? null" />
              </td>
              <td class="px-3 py-2">
                <Badge :variant="slo.enabled ? 'success' : 'secondary'">
                  {{ slo.enabled ? t('common.enabled') : t('common.disabled') }}
                </Badge>
              </td>
            </tr>
            <tr v-if="!loading && filtered.length === 0">
              <td colspan="8" class="h-24 text-center">
                <div class="flex flex-col items-center justify-center gap-1.5 text-muted-foreground">
                  <span class="text-xs">{{ t('slo.emptyState') }}</span>
                  <Button size="sm" variant="outline" @click="openCreate">
                    <Plus class="h-3.5 w-3.5" />
                    {{ t('slo.newSlo') }}
                  </Button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- New / Edit SLO dialog -->
    <Dialog :open="showFormDialog" @update:open="(v) => { showFormDialog = v }">
      <DialogContent class="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{{ editing ? t('slo.editSlo') : t('slo.newSlo') }}</DialogTitle>
          <DialogDescription>{{ t('slo.formHelp') }}</DialogDescription>
        </DialogHeader>

        <form class="space-y-3 max-h-[70vh] overflow-y-auto pr-1" @submit.prevent="save">
          <div class="grid grid-cols-2 gap-3">
            <div class="space-y-1.5">
              <label class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                {{ t('slo.field.name') }}
              </label>
              <Input v-model="form.name" required placeholder="checkout-availability" />
            </div>
            <div class="space-y-1.5">
              <label class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                {{ t('slo.field.component') }}
              </label>
              <Select v-model="form.component_id">
                <SelectTrigger><SelectValue :placeholder="t('slo.field.componentPlaceholder')" /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="">—</SelectItem>
                  <SelectItem v-for="c in components" :key="c.id" :value="c.id">
                    {{ c.display_name || c.name }}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <div class="space-y-1.5">
            <label class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
              {{ t('slo.field.description') }}
            </label>
            <Input v-model="form.description" :placeholder="t('slo.field.descriptionPlaceholder')" />
          </div>

          <div class="grid grid-cols-3 gap-3">
            <div class="space-y-1.5">
              <label class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                {{ t('slo.field.sliType') }}
              </label>
              <Select v-model="form.sli_type">
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="availability">{{ t('slo.sliType.availability') }}</SelectItem>
                  <SelectItem value="latency">{{ t('slo.sliType.latency') }}</SelectItem>
                  <SelectItem value="error_rate">{{ t('slo.sliType.errorRate') }}</SelectItem>
                  <SelectItem value="custom">{{ t('slo.sliType.custom') }}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div class="space-y-1.5">
              <label class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                {{ t('slo.field.objectivePct') }}
              </label>
              <Input
                v-model.number="form.objective_pct"
                type="number"
                step="0.001"
                min="0.001"
                max="99.999"
                required
              />
            </div>
            <div class="space-y-1.5">
              <label class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                {{ t('slo.field.windowDays') }}
              </label>
              <Select v-model.number="form.window_days">
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem :value="7">7d</SelectItem>
                  <SelectItem :value="28">28d</SelectItem>
                  <SelectItem :value="30">30d</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <div class="space-y-1.5">
            <label class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
              {{ t('slo.field.goodQuery') }}
            </label>
            <Textarea v-model="form.good_events_query" required class="font-mono text-[11px]" rows="2" />
          </div>
          <div class="space-y-1.5">
            <label class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
              {{ t('slo.field.totalQuery') }}
            </label>
            <Textarea v-model="form.total_events_query" required class="font-mono text-[11px]" rows="2" />
          </div>

          <!-- Preview -->
          <div class="rounded-md border border-border/50 bg-panel/40 p-3 space-y-2">
            <div class="flex items-center justify-between">
              <span class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                {{ t('slo.preview') }}
              </span>
              <Button type="button" size="sm" variant="outline" :disabled="previewLoading" @click="testPreview">
                {{ previewLoading ? t('common.loading') : t('slo.testPreview') }}
              </Button>
            </div>
            <p v-if="previewSummary" class="text-[11px] font-mono text-foreground">{{ previewSummary }}</p>
            <p v-else class="text-[11px] text-muted-foreground/60">{{ t('slo.previewHint') }}</p>
          </div>

          <div class="flex items-center gap-2">
            <Switch v-model="form.enabled" />
            <span class="text-[11px] text-muted-foreground">{{ t('slo.field.enabled') }}</span>
          </div>

          <DialogFooter class="gap-1.5 pt-1">
            <Button type="button" variant="outline" size="sm" @click="showFormDialog = false">
              {{ t('common.cancel') }}
            </Button>
            <Button type="submit" size="sm" :disabled="saving">
              {{ saving ? t('common.loading') : editing ? t('common.save') : t('common.create') }}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  </div>
</template>
