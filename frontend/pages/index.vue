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
} from 'lucide-vue-next'
import { Skeleton } from '@/components/ui/skeleton'
import { Button } from '@/components/ui/button'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const authStore = useAuthStore()
const api = useApi()

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
    color: 'text-violet-400',
    accent: 'bg-violet-400',
    bgTint: 'bg-violet-400/10',
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
    color: 'text-cyan-400',
    accent: 'bg-cyan-400',
    bgTint: 'bg-cyan-400/10',
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
