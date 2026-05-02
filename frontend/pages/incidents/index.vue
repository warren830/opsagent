<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { AlertOctagon, Flag, Hash, Plus, Clock3, Megaphone, ChevronRight } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter,
} from '@/components/ui/dialog'
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const router = useRouter()

interface Incident {
  id: string
  number: number
  title: string
  severity: 'sev1' | 'sev2' | 'sev3' | 'sev4'
  status: string
  commander_user_id: string | null
  impact_summary: string | null
  affected_component_ids: string[]
  affected_customer_tier: string | null
  detection_source: string
  started_at: string
  detected_at: string
  resolved_at: string | null
  closed_at: string | null
  war_room_channel_ref: { slack_channel_id?: string; url?: string } | null
  bridge_url: string | null
  jira_key: string | null
  labels: Record<string, unknown>
}

const active = ref<Incident[]>([])
const recent = ref<Incident[]>([])
const loading = ref(true)
const severityFilter = ref<string>('all')
const statusFilter = ref<string>('all')

// Declare dialog state.
const showDeclare = ref(false)
const declareForm = ref({
  title: '',
  severity: 'sev2',
  detection_source: 'manual',
  impact_summary: '',
})
const declareBusy = ref(false)

const SEVERITY_ORDER = ['sev1', 'sev2', 'sev3', 'sev4']

const filteredActive = computed(() => {
  let rows = active.value
  if (severityFilter.value !== 'all') rows = rows.filter((r) => r.severity === severityFilter.value)
  if (statusFilter.value !== 'all') rows = rows.filter((r) => r.status === statusFilter.value)
  return [...rows].sort((a, b) => {
    const sa = SEVERITY_ORDER.indexOf(a.severity)
    const sb = SEVERITY_ORDER.indexOf(b.severity)
    if (sa !== sb) return sa - sb
    return new Date(b.detected_at).getTime() - new Date(a.detected_at).getTime()
  })
})

const SEVERITY_STYLE: Record<string, { variant: 'destructive' | 'warning' | 'info' | 'secondary'; label: string }> = {
  sev1: { variant: 'destructive', label: 'SEV1' },
  sev2: { variant: 'warning', label: 'SEV2' },
  sev3: { variant: 'info', label: 'SEV3' },
  sev4: { variant: 'secondary', label: 'SEV4' },
}

function statusStyle(status: string): { variant: 'warning' | 'info' | 'success' | 'secondary'; label: string } {
  const map: Record<string, { variant: 'warning' | 'info' | 'success' | 'secondary'; label: string }> = {
    triggered: { variant: 'warning', label: t('incident.status.triggered') },
    acknowledged: { variant: 'info', label: t('incident.status.acknowledged') },
    investigating: { variant: 'info', label: t('incident.status.investigating') },
    identified: { variant: 'info', label: t('incident.status.identified') },
    mitigated: { variant: 'info', label: t('incident.status.mitigated') },
    resolved: { variant: 'success', label: t('incident.status.resolved') },
    postmortem_draft: { variant: 'secondary', label: t('incident.status.postmortem_draft') },
    postmortem_published: { variant: 'secondary', label: t('incident.status.postmortem_published') },
    closed: { variant: 'secondary', label: t('incident.status.closed') },
  }
  return map[status] || { variant: 'secondary', label: status }
}

function formatElapsed(startIso: string, endIso: string | null): string {
  const start = new Date(startIso).getTime()
  const end = endIso ? new Date(endIso).getTime() : Date.now()
  const s = Math.max(0, Math.floor((end - start) / 1000))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  if (h > 0) return `${h}h${m}m`
  if (m > 0) return `${m}m${s % 60}s`
  return `${s}s`
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  })
}

