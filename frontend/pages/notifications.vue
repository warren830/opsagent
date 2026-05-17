<script setup lang="ts">
import {
  Bell,
  Check,
  CheckCheck,
  ClipboardCheck,
  XCircle,
  AlertTriangle,
  Undo2,
  Rocket,
  ShieldAlert,
  Zap,
  Filter,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import type { AppNotification } from '@/composables/useNotifications'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const router = useRouter()
const { unreadCount, notifications, loaded, fetchNotifications, markRead, markAllRead } = useNotifications()

const loading = ref(true)
const activeFilter = ref<'all' | 'unread' | 'read'>('all')
const PAGE_SIZE = 50
const currentOffset = ref(0)
const hasMore = ref(true)

onMounted(async () => {
  await fetchNotifications(PAGE_SIZE, 0)
  loading.value = false
})

const filteredNotifications = computed<AppNotification[]>(() => {
  const list = notifications.value as AppNotification[]
  if (activeFilter.value === 'unread') return list.filter(n => !n.is_read)
  if (activeFilter.value === 'read') return list.filter(n => n.is_read)
  return list
})

async function loadMore() {
  currentOffset.value += PAGE_SIZE
  const batch = await fetchNotifications(PAGE_SIZE, currentOffset.value)
  if (batch.length < PAGE_SIZE) hasMore.value = false
}

async function handleMarkAllRead() {
  await markAllRead()
}

async function handleClick(n: AppNotification) {
  if (!n.is_read) markRead(n.id)
  if (n.event_type === 'issue_created' || n.event_type === 'rca_completed' || n.event_type === 'rca_failed' || n.event_type === 'alert_fired') {
    router.push('/issues')
  } else if (n.event_type === 'deployment_change') {
    router.push('/deployments')
  } else if (n.related_id) {
    router.push('/approvals')
  }
}

function eventIcon(type: string) {
  switch (type) {
    case 'approval_submitted': return ClipboardCheck
    case 'approval_approved': return Check
    case 'approval_rejected': return XCircle
    case 'approval_executed': return CheckCheck
    case 'approval_failed': return AlertTriangle
    case 'approval_withdrawn': return Undo2
    case 'issue_created': return ShieldAlert
    case 'rca_completed': return Check
    case 'rca_failed': return AlertTriangle
    case 'deployment_change': return Rocket
    case 'alert_fired': return Zap
    default: return Bell
  }
}

function eventColor(type: string): string {
  switch (type) {
    case 'approval_approved':
    case 'approval_executed':
    case 'rca_completed':
      return 'text-emerald-400'
    case 'approval_rejected':
    case 'approval_failed':
    case 'rca_failed':
      return 'text-red-400'
    case 'approval_submitted':
      return 'text-[#818CF8]'
    case 'approval_withdrawn':
      return 'text-amber-400'
    case 'issue_created':
    case 'alert_fired':
      return 'text-orange-400'
    case 'deployment_change':
      return 'text-[#5EEAD4]'
    default:
      return 'text-muted-foreground'
  }
}

function eventBadgeVariant(type: string): 'success' | 'destructive' | 'info' | 'warning' | 'secondary' {
  if (type.includes('approved') || type.includes('executed') || type.includes('completed')) return 'success'
  if (type.includes('rejected') || type.includes('failed')) return 'destructive'
  if (type.includes('submitted') || type.includes('deployment')) return 'info'
  if (type.includes('created') || type.includes('fired') || type.includes('withdrawn')) return 'warning'
  return 'secondary'
}

function eventLabel(type: string): string {
  const map: Record<string, string> = {
    approval_submitted: t('notification.submitted'),
    approval_approved: t('notification.approved'),
    approval_rejected: t('notification.rejected'),
    approval_executed: t('notification.executed'),
    approval_failed: t('notification.failed'),
    approval_withdrawn: t('notification.withdrawn'),
    issue_created: t('notification.issueCreated'),
    rca_completed: t('notification.rcaCompleted'),
    rca_failed: t('notification.rcaFailed'),
    deployment_change: t('notification.deploymentChange'),
    alert_fired: t('notification.alertFired'),
  }
  return map[type] || type
}

function formatDate(dateStr: string): string {
  return new Date(dateStr).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return t('notification.justNow')
  if (mins < 60) return `${mins}${t('notification.minuteAbbr')}`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}${t('notification.hourAbbr')}`
  const days = Math.floor(hours / 24)
  return `${days}${t('notification.dayAbbr')}`
}

const filters = [
  { value: 'all' as const, label: () => t('notification.filterAll') },
  { value: 'unread' as const, label: () => t('notification.filterUnread') },
  { value: 'read' as const, label: () => t('notification.filterRead') },
]
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <Bell class="h-4 w-4 text-[#818CF8]" />
        <h1 class="text-base font-semibold text-foreground">{{ t('notification.title') }}</h1>
        <span
          v-if="unreadCount > 0"
          class="rounded-full bg-red-500/20 px-2 py-0 text-[10px] font-semibold text-red-400"
        >
          {{ unreadCount }}
        </span>
      </div>

      <Button
        v-if="unreadCount > 0"
        variant="outline"
        size="sm"
        class="h-8 text-xs border-white/10 hover:border-[#818CF8]/40 hover:text-[#818CF8]"
        @click="handleMarkAllRead"
      >
        <CheckCheck class="h-3.5 w-3.5 mr-1.5" />
        {{ t('notification.markAllRead') }}
      </Button>
    </div>

    <!-- Filter tabs -->
    <div class="flex items-center gap-1.5">
      <Filter class="h-3.5 w-3.5 text-muted-foreground shrink-0" />
      <Button
        v-for="f in filters"
        :key="f.value"
        size="sm"
        :variant="activeFilter === f.value ? 'default' : 'outline'"
        class="h-7 text-xs"
        :class="activeFilter !== f.value && 'border-white/10 text-muted-foreground hover:text-foreground'"
        @click="activeFilter = f.value"
      >
        {{ f.label() }}
        <span
          v-if="f.value === 'unread' && unreadCount > 0"
          class="ml-1 rounded-full bg-red-500/30 px-1.5 py-0 text-[9px] font-semibold text-red-400"
        >
          {{ unreadCount }}
        </span>
      </Button>
    </div>

    <!-- Loading skeleton -->
    <div v-if="loading" class="space-y-1.5">
      <div
        v-for="i in 6"
        :key="i"
        class="flex items-start gap-3 rounded-md border border-white/[0.06] bg-white/[0.02] px-4 py-3"
      >
        <div class="h-5 w-5 rounded-full bg-white/5 animate-pulse shrink-0 mt-0.5" />
        <div class="flex-1 space-y-2">
          <div class="h-3 w-1/3 rounded bg-white/5 animate-pulse" />
          <div class="h-2.5 w-2/3 rounded bg-white/[0.04] animate-pulse" />
          <div class="h-2 w-1/2 rounded bg-white/[0.03] animate-pulse" />
        </div>
        <div class="h-2.5 w-10 rounded bg-white/[0.04] animate-pulse mt-1 shrink-0" />
      </div>
    </div>

    <!-- Empty state -->
    <div
      v-else-if="filteredNotifications.length === 0"
      class="flex flex-col items-center justify-center gap-3 py-20 text-center"
    >
      <div class="flex h-14 w-14 items-center justify-center rounded-full bg-white/[0.04] border border-white/[0.07]">
        <Bell class="h-6 w-6 text-muted-foreground/30" />
      </div>
      <p class="text-sm text-muted-foreground">{{ t('notification.noNotifications') }}</p>
      <p class="text-xs text-muted-foreground/50">
        {{ activeFilter === 'unread' ? t('notification.noUnread') : t('notification.emptyHint') }}
      </p>
    </div>

    <!-- Notification list -->
    <div v-else class="space-y-1">
      <button
        v-for="n in filteredNotifications"
        :key="n.id"
        class="group flex w-full items-start gap-3 rounded-md border px-4 py-3 text-left transition-all hover:border-white/10 hover:bg-white/[0.03]"
        :class="!n.is_read
          ? 'border-[#818CF8]/20 bg-[#818CF8]/[0.04] hover:border-[#818CF8]/30'
          : 'border-white/[0.06] bg-white/[0.015]'"
        @click="handleClick(n)"
      >
        <!-- Icon column -->
        <div
          class="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-md border"
          :class="!n.is_read
            ? 'border-white/10 bg-white/[0.06]'
            : 'border-white/[0.06] bg-white/[0.02]'"
        >
          <component
            :is="eventIcon(n.event_type)"
            class="h-3.5 w-3.5 transition-opacity"
            :class="[eventColor(n.event_type), n.is_read ? 'opacity-40' : '']"
          />
        </div>

        <!-- Content -->
        <div class="flex-1 min-w-0 space-y-1">
          <!-- Badge + unread indicator -->
          <div class="flex items-center gap-2">
            <Badge
              :variant="eventBadgeVariant(n.event_type) as any"
              class="text-[9px] h-4 px-1.5 uppercase tracking-wide"
            >
              {{ eventLabel(n.event_type) }}
            </Badge>
            <span
              v-if="!n.is_read"
              class="h-1.5 w-1.5 rounded-full bg-[#818CF8] shadow-[0_0_4px_rgba(129,140,248,0.6)] shrink-0"
            />
          </div>

          <!-- Title -->
          <p
            class="text-xs leading-snug"
            :class="n.is_read ? 'text-muted-foreground' : 'font-medium text-foreground'"
          >
            {{ n.title }}
          </p>

          <!-- Description -->
          <p
            v-if="n.description"
            class="text-[11px] text-muted-foreground/60 line-clamp-2 leading-relaxed"
          >
            {{ n.description }}
          </p>
        </div>

        <!-- Time column -->
        <div class="shrink-0 flex flex-col items-end gap-1 pt-0.5">
          <span class="text-[10px] text-muted-foreground/40 tabular-nums whitespace-nowrap">
            {{ timeAgo(n.created_at) }}
          </span>
          <span class="text-[9px] text-muted-foreground/25 tabular-nums whitespace-nowrap hidden group-hover:inline">
            {{ formatDate(n.created_at) }}
          </span>
        </div>
      </button>
    </div>

    <!-- Load more -->
    <div v-if="!loading && hasMore && filteredNotifications.length >= PAGE_SIZE" class="flex justify-center pt-2">
      <Button
        variant="outline"
        size="sm"
        class="h-7 text-xs border-white/10 text-muted-foreground hover:text-foreground"
        @click="loadMore"
      >
        {{ t('notification.loadMore') }}
      </Button>
    </div>

    <!-- Aurora bottom decoration strip -->
    <div class="pointer-events-none fixed bottom-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-[#818CF8]/20 to-transparent" />
  </div>
</template>
