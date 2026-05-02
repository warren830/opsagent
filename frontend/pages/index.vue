<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import {
  Activity,
  Building2,
  Users,
  Zap,
  Server,
  AlertTriangle,
  RefreshCw,
  Cloud,
  ChevronRight,
} from 'lucide-vue-next'
import { Skeleton } from '@/components/ui/skeleton'
import { Button } from '@/components/ui/button'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const authStore = useAuthStore()
const api = useApi()

// Live data for "Systems online" chips — clusters + ranked issues.
// See useHomeLiveData.ts for the issue-ranking decision.
const {
  clusters: liveClusters,
  issues: liveIssues,
  clustersLoading,
  issuesLoading,
} = useHomeLiveData()

// Small helpers for issue chip styling — severity → token class.
const sevBadgeVariant: Record<string, 'destructive' | 'warning' | 'info' | 'secondary'> = {
  critical: 'destructive',
  high: 'destructive',
  medium: 'warning',
  low: 'info',
}
const sevBadgeLabel: Record<string, string> = {
  critical: 'P0', high: 'P1', medium: 'P2', low: 'P3',
}

// Cluster status → dot color. Healthy-looking statuses go to success,
// anything else flags via destructive.
function clusterDotClass(status: string): string {
  const s = (status || '').toLowerCase()
  if (s === 'active' || s === 'healthy' || s === 'ready') return 'bg-success'
  if (s === 'degraded' || s === 'warning') return 'bg-warning'
  return 'bg-muted-foreground/40'
}

interface DashboardStats {
  active_sessions: number
  tenants: number
  users: number
  skills: number
  clusters: number
  open_issues: number
}

const stats = ref<DashboardStats | null>(null)
const loading = ref(true)

const statCards = [
  {
    key: 'active_sessions' as keyof DashboardStats,
    labelKey: 'dashboard.activeSessions',
    icon: Activity,
    color: 'text-info',
    accent: 'bg-info',
    bgTint: 'bg-info/10',
  },
  {
    key: 'tenants' as keyof DashboardStats,
    labelKey: 'dashboard.totalTenants',
    icon: Building2,
    color: 'text-ai',
    accent: 'bg-ai',
    bgTint: 'bg-ai/10',
  },
  {
    key: 'users' as keyof DashboardStats,
    labelKey: 'dashboard.totalUsers',
    icon: Users,
    color: 'text-success',
    accent: 'bg-success',
    bgTint: 'bg-success/10',
  },
  {
    key: 'skills' as keyof DashboardStats,
    labelKey: 'dashboard.totalSkills',
    icon: Zap,
    color: 'text-primary',
    accent: 'bg-primary',
    bgTint: 'bg-primary/10',
  },
  {
    key: 'clusters' as keyof DashboardStats,
    labelKey: 'dashboard.totalClusters',
    icon: Server,
    color: 'text-primary',
    accent: 'bg-primary',
    bgTint: 'bg-primary/10',
  },
  {
    key: 'open_issues' as keyof DashboardStats,
    labelKey: 'dashboard.openIssues',
    icon: AlertTriangle,
    color: 'text-destructive',
    accent: 'bg-destructive',
    bgTint: 'bg-destructive/10',
  },
]

const refreshing = ref(false)
let autoRefreshTimer: ReturnType<typeof setInterval> | null = null

async function fetchStats() {
  loading.value = true
  try {
    stats.value = await api.get<DashboardStats>('/api/dashboard/stats')
  } catch {
    stats.value = {
      active_sessions: 0,
      tenants: 0,
      users: 0,
      skills: 0,
      clusters: 0,
      open_issues: 0,
    }
  } finally {
    loading.value = false
  }
}

async function refresh() {
  refreshing.value = true
  try {
    stats.value = await api.get<DashboardStats>('/api/dashboard/stats')
  } catch { /* silent */ }
  refreshing.value = false
}

onMounted(() => {
  fetchStats()
  // Auto-refresh every 60s
  autoRefreshTimer = setInterval(refresh, 60_000)
})

onUnmounted(() => {
  if (autoRefreshTimer) clearInterval(autoRefreshTimer)
})
</script>