async function fetchAll() {
  loading.value = true
  try {
    const [activeRows, resolvedRows] = await Promise.all([
      api.get<Incident[]>('/api/incidents/active'),
      api.get<Incident[]>('/api/incidents?status=resolved'),
    ])
    active.value = activeRows
    // Keep only the past 7 days of resolved incidents.
    const sevenDaysAgo = Date.now() - 7 * 24 * 3600 * 1000
    recent.value = resolvedRows
      .filter((r) => r.resolved_at && new Date(r.resolved_at).getTime() >= sevenDaysAgo)
      .sort((a, b) => new Date(b.resolved_at || b.detected_at).getTime() - new Date(a.resolved_at || a.detected_at).getTime())
      .slice(0, 20)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

async function submitDeclare() {
  if (!declareForm.value.title.trim()) {
    toast.error(t('incident.errors.titleRequired'))
    return
  }
  declareBusy.value = true
  try {
    const created = await api.post<Incident>('/api/incidents', {
      title: declareForm.value.title.trim(),
      severity: declareForm.value.severity,
      detection_source: declareForm.value.detection_source,
      impact_summary: declareForm.value.impact_summary.trim() || undefined,
      started_at: new Date().toISOString(),
    })
    toast.success(t('incident.declared', { num: created.number }))
    showDeclare.value = false
    declareForm.value = { title: '', severity: 'sev2', detection_source: 'manual', impact_summary: '' }
    router.push(`/incidents/${created.id}`)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    declareBusy.value = false
  }
}

function openWarRoom(inc: Incident) {
  router.push(`/incidents/${inc.id}`)
}

function slackHref(inc: Incident): string | null {
  const ref = inc.war_room_channel_ref
  if (!ref) return null
  if (typeof ref.url === 'string' && ref.url) return ref.url
  if (typeof ref.slack_channel_id === 'string' && ref.slack_channel_id) {
    return `slack://channel?id=${ref.slack_channel_id}`
  }
  return null
}

// Ticking clock for duration badges on active cards.
const now = ref(Date.now())
let timer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  fetchAll()
  timer = setInterval(() => { now.value = Date.now() }, 30_000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <div class="space-y-6">
    <!-- Header -->
    <div class="flex items-center justify-between gap-3 flex-wrap">
      <div>
        <h1 class="text-base font-semibold text-foreground flex items-center gap-2">
          <AlertOctagon class="h-4 w-4 text-destructive" />
          {{ t('incident.title') }}
        </h1>
        <p class="text-[11px] text-muted-foreground mt-0.5">
          {{ t('incident.subtitle') }}
        </p>
      </div>

      <div class="flex items-center gap-2">
        <Select v-model="severityFilter">
          <SelectTrigger class="h-8 text-xs w-32">
            <SelectValue :placeholder="t('incident.severityAll')" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{{ t('incident.severityAll') }}</SelectItem>
            <SelectItem value="sev1">SEV1</SelectItem>
            <SelectItem value="sev2">SEV2</SelectItem>
            <SelectItem value="sev3">SEV3</SelectItem>
            <SelectItem value="sev4">SEV4</SelectItem>
          </SelectContent>
        </Select>
        <Select v-model="statusFilter">
          <SelectTrigger class="h-8 text-xs w-40">
            <SelectValue :placeholder="t('incident.statusAll')" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{{ t('incident.statusAll') }}</SelectItem>
            <SelectItem value="triggered">{{ t('incident.status.triggered') }}</SelectItem>
            <SelectItem value="acknowledged">{{ t('incident.status.acknowledged') }}</SelectItem>
            <SelectItem value="investigating">{{ t('incident.status.investigating') }}</SelectItem>
            <SelectItem value="identified">{{ t('incident.status.identified') }}</SelectItem>
            <SelectItem value="mitigated">{{ t('incident.status.mitigated') }}</SelectItem>
          </SelectContent>
        </Select>

        <Button size="sm" @click="showDeclare = true">
          <Plus class="h-3 w-3" />
          {{ t('incident.declare') }}
        </Button>
      </div>
    </div>

    <!-- ACTIVE -->
    <section class="space-y-2">
      <div class="flex items-center gap-2">
        <h2 class="text-[11px] uppercase tracking-[0.2em] text-muted-foreground font-semibold">
          {{ t('incident.active') }}
        </h2>
        <span class="text-[11px] text-muted-foreground/70 tabular-nums">
          {{ filteredActive.length }}
        </span>
      </div>

      <div v-if="loading" class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        <div v-for="n in 3" :key="n" class="h-36 rounded-md bg-panel/60 animate-pulse" />
      </div>

      <div v-else-if="filteredActive.length === 0"
        class="rounded-md bg-panel/50 p-6 text-xs text-muted-foreground text-center">
        {{ t('incident.noActive') }}
      </div>

      <div v-else class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        <article
          v-for="inc in filteredActive"
          :key="inc.id"
          class="group rounded-md bg-panel/60 hover:bg-panel/80 p-3.5 transition-colors flex flex-col gap-2 cursor-pointer"
          @click="openWarRoom(inc)"
        >
          <!-- Header row: severity + number + duration -->
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-1.5">
              <Badge :variant="SEVERITY_STYLE[inc.severity].variant" class="text-[10px] font-semibold">
                {{ SEVERITY_STYLE[inc.severity].label }}
              </Badge>
              <span class="text-[11px] text-muted-foreground tabular-nums">
                INC-{{ String(inc.number).padStart(4, '0') }}
              </span>
            </div>
            <div class="flex items-center gap-1 text-[11px] text-warning tabular-nums">
              <Clock3 class="h-3 w-3" />
              <span>{{ formatElapsed(inc.started_at, null) }}</span>
              <!-- Reactive re-render tick -->
              <span class="hidden">{{ now }}</span>
            </div>
          </div>

          <!-- Title -->
          <h3 class="text-sm font-medium text-foreground line-clamp-2 leading-snug">
            {{ inc.title }}
          </h3>

          <!-- Impact summary -->
          <p v-if="inc.impact_summary" class="text-[11px] text-muted-foreground line-clamp-2">
            {{ inc.impact_summary }}
          </p>

          <!-- Meta row -->
          <div class="flex items-center gap-1.5 flex-wrap pt-1">
            <Badge :variant="statusStyle(inc.status).variant" class="text-[10px]">
              {{ statusStyle(inc.status).label }}
            </Badge>
            <Badge v-if="inc.affected_customer_tier" variant="outline" class="text-[10px]">
              {{ inc.affected_customer_tier }}
            </Badge>
            <Badge v-if="inc.jira_key" variant="outline" class="text-[10px]">
              <Hash class="h-2.5 w-2.5 mr-0.5" />
              {{ inc.jira_key }}
            </Badge>
            <Badge v-if="inc.affected_component_ids.length > 0" variant="secondary" class="text-[10px]">
              {{ t('incident.componentsCount', { n: inc.affected_component_ids.length }) }}
            </Badge>
          </div>

          <!-- Footer: war-room link + open -->
          <div class="flex items-center justify-between pt-2 border-t border-border/40 mt-auto">
            <a
              v-if="slackHref(inc)"
              :href="slackHref(inc) || '#'"
              class="text-[11px] text-primary hover:underline flex items-center gap-1"
              @click.stop
            >
              <Megaphone class="h-3 w-3" />
              {{ t('incident.openWarRoom') }}
            </a>
            <span v-else class="text-[11px] text-muted-foreground/60">
              {{ t('incident.warRoomPending') }}
            </span>

            <Button
              size="sm"
              variant="ghost"
              class="h-7 px-2 text-[11px]"
              @click.stop="openWarRoom(inc)"
            >
              {{ t('common.open') }}
              <ChevronRight class="h-3 w-3" />
            </Button>
          </div>
        </article>
      </div>
    </section>

    <!-- RECENTLY RESOLVED -->
    <section class="space-y-2">
      <div class="flex items-center gap-2">
        <h2 class="text-[11px] uppercase tracking-[0.2em] text-muted-foreground font-semibold">
          {{ t('incident.recentlyResolved') }}
        </h2>
        <span class="text-[11px] text-muted-foreground/70">
          {{ t('incident.last7Days') }}
        </span>
      </div>

      <div v-if="recent.length === 0" class="rounded-md bg-panel/40 p-4 text-xs text-muted-foreground">
        {{ t('incident.noRecent') }}
      </div>

      <ul v-else class="rounded-md bg-panel/50 divide-y divide-border/30">
        <li
          v-for="inc in recent"
          :key="inc.id"
          class="px-3 py-2 flex items-center gap-3 hover:bg-panel/80 cursor-pointer transition-colors"
          @click="openWarRoom(inc)"
        >
          <Flag class="h-3 w-3 text-success shrink-0" />
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <Badge :variant="SEVERITY_STYLE[inc.severity].variant" class="text-[9px]">
                {{ SEVERITY_STYLE[inc.severity].label }}
              </Badge>
              <span class="text-[11px] text-muted-foreground tabular-nums">
                INC-{{ String(inc.number).padStart(4, '0') }}
              </span>
              <span class="text-xs text-foreground truncate">{{ inc.title }}</span>
            </div>
          </div>
          <div class="text-[11px] text-muted-foreground tabular-nums">
            {{ formatElapsed(inc.started_at, inc.resolved_at) }}
          </div>
          <div class="text-[11px] text-muted-foreground/60 hidden sm:block">
            {{ inc.resolved_at ? formatDate(inc.resolved_at) : '' }}
          </div>
        </li>
      </ul>
    </section>

    <!-- Declare dialog -->
    <Dialog v-model:open="showDeclare">
      <DialogContent class="max-w-lg">
        <DialogHeader>
          <DialogTitle>{{ t('incident.declare') }}</DialogTitle>
          <DialogDescription>{{ t('incident.declareDesc') }}</DialogDescription>
        </DialogHeader>

        <div class="space-y-3">
          <div>
            <label class="text-[11px] text-muted-foreground uppercase tracking-wider">
              {{ t('incident.field.titleShort') }}
            </label>
            <input
              v-model="declareForm.title"
              class="mt-1 w-full h-8 px-2 rounded-md bg-background/80 text-xs outline-none focus:ring-1 focus:ring-primary/50"
              :placeholder="t('incident.placeholder.title')"
            >
          </div>

          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="text-[11px] text-muted-foreground uppercase tracking-wider">
                {{ t('incident.field.severity') }}
              </label>
              <Select v-model="declareForm.severity">
                <SelectTrigger class="h-8 mt-1 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="sev1">SEV1 · {{ t('incident.severityHint.sev1') }}</SelectItem>
                  <SelectItem value="sev2">SEV2 · {{ t('incident.severityHint.sev2') }}</SelectItem>
                  <SelectItem value="sev3">SEV3 · {{ t('incident.severityHint.sev3') }}</SelectItem>
                  <SelectItem value="sev4">SEV4 · {{ t('incident.severityHint.sev4') }}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div>
              <label class="text-[11px] text-muted-foreground uppercase tracking-wider">
                {{ t('incident.field.detectionSource') }}
              </label>
              <Select v-model="declareForm.detection_source">
                <SelectTrigger class="h-8 mt-1 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="manual">manual</SelectItem>
                  <SelectItem value="alert">alert</SelectItem>
                  <SelectItem value="slo_burn">slo_burn</SelectItem>
                  <SelectItem value="synthetic">synthetic</SelectItem>
                  <SelectItem value="chaos">chaos</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <div>
            <label class="text-[11px] text-muted-foreground uppercase tracking-wider">
              {{ t('incident.field.impactSummary') }}
            </label>
            <textarea
              v-model="declareForm.impact_summary"
              rows="3"
              class="mt-1 w-full px-2 py-1.5 rounded-md bg-background/80 text-xs outline-none focus:ring-1 focus:ring-primary/50 resize-none"
              :placeholder="t('incident.placeholder.impact')"
            />
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" size="sm" @click="showDeclare = false">
            {{ t('common.cancel') }}
          </Button>
          <Button size="sm" :disabled="declareBusy" @click="submitDeclare">
            {{ declareBusy ? t('common.loading') : t('incident.declare') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
