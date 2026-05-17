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
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Popover, PopoverTrigger, PopoverContent } from '@/components/ui/popover'
import { ScrollArea } from '@/components/ui/scroll-area'
import type { AppNotification } from '@/composables/useNotifications'

const { t } = useI18n()
const router = useRouter()
const {
  unreadCount,
  notifications,
  loaded,
  fetchNotifications,
  markRead,
  markAllRead,
  connect,
  disconnect,
} = useNotifications()

const popoverOpen = ref(false)

onMounted(() => {
  connect()
})

onUnmounted(() => {
  disconnect()
})

// Lazy-load notifications on first popover open
watch(popoverOpen, (open) => {
  if (open) {
    fetchNotifications()
  }
})

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

function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return t('notification.justNow')
  if (mins < 60) return `${mins}m`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h`
  const days = Math.floor(hours / 24)
  return `${days}d`
}

function handleClick(n: AppNotification) {
  if (!n.is_read) markRead(n.id)
  popoverOpen.value = false
  if (n.event_type === 'issue_created' || n.event_type === 'rca_completed' || n.event_type === 'rca_failed' || n.event_type === 'alert_fired') {
    router.push('/issues')
  } else if (n.event_type === 'deployment_change') {
    router.push('/deployments')
  } else if (n.related_id) {
    router.push('/approvals')
  }
}
</script>

<template>
  <ClientOnly>
    <Popover v-model:open="popoverOpen">
      <PopoverTrigger as-child>
        <Button
          variant="ghost"
          size="sm"
          class="relative h-8 w-8 p-0 text-muted-foreground hover:text-foreground hover:bg-white/5 transition-colors"
        >
          <Bell class="h-4 w-4" />
          <!-- Unread badge -->
          <span
            v-if="unreadCount > 0"
            class="absolute -top-0.5 -right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-red-500 px-1 text-[9px] font-bold text-white shadow-[0_0_8px_rgba(239,68,68,0.55)] animate-pulse"
          >
            {{ unreadCount > 99 ? '99+' : unreadCount }}
          </span>
        </Button>
      </PopoverTrigger>

      <PopoverContent
        class="w-80 p-0 border border-[hsl(225,15%,15%)] bg-[hsl(225,20%,7%)] shadow-[0_8px_32px_rgba(0,0,0,0.6)]"
        align="end"
        :side-offset="8"
      >
        <!-- Header -->
        <div class="flex items-center justify-between border-b border-white/[0.07] px-3 py-2">
          <div class="flex items-center gap-2">
            <Bell class="h-3.5 w-3.5 text-[#818CF8]" />
            <span class="text-xs font-semibold text-foreground">{{ t('notification.title') }}</span>
            <span v-if="unreadCount > 0" class="rounded-full bg-red-500/20 px-1.5 py-0 text-[9px] font-semibold text-red-400">
              {{ unreadCount }}
            </span>
          </div>
          <Button
            v-if="unreadCount > 0"
            variant="ghost"
            size="sm"
            class="h-5 text-[10px] px-1.5 text-muted-foreground hover:text-foreground"
            @click="markAllRead"
          >
            <CheckCheck class="h-3 w-3 mr-1" />
            {{ t('notification.markAllRead') }}
          </Button>
        </div>

        <!-- Notification list -->
        <ScrollArea class="max-h-[360px]">
          <!-- Loading skeleton -->
          <div v-if="!loaded" class="space-y-px p-1">
            <div
              v-for="i in 4"
              :key="i"
              class="flex gap-2.5 rounded px-3 py-2.5"
            >
              <div class="h-4 w-4 rounded-full bg-white/5 animate-pulse shrink-0 mt-0.5" />
              <div class="flex-1 space-y-1.5">
                <div class="h-2.5 w-3/4 rounded bg-white/5 animate-pulse" />
                <div class="h-2 w-1/2 rounded bg-white/[0.04] animate-pulse" />
              </div>
            </div>
          </div>

          <!-- Empty state -->
          <div
            v-else-if="notifications.length === 0"
            class="flex flex-col items-center justify-center gap-2 py-10 text-center"
          >
            <Bell class="h-6 w-6 text-muted-foreground/30" />
            <p class="text-xs text-muted-foreground">{{ t('notification.noNotifications') }}</p>
          </div>

          <!-- Items -->
          <div v-else class="divide-y divide-white/[0.04]">
            <button
              v-for="n in notifications.slice(0, 20)"
              :key="n.id"
              class="group flex w-full items-start gap-2.5 px-3 py-2.5 text-left transition-colors hover:bg-white/[0.04]"
              :class="!n.is_read ? 'bg-[#818CF8]/[0.04]' : ''"
              @click="handleClick(n)"
            >
              <!-- Event icon -->
              <component
                :is="eventIcon(n.event_type)"
                class="h-4 w-4 mt-0.5 shrink-0 transition-opacity"
                :class="[eventColor(n.event_type), n.is_read ? 'opacity-50' : '']"
              />

              <!-- Content -->
              <div class="flex-1 min-w-0">
                <p
                  class="text-[11px] leading-tight"
                  :class="n.is_read ? 'text-muted-foreground' : 'text-foreground font-medium'"
                >
                  {{ n.title }}
                </p>
                <p
                  v-if="n.description"
                  class="text-[10px] text-muted-foreground/60 truncate mt-0.5"
                >
                  {{ n.description }}
                </p>
              </div>

              <!-- Time + unread dot -->
              <div class="flex items-center gap-1.5 shrink-0">
                <span class="text-[9px] text-muted-foreground/40">{{ timeAgo(n.created_at) }}</span>
                <span
                  v-if="!n.is_read"
                  class="h-1.5 w-1.5 rounded-full bg-[#818CF8] shadow-[0_0_4px_rgba(129,140,248,0.6)]"
                />
              </div>
            </button>
          </div>
        </ScrollArea>

        <!-- Footer -->
        <div class="border-t border-white/[0.07] px-3 py-1.5">
          <NuxtLink
            to="/notifications"
            class="block text-center text-[10px] text-[#818CF8] hover:text-[#B794F6] transition-colors hover:underline underline-offset-2"
            @click="popoverOpen = false"
          >
            {{ t('notification.viewAll') }}
          </NuxtLink>
        </div>
      </PopoverContent>
    </Popover>
  </ClientOnly>
</template>