<template>
  <div class="space-y-5">
    <!-- Page title -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-base font-semibold text-foreground">
          {{ t('dashboard.welcome') }}, {{ authStore.user?.username }}
        </h1>
        <p class="text-xs text-muted-foreground mt-0.5">
          {{ t('app.description') }}
        </p>
      </div>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 px-2 text-muted-foreground hover:text-foreground"
        :disabled="refreshing"
        @click="refresh"
      >
        <RefreshCw class="h-3.5 w-3.5" :class="refreshing ? 'animate-spin' : ''" />
      </Button>
    </div>

    <!-- Systems online — live chips flow directly on page surface.
         No card wrap: just a small label + content. Mirrors the sidebar
         nav-group pattern (label above content, no box). -->
    <section class="space-y-4">
      <!-- Clusters -->
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-1.5">
            <Cloud class="h-3.5 w-3.5 text-primary/80" />
            <span class="text-[11px] font-semibold text-muted-foreground uppercase tracking-widest">
              {{ t('dashboard.liveClusters') }}
            </span>
          </div>
          <NuxtLink
            to="/clusters"
            class="inline-flex items-center gap-0.5 text-[11px] text-muted-foreground/70 hover:text-primary transition-colors"
          >
            {{ t('common.viewAll') }}
            <ChevronRight class="h-3 w-3" />
          </NuxtLink>
        </div>
        <div v-if="clustersLoading" class="flex flex-wrap gap-1.5">
          <Skeleton v-for="i in 5" :key="i" class="h-6 w-28 rounded" />
        </div>
        <p v-else-if="liveClusters.length === 0" class="text-[12px] text-muted-foreground/70 italic">
          {{ t('dashboard.noClustersYet') }}
        </p>
        <div v-else class="flex flex-wrap gap-x-4 gap-y-1.5">
          <NuxtLink
            v-for="c in liveClusters"
            :key="c.id"
            :to="`/clusters`"
            class="group inline-flex items-center gap-1.5 rounded px-1.5 py-1 text-[13px] text-foreground/90 transition-colors hover:text-foreground hover:bg-primary/8"
            :title="`${c.cloud}${c.region ? ' · ' + c.region : ''}`"
          >
            <span class="relative inline-flex h-2 w-2 shrink-0">
              <span :class="['absolute inset-0 rounded-full opacity-70 animate-pulse', clusterDotClass(c.status)]" />
              <span :class="['relative inline-block h-2 w-2 rounded-full', clusterDotClass(c.status)]" />
            </span>
            <span class="font-medium">{{ c.name }}</span>
            <span class="text-[11px] text-muted-foreground/60 font-mono uppercase">{{ c.cloud }}</span>
          </NuxtLink>
        </div>
      </div>

      <!-- Issues — compact inline list -->
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-1.5">
            <AlertTriangle class="h-3.5 w-3.5 text-destructive/80" />
            <span class="text-[11px] font-semibold text-muted-foreground uppercase tracking-widest">
              {{ t('dashboard.recentIssues') }}
            </span>
          </div>
          <NuxtLink
            to="/issues"
            class="inline-flex items-center gap-0.5 text-[11px] text-muted-foreground/70 hover:text-primary transition-colors"
          >
            {{ t('common.viewAll') }}
            <ChevronRight class="h-3 w-3" />
          </NuxtLink>
        </div>
        <div v-if="issuesLoading" class="space-y-1">
          <Skeleton v-for="i in 3" :key="i" class="h-7 w-full rounded" />
        </div>
        <p v-else-if="liveIssues.length === 0" class="text-[12px] text-success/80 italic">
          {{ t('dashboard.noIssuesHappy') }}
        </p>
        <ul v-else class="divide-y divide-border/30 border-y border-border/30">
          <li v-for="i in liveIssues" :key="i.id">
            <NuxtLink
              :to="`/issues`"
              class="flex items-center gap-3 px-1.5 py-2 text-[13px] transition-colors hover:bg-accent/20"
            >
              <span
                :class="[
                  'inline-flex h-5 items-center rounded px-1.5 text-[10px] font-mono font-semibold shrink-0',
                  sevBadgeVariant[i.severity] === 'destructive' ? 'bg-destructive/15 text-destructive'
                  : sevBadgeVariant[i.severity] === 'warning' ? 'bg-warning/15 text-warning'
                  : 'bg-info/15 text-info',
                ]"
              >
                {{ sevBadgeLabel[i.severity] || i.severity.toUpperCase() }}
              </span>
              <span class="flex-1 min-w-0 truncate text-foreground/90">{{ i.title }}</span>
              <span class="text-[11px] text-muted-foreground/60 font-mono uppercase shrink-0">{{ i.issue_type }}</span>
            </NuxtLink>
          </li>
        </ul>
      </div>
    </section>

    <!-- Stats Grid — Grafana panel style -->
    <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      <div
        v-for="card in statCards"
        :key="card.key"
        class="group rounded border border-border/60 bg-card overflow-hidden transition-all duration-150 hover:border-border"
      >
        <!-- Top color accent bar -->
        <div :class="[card.accent, 'h-0.5']" />

        <div class="px-4 py-3">
          <!-- Loading skeleton -->
          <template v-if="loading">
            <div class="flex items-center justify-between">
              <div class="space-y-2">
                <Skeleton class="h-3 w-20 rounded-sm" />
                <Skeleton class="h-7 w-12 rounded-sm" />
              </div>
              <Skeleton class="h-8 w-8 rounded" />
            </div>
          </template>

          <!-- Loaded state -->
          <template v-else>
            <div class="flex items-center justify-between">
              <div>
                <p class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                  {{ t(card.labelKey) }}
                </p>
                <p class="mt-1 text-2xl font-bold tracking-tight text-foreground">
                  {{ stats?.[card.key] ?? 0 }}
                </p>
              </div>
              <div
                :class="[card.bgTint, 'flex h-9 w-9 items-center justify-center rounded transition-transform duration-200 group-hover:scale-105']"
              >
                <component :is="card.icon" :class="['h-4.5 w-4.5', card.color]" />
              </div>
            </div>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>
