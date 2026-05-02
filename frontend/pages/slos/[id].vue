<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, RefreshCw, Pencil, Trash2, Power, PowerOff, Zap } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import BudgetBar from '@/components/slos/BudgetBar.vue'
import BurnRateBadge from '@/components/slos/BurnRateBadge.vue'
import SparklineChart from '@/components/slos/SparklineChart.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const route = useRoute()
const router = useRouter()

const id = computed(() => route.params.id as string)

// ──────────────────── Types ────────────────────

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
  enabled: boolean
  recording_rules_hash: string | null
  created_by: string
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
  window_start: string
  window_end: string
}

interface PrometheusMatrix {
  prometheus: {
    result?: Array<{ values?: Array<[number, string]> }>
  }
  start: number
  end: number
}

interface SyncResult {
  slo_id: string
  synced: boolean
  message: string
  recording_rules_hash: string | null
}

// ──────────────────── State ────────────────────

const slo = ref<Slo | null>(null)
const snapshot = ref<BudgetSnapshot | null>(null)
const history = ref<BudgetSnapshot[]>([])
const sliPoints = ref<Array<[number, number]>>([])
const loading = ref(true)
const tab = ref<'sli' | 'budget' | 'burns'>('sli')

// ──────────────────── Loaders ────────────────────

async function fetchSlo() {
  slo.value = await api.get<Slo>(`/api/slos/${id.value}`)
}

async function fetchBudget() {
  try {
    snapshot.value = await api.get<BudgetSnapshot>(`/api/slos/${id.value}/budget`)
  } catch {
    snapshot.value = null
  }
}

async function fetchHistory() {
  try {
    history.value = await api.get<BudgetSnapshot[]>(`/api/slos/${id.value}/budget/history?days=90`)
  } catch {
    history.value = []
  }
}

async function fetchSli() {
  try {
    const r = await api.get<PrometheusMatrix>(`/api/slos/${id.value}/sli?window=28d&step=1h`)
    const rows = r.prometheus?.result?.[0]?.values ?? []
    sliPoints.value = rows
      .map(([ts, v]) => [Number(ts), Number(v) * 100] as [number, number])
      .filter((p) => Number.isFinite(p[1]))
  } catch {
    sliPoints.value = []
  }
}

