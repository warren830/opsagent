<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  ArrowLeft, Clock3, Megaphone, Hash, LinkIcon, Brain, Plus,
  Users, Layers, Rocket, Activity, AlertCircle, MessageSquare, FileText,
} from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter,
} from '@/components/ui/dialog'
import type { IncidentTimelineEvent } from '@/composables/useIncidentStream'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const api = useApi()

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
  source_issue_id: string | null
  started_at: string
  detected_at: string
  acknowledged_at: string | null
  mitigated_at: string | null
  resolved_at: string | null
  closed_at: string | null
  war_room_channel_ref: { slack_channel_id?: string; url?: string } | null
  bridge_url: string | null
  jira_key: string | null
  labels: Record<string, unknown>
  slo_budget_burn: Record<string, unknown> | null
}

interface Participant {
  incident_id: string
  user_id: string
  role: string
  joined_at: string
  left_at: string | null
  added_via: string
}

interface IncidentUpdate {
  id: string
  incident_id: string
  author_user_id: string | null
  audience: string
  status_at_time: string
  body_markdown: string
  published_at: string | null
  created_at: string
}

interface CatalogEntity {
  id: string
  name: string
  kind: string
  system_id: string | null
}

const incidentId = computed(() => route.params.id as string)

const inc = ref<Incident | null>(null)
const participants = ref<Participant[]>([])
const updates = ref<IncidentUpdate[]>([])
const components = ref<CatalogEntity[]>([])
const loading = ref(true)
const noteInput = ref('')
const savingNote = ref(false)

// Live timeline from SSE.
const { events: timelineEvents, connected, refresh } = useIncidentStream(incidentId)

// Status transition handling.
const pendingStatus = ref<string | null>(null)
const reasonDialogOpen = ref(false)
const reasonInput = ref('')
const transitionBusy = ref(false)
const severityBusy = ref(false)

// Draft update dialog.
const updateDraftOpen = ref(false)
const updateForm = ref({ audience: 'internal', body_markdown: '', publish: true })
const updateBusy = ref(false)

const ALL_STATUSES = [
  'triggered', 'acknowledged', 'investigating', 'identified', 'mitigated',
  'resolved', 'postmortem_draft', 'postmortem_published', 'closed',
]
const ALL_SEVERITIES = ['sev1', 'sev2', 'sev3', 'sev4']

const SEVERITY_STYLE: Record<string, { variant: 'destructive' | 'warning' | 'info' | 'secondary'; label: string }> = {
  sev1: { variant: 'destructive', label: 'SEV1' },
  sev2: { variant: 'warning', label: 'SEV2' },
  sev3: { variant: 'info', label: 'SEV3' },
  sev4: { variant: 'secondary', label: 'SEV4' },
}

function statusLabel(status: string): string {
  return t(`incident.status.${status}`, status)
}

const REASON_REQUIRED_EDGES: Array<[string, string]> = [
  ['identified', 'investigating'],
  ['mitigated', 'investigating'],
  ['resolved', 'investigating'],
]

function reasonRequired(from: string, to: string): boolean {
  return REASON_REQUIRED_EDGES.some(([f, t]) => f === from && t === to)
}

function formatElapsed(startIso: string | null | undefined, endIso: string | null): string {
  if (!startIso) return '—'
  const start = new Date(startIso).getTime()
  const end = endIso ? new Date(endIso).getTime() : Date.now()
  const s = Math.max(0, Math.floor((end - start) / 1000))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  if (h > 0) return `${h}h${String(m).padStart(2, '0')}m`
  if (m > 0) return `${m}m${String(s % 60).padStart(2, '0')}s`
  return `${s}s`
}

function formatTs(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  })
}