async function refreshAll() {
  loading.value = true
  try {
    await Promise.all([fetchSlo(), fetchBudget(), fetchHistory(), fetchSli()])
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(refreshAll)

// ──────────────────── Derived ────────────────────

const budgetHistoryPoints = computed<Array<[number, number]>>(() =>
  history.value.map((h) => [new Date(h.captured_at).getTime() / 1000, h.budget_remaining_pct]),
)

const burnWindows = computed(() => {
  const s = snapshot.value
  if (!s) return []
  return [
    { label: '1h', value: s.burn_rate_1h },
    { label: '6h', value: s.burn_rate_6h },
    { label: '24h', value: s.burn_rate_24h },
    { label: '3d', value: s.burn_rate_3d },
  ]
})

// Derive "burn events" client-side from the history: a burn event starts
// whenever burn_rate_1h exceeds 1x after a normal period, and ends when it
// drops back below 1x. Good enough as a placeholder until we ship a
// dedicated endpoint.
interface BurnEvent {
  startedAt: string
  endedAt: string | null
  peakRate: number
  window: string
}
const burnEvents = computed<BurnEvent[]>(() => {
  const out: BurnEvent[] = []
  let active: BurnEvent | null = null
  for (const row of history.value) {
    const rate = row.burn_rate_1h ?? 0
    if (rate >= 1 && !active) {
      active = { startedAt: row.captured_at, endedAt: null, peakRate: rate, window: '1h' }
    } else if (rate >= 1 && active) {
      active.peakRate = Math.max(active.peakRate, rate)
    } else if (rate < 1 && active) {
      active.endedAt = row.captured_at
      out.push(active)
      active = null
    }
  }
  if (active) out.push(active)
  return out.reverse().slice(0, 12)
})

// ──────────────────── Actions ────────────────────

async function toggleEnabled() {
  if (!slo.value) return
  try {
    const endpoint = slo.value.enabled ? 'disable' : 'enable'
    slo.value = await api.post<Slo>(`/api/slos/${id.value}/${endpoint}`)
    toast.success(t('common.success'))
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function syncRules() {
  try {
    const r = await api.post<SyncResult>(`/api/slos/${id.value}/sync-rules`)
    if (r.synced) toast.success(r.message)
    else toast.warning(r.message)
    await fetchSlo()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function removeSlo() {
  if (!confirm(t('slo.deleteConfirm'))) return
  try {
    await api.del(`/api/slos/${id.value}`)
    toast.success(t('common.success'))
    router.push('/slos')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}
</script>

<template>
  <div class="space-y-5">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <Button variant="ghost" size="icon-sm" @click="router.push('/slos')">
          <ArrowLeft class="h-3.5 w-3.5" />
        </Button>
        <h1 class="text-base font-semibold text-foreground">
          {{ slo?.name || t('common.loading') }}
        </h1>
        <Badge v-if="slo" :variant="slo.enabled ? 'success' : 'secondary'" class="ml-1">
          {{ slo.enabled ? t('common.enabled') : t('common.disabled') }}
        </Badge>
      </div>
      <div class="flex items-center gap-2">
        <Button variant="outline" size="sm" :disabled="loading" @click="refreshAll">
          <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': loading }" />
          {{ t('common.refresh') }}
        </Button>
        <Button v-if="slo" variant="outline" size="sm" @click="syncRules">
          <Zap class="h-3.5 w-3.5" />
          {{ t('slo.syncRules') }}
        </Button>
        <Button v-if="slo" variant="outline" size="sm" @click="toggleEnabled">
          <component :is="slo.enabled ? PowerOff : Power" class="h-3.5 w-3.5" />
          {{ slo.enabled ? t('slo.disable') : t('slo.enable') }}
        </Button>
        <Button variant="outline" size="sm" @click="router.push(`/slos?edit=${id}`)">
          <Pencil class="h-3.5 w-3.5" />
          {{ t('common.edit') }}
        </Button>
        <Button variant="outline" size="sm" class="text-destructive hover:text-destructive" @click="removeSlo">
          <Trash2 class="h-3.5 w-3.5" />
          {{ t('common.delete') }}
        </Button>
      </div>
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-[320px_1fr] gap-5">
      <!-- LEFT: metadata + current budget + burn rates -->
      <div class="space-y-4">
        <!-- Metadata -->
        <section class="rounded-md bg-panel/60 p-4 space-y-3">
          <h2 class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
            {{ t('slo.sections.definition') }}
          </h2>
          <div v-if="slo" class="space-y-2 text-xs">
            <div class="flex justify-between">
              <span class="text-muted-foreground">{{ t('slo.field.objectivePct') }}</span>
              <span class="font-mono">{{ slo.objective_pct.toFixed(3) }}%</span>
            </div>
            <div class="flex justify-between">
              <span class="text-muted-foreground">{{ t('slo.field.windowDays') }}</span>
              <span class="font-mono">{{ slo.window_days }}d</span>
            </div>
            <div class="flex justify-between">
              <span class="text-muted-foreground">{{ t('slo.field.sliType') }}</span>
              <span class="uppercase">{{ slo.sli_type }}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-muted-foreground">{{ t('slo.field.component') }}</span>
              <NuxtLink v-if="slo.component_id" :to="`/catalog/${slo.component_id}`" class="text-primary hover:underline truncate">
                {{ slo.component_id.slice(0, 8) }}
              </NuxtLink>
              <span v-else>—</span>
            </div>
            <div class="flex justify-between">
              <span class="text-muted-foreground">{{ t('slo.field.policy') }}</span>
              <span class="font-mono text-[11px]">{{ slo.burn_rate_policy }}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-muted-foreground">{{ t('slo.rulesHash') }}</span>
              <span v-if="slo.recording_rules_hash" class="font-mono text-[10px] truncate max-w-[120px]">
                {{ slo.recording_rules_hash.slice(0, 12) }}
              </span>
              <span v-else class="text-muted-foreground/60">—</span>
            </div>
            <div v-if="slo.description" class="pt-2 border-t border-border/40">
              <p class="text-[11px] text-muted-foreground/80 leading-relaxed">{{ slo.description }}</p>
            </div>
          </div>
        </section>

        <!-- Current budget -->
        <section class="rounded-md bg-panel/60 p-4 space-y-3">
          <h2 class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
            {{ t('slo.sections.currentBudget') }}
          </h2>
          <div v-if="snapshot" class="space-y-2">
            <BudgetBar
              :remaining-pct="snapshot.budget_remaining_pct"
              :consumed-minutes="snapshot.budget_consumed_minutes"
              :total-minutes="snapshot.budget_total_minutes"
            />
            <div class="flex items-center justify-between text-[11px]">
              <span class="text-muted-foreground">{{ t('slo.currentSli') }}</span>
              <span class="font-mono">{{ snapshot.sli_achieved_pct.toFixed(3) }}%</span>
            </div>
            <div class="text-[10px] text-muted-foreground/60">
              {{ t('slo.snapshotAt', { time: formatDate(snapshot.captured_at) }) }}
            </div>
          </div>
          <p v-else class="text-[11px] text-muted-foreground/60">{{ t('slo.noSnapshot') }}</p>
        </section>

        <!-- Burn rates -->
        <section class="rounded-md bg-panel/60 p-4 space-y-3">
          <h2 class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
            {{ t('slo.sections.burnRates') }}
          </h2>
          <div v-if="snapshot" class="space-y-1.5">
            <div
              v-for="w in burnWindows"
              :key="w.label"
              class="flex items-center justify-between text-[11px]"
            >
              <span class="text-muted-foreground font-mono">{{ w.label }}</span>
              <BurnRateBadge :rate="w.value" />
            </div>
          </div>
          <p v-else class="text-[11px] text-muted-foreground/60">{{ t('slo.noSnapshot') }}</p>
        </section>
      </div>

      <!-- RIGHT: tabs with charts -->
      <div class="space-y-3">
        <!-- Tab bar -->
        <div class="flex items-center gap-1 border-b border-border/40">
          <button
            v-for="t2 in (['sli','budget','burns'] as const)"
            :key="t2"
            class="px-3 h-8 text-[11px] font-medium uppercase tracking-wider transition-colors border-b-2"
            :class="tab === t2 ? 'border-primary text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground'"
            @click="tab = t2"
          >
            {{ t(`slo.tab.${t2}`) }}
          </button>
        </div>

        <!-- SLI time series -->
        <section v-if="tab === 'sli'" class="rounded-md bg-panel/60 p-4 space-y-3">
          <div class="flex items-baseline justify-between">
            <h3 class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              {{ t('slo.charts.sliTitle') }}
            </h3>
            <span v-if="slo" class="text-[10px] text-muted-foreground/60">28d · step=1h</span>
          </div>
          <SparklineChart
            :points="sliPoints"
            :height="200"
            :width="800"
            :y-min="slo ? Math.max(0, slo.objective_pct - 2) : undefined"
            :y-max="100"
            :reference-y="slo?.objective_pct"
            :reference-label="slo ? `target ${slo.objective_pct.toFixed(2)}%` : null"
            filled
          />
        </section>

        <!-- Budget history -->
        <section v-if="tab === 'budget'" class="rounded-md bg-panel/60 p-4 space-y-3">
          <div class="flex items-baseline justify-between">
            <h3 class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              {{ t('slo.charts.budgetTitle') }}
            </h3>
            <span class="text-[10px] text-muted-foreground/60">90d</span>
          </div>
          <SparklineChart
            :points="budgetHistoryPoints"
            :height="200"
            :width="800"
            :y-min="-20"
            :y-max="100"
            :reference-y="0"
            reference-label="breach"
            filled
          />
        </section>

        <!-- Burn events -->
        <section v-if="tab === 'burns'" class="rounded-md bg-panel/60 p-4 space-y-3">
          <h3 class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
            {{ t('slo.charts.burnsTitle') }}
          </h3>
          <div v-if="burnEvents.length" class="space-y-1.5">
            <div
              v-for="(ev, idx) in burnEvents"
              :key="idx"
              class="flex items-center justify-between rounded-sm bg-background/40 px-3 py-2 text-xs"
            >
              <div class="flex items-center gap-2">
                <span class="font-mono text-[10px] text-muted-foreground">{{ formatDate(ev.startedAt) }}</span>
                <span class="text-muted-foreground/40">→</span>
                <span class="font-mono text-[10px] text-muted-foreground">
                  {{ ev.endedAt ? formatDate(ev.endedAt) : t('slo.burnsOngoing') }}
                </span>
              </div>
              <BurnRateBadge :rate="ev.peakRate" :label="t('slo.burnsPeak')" />
            </div>
          </div>
          <p v-else class="text-[11px] text-muted-foreground/60">{{ t('slo.noBurnEvents') }}</p>
        </section>
      </div>
    </div>
  </div>
</template>