async function fetchIncident() {
  loading.value = true
  try {
    const detail = await api.get<{
      incident: Incident
      timeline: IncidentTimelineEvent[]
      participants: Participant[]
      recent_updates: IncidentUpdate[]
    } & Incident>(`/api/incidents/${incidentId.value}`)
    // Backend flattens Incident into the response.
    inc.value = {
      id: detail.id,
      number: detail.number,
      title: detail.title,
      severity: detail.severity,
      status: detail.status,
      commander_user_id: detail.commander_user_id,
      impact_summary: detail.impact_summary,
      affected_component_ids: detail.affected_component_ids,
      affected_customer_tier: detail.affected_customer_tier,
      detection_source: detail.detection_source,
      source_issue_id: detail.source_issue_id,
      started_at: detail.started_at,
      detected_at: detail.detected_at,
      acknowledged_at: detail.acknowledged_at,
      mitigated_at: detail.mitigated_at,
      resolved_at: detail.resolved_at,
      closed_at: detail.closed_at,
      war_room_channel_ref: detail.war_room_channel_ref,
      bridge_url: detail.bridge_url,
      jira_key: detail.jira_key,
      labels: detail.labels,
      slo_budget_burn: detail.slo_budget_burn,
    }
    participants.value = detail.participants || []
    // Fetch all updates (detail only returns the most recent 5).
    updates.value = await api.get<IncidentUpdate[]>(`/api/incidents/${incidentId.value}/updates`)

    if (inc.value.affected_component_ids.length > 0) {
      try {
        // catalog list returns all tenant entities; filter client-side.
        const allEntities = await api.get<CatalogEntity[]>('/api/catalog/entities')
        const ids = new Set(inc.value.affected_component_ids)
        components.value = allEntities.filter((e) => ids.has(e.id))
      } catch {
        components.value = []
      }
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

function handleStatusSelect(newStatus: string) {
  if (!inc.value) return
  if (newStatus === inc.value.status) return
  pendingStatus.value = newStatus
  if (reasonRequired(inc.value.status, newStatus)) {
    reasonInput.value = ''
    reasonDialogOpen.value = true
  } else {
    void executeTransition(null)
  }
}

async function executeTransition(reason: string | null) {
  if (!inc.value || !pendingStatus.value) return
  transitionBusy.value = true
  try {
    const updated = await api.post<Incident>(`/api/incidents/${inc.value.id}/transition`, {
      to_status: pendingStatus.value,
      reason: reason || undefined,
    })
    inc.value.status = updated.status
    inc.value.acknowledged_at = updated.acknowledged_at
    inc.value.mitigated_at = updated.mitigated_at
    inc.value.resolved_at = updated.resolved_at
    inc.value.closed_at = updated.closed_at
    toast.success(t('incident.transitionOk', { status: statusLabel(updated.status) }))
    reasonDialogOpen.value = false
    pendingStatus.value = null
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    transitionBusy.value = false
  }
}

async function handleSeverityChange(newSeverity: string) {
  if (!inc.value || newSeverity === inc.value.severity) return
  const reason = window.prompt(t('incident.severityReasonPrompt')) || ''
  if (!reason.trim()) {
    toast.error(t('incident.errors.reasonRequired'))
    return
  }
  severityBusy.value = true
  try {
    const updated = await api.post<Incident>(`/api/incidents/${inc.value.id}/severity`, {
      to_severity: newSeverity,
      reason: reason.trim(),
    })
    inc.value.severity = updated.severity
    toast.success(t('incident.severityOk'))
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    severityBusy.value = false
  }
}

async function submitNote() {
  if (!inc.value || !noteInput.value.trim()) return
  savingNote.value = true
  try {
    await api.post(`/api/incidents/${inc.value.id}/timeline`, {
      summary: noteInput.value.trim(),
      kind: 'manual_note',
      payload: {},
    })
    noteInput.value = ''
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    savingNote.value = false
  }
}

async function submitUpdate() {
  if (!inc.value || !updateForm.value.body_markdown.trim()) {
    toast.error(t('incident.errors.bodyRequired'))
    return
  }
  updateBusy.value = true
  try {
    const created = await api.post<IncidentUpdate>(`/api/incidents/${inc.value.id}/updates`, {
      audience: updateForm.value.audience,
      body_markdown: updateForm.value.body_markdown.trim(),
      publish: updateForm.value.publish,
    })
    updates.value = [created, ...updates.value]
    toast.success(updateForm.value.publish ? t('incident.updatePublished') : t('incident.updateDrafted'))
    updateDraftOpen.value = false
    updateForm.value = { audience: 'internal', body_markdown: '', publish: true }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    updateBusy.value = false
  }
}

// Ticking clock for the duration badge in the header.
const now = ref(Date.now())
let timer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  fetchIncident()
  timer = setInterval(() => { now.value = Date.now() }, 15_000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})

// Re-fetch on id change (unlikely but safe).
watch(incidentId, (id, old) => {
  if (id && id !== old) fetchIncident()
})

function slackHref(): string | null {
  const ref = inc.value?.war_room_channel_ref
  if (!ref) return null
  if (typeof ref.url === 'string' && ref.url) return ref.url
  if (typeof ref.slack_channel_id === 'string' && ref.slack_channel_id) {
    return `slack://channel?id=${ref.slack_channel_id}`
  }
  return null
}

function timelineKindStyle(kind: string): { icon: string; color: string } {
  if (kind === 'incident_status_changed') return { icon: '🔀', color: 'text-info' }
  if (kind === 'incident_severity_changed') return { icon: '⚠️', color: 'text-warning' }
  if (kind === 'join' || kind === 'leave') return { icon: '👤', color: 'text-muted-foreground' }
  if (kind === 'update_published' || kind === 'update_drafted') return { icon: '📢', color: 'text-info' }
  if (kind.startsWith('deploy')) return { icon: '🚀', color: 'text-primary' }
  if (kind === 'rollback_initiated') return { icon: '⏪', color: 'text-destructive' }
  if (kind === 'promote_initiated') return { icon: '⏫', color: 'text-success' }
  if (kind === 'chat_tool_call') return { icon: '🤖', color: 'text-ai' }
  if (kind === 'manual_note') return { icon: '📝', color: 'text-foreground' }
  if (kind === 'postmortem_draft_ready') return { icon: '📄', color: 'text-success' }
  if (kind === 'postmortem_draft_error') return { icon: '⚠️', color: 'text-destructive' }
  return { icon: '•', color: 'text-muted-foreground' }
}

function openAgentChat() {
  if (!inc.value) return
  router.push(`/?incident_id=${inc.value.id}`)
}

function openPostmortem() {
  if (!inc.value) return
  router.push(`/incidents/${inc.value.id}/postmortem`)
}

function openJira() {
  if (!inc.value?.jira_key) return
  // Jira URL is tenant-specific — navigate by key only; the channel
  // configuration stores the base URL. Without it we show a toast.
  toast.info(inc.value.jira_key)
}
</script>

<template>
  <div v-if="loading" class="p-6">
    <div class="h-8 w-48 bg-panel/60 rounded animate-pulse mb-4" />
    <div class="h-32 bg-panel/40 rounded animate-pulse" />
  </div>

  <div v-else-if="!inc" class="p-6">
    <p class="text-xs text-muted-foreground">{{ t('incident.notFound') }}</p>
  </div>

  <div v-else class="flex flex-col h-[calc(100vh-4rem)] gap-3">
    <!-- Top bar -->
    <header class="flex items-center gap-3 flex-wrap">
      <Button variant="ghost" size="sm" class="h-7 px-2" @click="router.push('/incidents')">
        <ArrowLeft class="h-3 w-3" />
        <span class="text-[11px]">{{ t('common.back') }}</span>
      </Button>

      <div class="flex items-center gap-1.5 text-sm font-semibold text-foreground tabular-nums">
        INC-{{ String(inc.number).padStart(4, '0') }}
      </div>

      <Select :model-value="inc.severity" :disabled="severityBusy" @update:model-value="(v) => handleSeverityChange(v as string)">
        <SelectTrigger class="h-7 text-xs w-24">
          <SelectValue>
            <Badge :variant="SEVERITY_STYLE[inc.severity].variant" class="text-[10px] font-semibold">
              {{ SEVERITY_STYLE[inc.severity].label }}
            </Badge>
          </SelectValue>
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="s in ALL_SEVERITIES" :key="s" :value="s">{{ s.toUpperCase() }}</SelectItem>
        </SelectContent>
      </Select>

      <Select :model-value="inc.status" :disabled="transitionBusy" @update:model-value="(v) => handleStatusSelect(v as string)">
        <SelectTrigger class="h-7 text-xs w-44">
          <SelectValue>
            <span class="text-[11px]">{{ statusLabel(inc.status) }}</span>
          </SelectValue>
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="s in ALL_STATUSES" :key="s" :value="s">{{ statusLabel(s) }}</SelectItem>
        </SelectContent>
      </Select>

      <div class="flex items-center gap-1 text-[11px] text-warning tabular-nums">
        <Clock3 class="h-3 w-3" />
        <span>{{ formatElapsed(inc.started_at, inc.resolved_at) }}</span>
        <span class="hidden">{{ now }}</span>
      </div>

      <div class="ml-auto flex items-center gap-2">
        <Badge v-if="!connected" variant="warning" class="text-[10px]">
          {{ t('incident.streamDisconnected') }}
        </Badge>
        <Button v-if="inc.jira_key" variant="outline" size="sm" class="h-7 px-2 text-[11px]" @click="openJira">
          <Hash class="h-3 w-3" />
          {{ inc.jira_key }}
        </Button>
        <a v-if="slackHref()" :href="slackHref() || '#'" target="_blank" class="inline-flex items-center gap-1 text-[11px] text-primary hover:underline">
          <Megaphone class="h-3 w-3" />
          {{ t('incident.warRoom') }}
        </a>
        <Button variant="outline" size="sm" class="h-7 px-2 text-[11px]" @click="openPostmortem">
          <FileText class="h-3 w-3" />
          {{ t('incident.postmortem') }}
        </Button>
      </div>
    </header>

    <!-- Title + impact -->
    <section class="bg-panel/50 rounded-md p-3 space-y-1">
      <h1 class="text-base font-semibold text-foreground leading-tight">{{ inc.title }}</h1>
      <p v-if="inc.impact_summary" class="text-xs text-muted-foreground leading-relaxed">
        {{ inc.impact_summary }}
      </p>
      <div class="flex items-center gap-2 text-[11px] text-muted-foreground flex-wrap pt-1">
        <span class="inline-flex items-center gap-1">
          <Users class="h-3 w-3" />
          {{ t('incident.respondersCount', { n: participants.length }) }}
        </span>
        <span v-if="inc.bridge_url" class="inline-flex items-center gap-1">
          <LinkIcon class="h-3 w-3" />
          <a :href="inc.bridge_url" target="_blank" class="hover:underline text-primary">{{ t('incident.bridge') }}</a>
        </span>
        <span class="inline-flex items-center gap-1">
          <AlertCircle class="h-3 w-3" />
          {{ t('incident.detectionSource') }}: {{ inc.detection_source }}
        </span>
      </div>
    </section>

    <!-- Three-column main -->
    <section class="grid grid-cols-1 lg:grid-cols-[260px_1fr_300px] gap-3 flex-1 min-h-0">
      <!-- LEFT: affected + slo + recent changes + participants -->
      <div class="flex flex-col gap-3 overflow-y-auto pr-1">
        <div class="bg-panel/60 rounded-md p-3 space-y-2">
          <h3 class="text-[10px] uppercase tracking-[0.2em] text-muted-foreground font-semibold flex items-center gap-1.5">
            <Layers class="h-3 w-3" />
            {{ t('incident.affectedServices') }}
          </h3>
          <div v-if="components.length === 0" class="text-[11px] text-muted-foreground/70">
            {{ t('incident.noComponents') }}
          </div>
          <div v-else class="flex flex-wrap gap-1">
            <a
              v-for="c in components"
              :key="c.id"
              :href="`/topology?component=${c.id}`"
              class="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-background/60 text-[11px] hover:bg-background/90 text-foreground/90 transition-colors"
            >
              {{ c.name }}
            </a>
          </div>
        </div>

        <div class="bg-panel/60 rounded-md p-3 space-y-2">
          <h3 class="text-[10px] uppercase tracking-[0.2em] text-muted-foreground font-semibold flex items-center gap-1.5">
            <Activity class="h-3 w-3" />
            {{ t('incident.sloImpact') }}
          </h3>
          <div v-if="!inc.slo_budget_burn || Object.keys(inc.slo_budget_burn).length === 0"
               class="text-[11px] text-muted-foreground/70">
            {{ t('incident.sloNone') }}
          </div>
          <div v-else class="space-y-1 text-[11px]">
            <div v-for="(val, key) in inc.slo_budget_burn" :key="String(key)"
                 class="flex items-center justify-between">
              <span class="text-muted-foreground truncate pr-2">{{ key }}</span>
              <span class="text-warning tabular-nums">{{ typeof val === 'number' ? `${(val * 100).toFixed(1)}%` : String(val) }}</span>
            </div>
          </div>
        </div>

        <div class="bg-panel/60 rounded-md p-3 space-y-2">
          <h3 class="text-[10px] uppercase tracking-[0.2em] text-muted-foreground font-semibold flex items-center gap-1.5">
            <Rocket class="h-3 w-3" />
            {{ t('incident.recentChanges') }}
          </h3>
          <!-- MVP: surface deploy/rollback events from the timeline feed. -->
          <div v-if="timelineEvents.filter((e) => e.kind.startsWith('deploy') || e.kind === 'rollback_initiated' || e.kind === 'promote_initiated').length === 0"
               class="text-[11px] text-muted-foreground/70">
            {{ t('incident.noRecentChanges') }}
          </div>
          <ul v-else class="space-y-1 text-[11px]">
            <li v-for="e in timelineEvents.filter((ev) => ev.kind.startsWith('deploy') || ev.kind === 'rollback_initiated' || ev.kind === 'promote_initiated').slice(-5).reverse()"
                :key="e.id"
                class="flex items-start gap-1.5">
              <span class="text-muted-foreground tabular-nums shrink-0 w-12">{{ formatTs(e.occurred_at) }}</span>
              <span class="truncate">{{ e.summary }}</span>
            </li>
          </ul>
        </div>

        <div class="bg-panel/60 rounded-md p-3 space-y-2">
          <h3 class="text-[10px] uppercase tracking-[0.2em] text-muted-foreground font-semibold flex items-center gap-1.5">
            <Users class="h-3 w-3" />
            {{ t('incident.participants') }}
          </h3>
          <div v-if="participants.length === 0" class="text-[11px] text-muted-foreground/70">
            {{ t('incident.noParticipants') }}
          </div>
          <ul v-else class="space-y-0.5 text-[11px]">
            <li v-for="p in participants" :key="`${p.user_id}-${p.role}`"
                class="flex items-center justify-between">
              <span class="truncate">{{ p.user_id.slice(0, 8) }}…</span>
              <Badge variant="outline" class="text-[9px]">{{ p.role }}</Badge>
            </li>
          </ul>
        </div>
      </div>

      <!-- MIDDLE: timeline + add note -->
      <div class="flex flex-col bg-panel/60 rounded-md overflow-hidden">
        <div class="px-3 py-2 flex items-center gap-2 border-b border-border/30">
          <Activity class="h-3 w-3 text-info" />
          <h3 class="text-[11px] uppercase tracking-[0.2em] text-muted-foreground font-semibold">
            {{ t('incident.timeline') }}
          </h3>
          <span class="text-[11px] text-muted-foreground/60 tabular-nums">
            {{ timelineEvents.length }}
          </span>
          <Button variant="ghost" size="sm" class="h-6 px-2 ml-auto text-[10px]" @click="refresh">
            {{ t('common.refresh') }}
          </Button>
        </div>

        <div class="flex-1 overflow-y-auto px-3 py-2 space-y-2">
          <div v-if="timelineEvents.length === 0" class="text-[11px] text-muted-foreground/70 pt-8 text-center">
            {{ t('incident.timelineEmpty') }}
          </div>
          <ul class="space-y-1.5">
            <li v-for="ev in [...timelineEvents].reverse()" :key="ev.id"
                class="flex items-start gap-2 text-[11px] leading-snug">
              <span class="shrink-0 tabular-nums w-14 text-muted-foreground/60 font-mono">
                {{ formatTs(ev.occurred_at) }}
              </span>
              <span :class="[timelineKindStyle(ev.kind).color, 'shrink-0 w-4 text-center']">
                {{ timelineKindStyle(ev.kind).icon }}
              </span>
              <span class="flex-1 text-foreground/90 break-words">
                {{ ev.summary }}
                <span class="text-[9px] text-muted-foreground/50 ml-1 tabular-nums">
                  [{{ ev.kind }}]
                </span>
              </span>
            </li>
          </ul>
        </div>

        <div class="border-t border-border/30 p-2 flex items-center gap-2">
          <input
            v-model="noteInput"
            class="flex-1 h-8 px-2 rounded-md bg-background/70 text-xs outline-none focus:ring-1 focus:ring-primary/50"
            :placeholder="t('incident.addNotePlaceholder')"
            @keydown.enter="submitNote"
          >
          <Button size="sm" class="h-8 px-3" :disabled="savingNote || !noteInput.trim()" @click="submitNote">
            <Plus class="h-3 w-3" />
            {{ t('incident.addNote') }}
          </Button>
        </div>
      </div>

      <!-- RIGHT: updates -->
      <div class="flex flex-col bg-panel/60 rounded-md overflow-hidden">
        <div class="px-3 py-2 flex items-center gap-2 border-b border-border/30">
          <Megaphone class="h-3 w-3 text-info" />
          <h3 class="text-[11px] uppercase tracking-[0.2em] text-muted-foreground font-semibold">
            {{ t('incident.updates') }}
          </h3>
          <Button variant="ghost" size="sm" class="h-6 px-2 ml-auto text-[10px]"
                  @click="updateDraftOpen = true">
            <Plus class="h-3 w-3" />
            {{ t('incident.draftUpdate') }}
          </Button>
        </div>

        <div class="flex-1 overflow-y-auto px-3 py-2 space-y-3">
          <div v-if="updates.length === 0" class="text-[11px] text-muted-foreground/70 pt-8 text-center">
            {{ t('incident.updatesEmpty') }}
          </div>
          <article v-for="u in updates" :key="u.id"
                   class="bg-background/50 rounded-md p-2.5 space-y-1">
            <div class="flex items-center justify-between">
              <Badge variant="outline" class="text-[9px]">{{ u.audience }}</Badge>
              <span class="text-[10px] text-muted-foreground/70 tabular-nums">
                {{ formatDateTime(u.created_at) }}
              </span>
            </div>
            <p class="text-[11px] text-foreground/90 whitespace-pre-wrap break-words">
              {{ u.body_markdown }}
            </p>
            <div class="text-[9px] text-muted-foreground/60 flex items-center gap-1">
              <span v-if="u.published_at" class="text-success">• {{ t('incident.published') }}</span>
              <span v-else class="text-warning">• {{ t('incident.draft') }}</span>
            </div>
          </article>
        </div>
      </div>
    </section>

    <!-- Bottom agent chat strip -->
    <section class="bg-panel/60 rounded-md p-3 flex items-center gap-2">
      <Brain class="h-4 w-4 text-ai" />
      <div class="flex-1">
        <p class="text-xs text-foreground">{{ t('incident.agentCta') }}</p>
        <p class="text-[10px] text-muted-foreground">{{ t('incident.agentCtaDesc') }}</p>
      </div>
      <Button size="sm" @click="openAgentChat">
        <MessageSquare class="h-3 w-3" />
        {{ t('incident.openAgentChat') }}
      </Button>
    </section>

    <!-- Reason dialog -->
    <Dialog v-model:open="reasonDialogOpen">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>{{ t('incident.reasonTitle') }}</DialogTitle>
          <DialogDescription>{{ t('incident.reasonDesc') }}</DialogDescription>
        </DialogHeader>
        <textarea
          v-model="reasonInput"
          rows="3"
          class="w-full px-2 py-1.5 rounded-md bg-background/80 text-xs outline-none focus:ring-1 focus:ring-primary/50 resize-none"
          :placeholder="t('incident.reasonPlaceholder')"
        />
        <DialogFooter>
          <Button variant="outline" size="sm" @click="reasonDialogOpen = false; pendingStatus = null">
            {{ t('common.cancel') }}
          </Button>
          <Button size="sm" :disabled="transitionBusy || !reasonInput.trim()" @click="executeTransition(reasonInput.trim())">
            {{ t('common.confirm') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <!-- Draft update dialog -->
    <Dialog v-model:open="updateDraftOpen">
      <DialogContent class="max-w-lg">
        <DialogHeader>
          <DialogTitle>{{ t('incident.draftUpdate') }}</DialogTitle>
          <DialogDescription>{{ t('incident.draftUpdateDesc') }}</DialogDescription>
        </DialogHeader>
        <div class="space-y-3">
          <div>
            <label class="text-[11px] text-muted-foreground uppercase tracking-wider">
              {{ t('incident.field.audience') }}
            </label>
            <Select v-model="updateForm.audience">
              <SelectTrigger class="h-8 mt-1 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="internal">internal</SelectItem>
                <SelectItem value="customers">customers</SelectItem>
                <SelectItem value="stakeholders">stakeholders</SelectItem>
                <SelectItem value="status_page">status_page</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div>
            <label class="text-[11px] text-muted-foreground uppercase tracking-wider">
              {{ t('incident.field.body') }}
            </label>
            <textarea
              v-model="updateForm.body_markdown"
              rows="6"
              class="mt-1 w-full px-2 py-1.5 rounded-md bg-background/80 text-xs outline-none focus:ring-1 focus:ring-primary/50 resize-none font-mono"
              :placeholder="t('incident.placeholder.updateBody')"
            />
          </div>
          <label class="inline-flex items-center gap-2 text-[11px] text-foreground">
            <input type="checkbox" v-model="updateForm.publish">
            {{ t('incident.publishNow') }}
          </label>
        </div>
        <DialogFooter>
          <Button variant="outline" size="sm" @click="updateDraftOpen = false">
            {{ t('common.cancel') }}
          </Button>
          <Button size="sm" :disabled="updateBusy" @click="submitUpdate">
            {{ updateBusy ? t('common.loading') : t('common.save') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
