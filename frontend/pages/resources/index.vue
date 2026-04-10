<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue'
import {
  ShieldCheck, ShieldAlert, DollarSign, Gauge, Activity, Wrench,
  Play, RefreshCw, Download, ChevronDown, ChevronRight,
  AlertTriangle, Info, CheckCircle2, Clock, Server, MessageSquare,
  ChevronsDownUp, ChevronsUpDown,
} from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

// ─── Types ─────────────────────────────────────────────────────────────────

interface DashboardStats {
  total_findings: number
  by_severity: { high: number; medium: number; low: number; info: number }
  by_category: { security: number; cost: number; reliability: number; performance: number; operations: number }
  by_service: { service: string; count: number }[]
  last_scan: Scan | null
}

interface Scan {
  id: string
  account_id: string | null
  account_name: string | null
  regions: string[]
  services: string[]
  status: string
  finding_count: number
  summary: Record<string, unknown>
  error_message: string | null
  report_path: string | null
  started_at: string | null
  completed_at: string | null
  created_at: string
}

interface Finding {
  id: string
  scan_id: string
  service: string
  check_id: string
  severity: string
  category: string
  short_desc: string
  description: string | null
  resource_arn: string | null
  resource_name: string | null
  region: string | null
  account_id: string | null
  compliant: boolean
  remediation: string | null
  detail: Record<string, unknown>
  created_at: string
}

interface CloudAccount {
  id: string
  account_id: string | null
  name: string
  provider: string
  regions: string[]
  is_mock: boolean
}

// ─── State ─────────────────────────────────────────────────────────────────

const dashboard = ref<DashboardStats | null>(null)
const findings = ref<Finding[]>([])
const scans = ref<Scan[]>([])
const accounts = ref<CloudAccount[]>([])
const loading = ref(true)
const scanning = ref(false)
const settingUp = ref(false)
const showHistory = ref(false)
const pollTimer = ref<ReturnType<typeof setInterval> | null>(null)
const screenerInstalled = ref(false)
const screenerChecked = ref(false)

// Row expand state
const expandedRows = ref<Set<string>>(new Set())
const allExpanded = computed(() => findings.value.length > 0 && expandedRows.value.size === findings.value.length)

function toggleRow(id: string) {
  const s = new Set(expandedRows.value)
  if (s.has(id)) s.delete(id)
  else s.add(id)
  expandedRows.value = s
}

function expandAll() {
  expandedRows.value = new Set(findings.value.map(f => f.id))
}

function collapseAll() {
  expandedRows.value = new Set()
}

// Filters
const filterSeverity = ref('__all__')
const filterCategory = ref('__all__')
const filterService = ref('__all__')
const filterRegion = ref('__all__')
const searchQuery = ref('')
const selectedAccountId = ref('__all__')
const selectedRegion = ref('__none__')
const selectedService = ref('__none__')
const globalServices = ref<string[]>([])
const chinaServices = ref<string[]>([])

const defaultRegions = [
  'us-east-1', 'us-east-2', 'us-west-1', 'us-west-2',
  'eu-west-1', 'eu-west-2', 'eu-central-1',
  'ap-northeast-1', 'ap-southeast-1', 'ap-southeast-2', 'ap-south-1',
  'cn-north-1', 'cn-northwest-1',
]

// Dynamic regions: from selected account if it has regions configured, otherwise defaults
const availableRegions = computed(() => {
  if (hasSelectedAccount.value) {
    const acct = awsAccounts.value.find(a => (a.account_id || a.id) === selectedAccountId.value)
    if (acct && acct.regions && acct.regions.length > 0) {
      return acct.regions
    }
  }
  return defaultRegions
})

// Dynamic services: switch list based on selected region (cn- → china, else → global)
const availableServices = computed(() => {
  if (hasSelectedRegion.value && selectedRegion.value.startsWith('cn-')) {
    return chinaServices.value
  }
  return globalServices.value
})

// ─── Computed ──────────────────────────────────────────────────────────────

const awsAccounts = computed(() => accounts.value.filter(a => a.provider === 'aws' && !a.is_mock))
const hasSelectedAccount = computed(() => selectedAccountId.value && selectedAccountId.value !== '__all__')
const hasSelectedRegion = computed(() => selectedRegion.value && selectedRegion.value !== '__none__')
const hasSelectedService = computed(() => selectedService.value && selectedService.value !== '__none__')
const canScan = computed(() => screenerInstalled.value && hasSelectedAccount.value && hasSelectedRegion.value && hasSelectedService.value)
const latestCompletedScan = computed(() => scans.value.find(s => s.status === 'completed'))
const canChat = computed(() => !!latestCompletedScan.value)

const sevColors: Record<string, string> = {
  H: 'text-rose-300 bg-gradient-to-br from-rose-950/80 to-purple-950/70 border-rose-700/30',
  M: 'text-amber-300 bg-gradient-to-br from-fuchsia-950/60 to-purple-950/50 border-fuchsia-700/25',
  L: 'text-violet-300 bg-gradient-to-br from-purple-950/50 to-indigo-950/40 border-purple-700/20',
  I: 'text-purple-300/70 bg-gradient-to-br from-indigo-950/40 to-slate-950/30 border-indigo-800/15',
}

const catIcons: Record<string, { icon: typeof ShieldCheck; color: string }> = {
  security: { icon: ShieldAlert, color: 'text-red-400' },
  cost: { icon: DollarSign, color: 'text-emerald-400' },
  reliability: { icon: Activity, color: 'text-blue-400' },
  performance: { icon: Gauge, color: 'text-purple-400' },
  operations: { icon: Wrench, color: 'text-amber-400' },
}

const uniqueServices = computed(() => {
  const svcs = new Set(findings.value.map(f => f.service))
  return Array.from(svcs).sort()
})

const uniqueRegions = computed(() => {
  const regs = new Set(findings.value.filter(f => f.region).map(f => f.region!))
  return Array.from(regs).sort()
})

const lastScanTime = computed(() => {
  if (!dashboard.value?.last_scan?.completed_at) return null
  return formatRelativeTime(dashboard.value.last_scan.completed_at)
})

// ─── Helpers ───────────────────────────────────────────────────────────────

function severityLabel(s: string): string {
  switch (s) {
    case 'H': return t('resource.high')
    case 'M': return t('resource.medium')
    case 'L': return t('resource.low')
    case 'I': return t('resource.info')
    default: return s
  }
}

function categoryLabel(c: string): string {
  switch (c) {
    case 'S': return t('resource.security')
    case 'C': return t('resource.cost')
    case 'R': return t('resource.reliability')
    case 'P': return t('resource.performance')
    case 'O': return t('resource.operations')
    default: return c
  }
}

function categoryColor(c: string): string {
  switch (c) {
    case 'S': return 'text-red-400'
    case 'C': return 'text-emerald-400'
    case 'R': return 'text-blue-400'
    case 'P': return 'text-purple-400'
    case 'O': return 'text-amber-400'
    default: return 'text-zinc-400'
  }
}

function severityVariant(s: string): 'destructive' | 'warning' | 'info' | 'secondary' {
  switch (s) {
    case 'H': return 'destructive'
    case 'M': return 'warning'
    case 'L': return 'info'
    default: return 'secondary'
  }
}

function scanStatusVariant(s: string): 'success' | 'warning' | 'destructive' | 'secondary' {
  switch (s) {
    case 'completed': return 'success'
    case 'running': return 'warning'
    case 'failed': return 'destructive'
    default: return 'secondary'
  }
}

function formatRelativeTime(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return 'just now'
  if (mins < 60) return `${mins}m ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  return `${days}d ago`
}

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, {
    year: 'numeric', month: 'short', day: 'numeric',
    hour: '2-digit', minute: '2-digit',
  })
}

function formatDuration(start: string | null, end: string | null): string {
  if (!start || !end) return '-'
  const diff = new Date(end).getTime() - new Date(start).getTime()
  const secs = Math.floor(diff / 1000)
  if (secs < 60) return `${secs}s`
  const mins = Math.floor(secs / 60)
  const remainSecs = secs % 60
  return `${mins}m ${remainSecs}s`
}

// ─── API Calls ─────────────────────────────────────────────────────────────

async function fetchDashboard() {
  try {
    dashboard.value = await api.get<DashboardStats>('/api/resources/dashboard')
  } catch { /* silent */ }
}

async function fetchFindings() {
  loading.value = true
  try {
    const params = new URLSearchParams()
    if (filterSeverity.value && filterSeverity.value !== '__all__') params.set('severity', filterSeverity.value)
    if (filterCategory.value && filterCategory.value !== '__all__') params.set('category', filterCategory.value)
    if (filterService.value && filterService.value !== '__all__') params.set('service', filterService.value)
    if (filterRegion.value && filterRegion.value !== '__all__') params.set('region', filterRegion.value)
    if (searchQuery.value) params.set('q', searchQuery.value)
    const qs = params.toString()
    findings.value = await api.get<Finding[]>(`/api/resources/findings${qs ? '?' + qs : ''}`)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

async function fetchScans() {
  try {
    scans.value = await api.get<Scan[]>('/api/resources/scans')
  } catch { /* silent */ }
}

async function fetchAccounts() {
  try {
    accounts.value = await api.get<CloudAccount[]>('/api/accounts')
  } catch { /* silent */ }
}

async function startScan() {
  scanning.value = true
  try {
    const payload: Record<string, unknown> = {}
    if (selectedAccountId.value && selectedAccountId.value !== '__all__') {
      payload.account_id = selectedAccountId.value
    }
    if (hasSelectedRegion.value) {
      payload.regions = [selectedRegion.value]
    }
    if (hasSelectedService.value) {
      payload.services = [selectedService.value]
    }
    const scan = await api.post<Scan>('/api/resources/scan', payload)
    toast.success(t('resource.scanStarted'))

    // Poll for completion
    startPolling(scan.id)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
    scanning.value = false
  }
}

function startPolling(scanId: string) {
  if (pollTimer.value) clearInterval(pollTimer.value)
  pollTimer.value = setInterval(async () => {
    try {
      const scan = await api.get<Scan>(`/api/resources/scans/${scanId}`)
      if (scan.status === 'completed') {
        stopPolling()
        scanning.value = false
        toast.success(t('resource.scanComplete', { count: scan.finding_count }))
        await refreshAll()
      } else if (scan.status === 'failed') {
        stopPolling()
        scanning.value = false
        toast.error(scan.error_message || t('resource.scanFailed'))
        await fetchScans()
      }
    } catch {
      stopPolling()
      scanning.value = false
    }
  }, 3000)
}

function stopPolling() {
  if (pollTimer.value) {
    clearInterval(pollTimer.value)
    pollTimer.value = null
  }
}

async function checkScreener() {
  try {
    const result = await api.get<{ installed: boolean; globalServices?: string[]; chinaServices?: string[] }>('/api/resources/screener/status')
    screenerInstalled.value = result.installed
    if (result.globalServices) globalServices.value = result.globalServices
    if (result.chinaServices) chinaServices.value = result.chinaServices
  } catch { /* silent */ }
  screenerChecked.value = true
}

async function setupScreener() {
  settingUp.value = true
  try {
    const result = await api.post<{ status: string; errors?: string[] }>('/api/resources/screener/setup', {})
    if (result.status === 'ok') {
      toast.success(t('resource.screenerReady'))
      screenerInstalled.value = true
    } else {
      toast.warning(t('resource.screenerSetupFailed'))
      if (result.errors) {
        for (const e of result.errors.slice(0, 3)) {
          toast.warning(e, { duration: 8000 })
        }
      }
      // Still might be partially installed
      await checkScreener()
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    settingUp.value = false
  }
}

async function refreshAll() {
  await Promise.all([fetchDashboard(), fetchFindings(), fetchScans()])
}

function chatWithAgent() {
  const scan = latestCompletedScan.value
  if (!scan) return

  // Open the chat panel
  const chatOpen = useState('chatPanelOpen')
  chatOpen.value = true

  // Pre-fill the chat input with a context-rich prompt
  const chatPrefill = useState<string>('chatPrefill', () => '')
  const reportFile = scan.report_path ? `workspace/${scan.report_path}/api-full.json` : null

  const lines = [
    `Please analyze my latest security scan results.`,
    `Account: ${scan.account_name || scan.account_id || 'unknown'}`,
    `Regions: ${scan.regions.join(', ')}`,
    `Services: ${scan.services.join(', ')}`,
    `Findings: ${scan.finding_count} total`,
  ]
  if (reportFile) {
    lines.push(`Report file: ${reportFile}`)
    lines.push(`Read the report file and give me a prioritized summary of the most critical findings with remediation recommendations.`)
  } else {
    lines.push(`Give me a prioritized summary of the most critical findings with remediation recommendations.`)
  }
  chatPrefill.value = lines.join('\n')
}

// ─── Watchers ──────────────────────────────────────────────────────────────

let searchTimeout: ReturnType<typeof setTimeout> | null = null
watch(searchQuery, () => {
  if (searchTimeout) clearTimeout(searchTimeout)
  searchTimeout = setTimeout(() => fetchFindings(), 300)
})

watch([filterSeverity, filterCategory, filterService, filterRegion], () => {
  fetchFindings()
})

// Reset service selection when region changes and current service is not available
watch(selectedRegion, () => {
  if (selectedService.value !== '__none__' && !availableServices.value.includes(selectedService.value)) {
    selectedService.value = '__none__'
  }
})

// ─── Lifecycle ─────────────────────────────────────────────────────────────

onMounted(async () => {
  await Promise.all([fetchDashboard(), fetchFindings(), fetchScans(), fetchAccounts(), checkScreener()])
})
</script>

<template>
  <div class="space-y-5">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('resource.title') }}</h1>
    </div>

    <!-- ═══════ PROGRESSIVE 6-STEP SCAN SETUP ═══════ -->
    <div v-if="screenerChecked" class="rounded-lg border border-border/60 bg-card/30 p-2">
      <div class="stepper-flow flex flex-wrap gap-1.5">
        <!-- Step 1: Install Screener -->
        <div
          class="stepper-step flex items-center gap-2 rounded-md px-3 py-2 transition-all min-w-0"
          :class="screenerInstalled
            ? 'bg-primary/8 ring-1 ring-primary/15'
            : 'bg-gradient-to-r from-primary/15 to-primary/8 ring-1 ring-primary/25'"
        >
          <div
            class="h-5 w-5 rounded-full flex items-center justify-center text-[10px] font-bold shrink-0"
            :class="screenerInstalled
              ? 'bg-primary/20 text-primary'
              : 'bg-primary text-primary-foreground'"
          >
            <CheckCircle2 v-if="screenerInstalled" class="h-3.5 w-3.5" />
            <span v-else>1</span>
          </div>
          <span class="text-[11px] font-medium whitespace-nowrap" :class="screenerInstalled ? 'text-primary/80' : 'text-foreground'">
            {{ t('resource.setupScreener') }}
          </span>
          <Button
            v-if="!screenerInstalled"
            size="sm"
            class="shrink-0 h-6 px-2 text-[10px]"
            :disabled="settingUp"
            @click="setupScreener"
          >
            <Download class="h-3 w-3" />
            {{ settingUp ? t('common.loading') : t('resource.install') }}
          </Button>
        </div>

        <ChevronRight class="h-4 w-4 text-muted-foreground/30 shrink-0 self-center max-sm:hidden" />

        <!-- Step 2: Select Account -->
        <div
          class="stepper-step flex items-center gap-2 rounded-md px-3 py-2 transition-all"
          :class="!screenerInstalled
            ? 'opacity-30 pointer-events-none'
            : hasSelectedAccount
              ? 'bg-primary/8 ring-1 ring-primary/15'
              : 'bg-card/60 ring-1 ring-border/40'"
        >
          <div
            class="h-5 w-5 rounded-full flex items-center justify-center text-[10px] font-bold shrink-0"
            :class="!screenerInstalled
              ? 'bg-muted text-muted-foreground/40'
              : hasSelectedAccount
                ? 'bg-primary/20 text-primary'
                : 'bg-primary text-primary-foreground'"
          >
            <CheckCircle2 v-if="hasSelectedAccount && screenerInstalled" class="h-3.5 w-3.5" />
            <span v-else>2</span>
          </div>
          <Select v-model="selectedAccountId" :disabled="!screenerInstalled">
            <SelectTrigger class="h-6 text-[11px] min-w-[140px] max-w-[200px] border-0 bg-transparent px-1">
              <SelectValue :placeholder="t('resource.selectAccount')" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="__all__">-- {{ t('resource.selectAccount') }} --</SelectItem>
              <SelectItem v-for="acct in awsAccounts" :key="acct.id" :value="acct.account_id || acct.id">
                {{ acct.name }} ({{ acct.account_id }})
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <ChevronRight class="h-4 w-4 text-muted-foreground/30 shrink-0 self-center max-sm:hidden" />

        <!-- Step 3: Select Region -->
        <div
          class="stepper-step flex items-center gap-2 rounded-md px-3 py-2 transition-all"
          :class="!hasSelectedAccount
            ? 'opacity-30 pointer-events-none'
            : hasSelectedRegion
              ? 'bg-primary/8 ring-1 ring-primary/15'
              : 'bg-card/60 ring-1 ring-border/40'"
        >
          <div
            class="h-5 w-5 rounded-full flex items-center justify-center text-[10px] font-bold shrink-0"
            :class="!hasSelectedAccount
              ? 'bg-muted text-muted-foreground/40'
              : hasSelectedRegion
                ? 'bg-primary/20 text-primary'
                : 'bg-primary text-primary-foreground'"
          >
            <CheckCircle2 v-if="hasSelectedRegion && hasSelectedAccount" class="h-3.5 w-3.5" />
            <span v-else>3</span>
          </div>
          <Select v-model="selectedRegion" :disabled="!hasSelectedAccount">
            <SelectTrigger class="h-6 text-[11px] min-w-[120px] max-w-[170px] border-0 bg-transparent px-1">
              <SelectValue :placeholder="t('resource.selectRegion')" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="__none__">-- {{ t('resource.selectRegion') }} --</SelectItem>
              <SelectItem v-for="r in availableRegions" :key="r" :value="r">{{ r }}</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <ChevronRight class="h-4 w-4 text-muted-foreground/30 shrink-0 self-center max-sm:hidden" />

        <!-- Step 4: Select Service -->
        <div
          class="stepper-step flex items-center gap-2 rounded-md px-3 py-2 transition-all"
          :class="!hasSelectedRegion
            ? 'opacity-30 pointer-events-none'
            : hasSelectedService
              ? 'bg-primary/8 ring-1 ring-primary/15'
              : 'bg-card/60 ring-1 ring-border/40'"
        >
          <div
            class="h-5 w-5 rounded-full flex items-center justify-center text-[10px] font-bold shrink-0"
            :class="!hasSelectedRegion
              ? 'bg-muted text-muted-foreground/40'
              : hasSelectedService
                ? 'bg-primary/20 text-primary'
                : 'bg-primary text-primary-foreground'"
          >
            <CheckCircle2 v-if="hasSelectedService && hasSelectedRegion" class="h-3.5 w-3.5" />
            <span v-else>4</span>
          </div>
          <Select v-model="selectedService" :disabled="!hasSelectedRegion">
            <SelectTrigger class="h-6 text-[11px] min-w-[120px] max-w-[170px] border-0 bg-transparent px-1">
              <SelectValue :placeholder="t('resource.selectService')" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="__none__">-- {{ t('resource.selectService') }} --</SelectItem>
              <SelectItem v-for="s in availableServices" :key="s" :value="s">{{ s }}</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <ChevronRight class="h-4 w-4 text-muted-foreground/30 shrink-0 self-center max-sm:hidden" />

        <!-- Step 5: Run Scan -->
        <div
          class="stepper-step flex items-center gap-2 rounded-md px-3 py-2 transition-all"
          :class="canScan
            ? 'bg-card/60 ring-1 ring-border/40'
            : 'opacity-30 pointer-events-none'"
        >
          <div
            class="h-5 w-5 rounded-full flex items-center justify-center text-[10px] font-bold shrink-0"
            :class="canScan
              ? 'bg-primary text-primary-foreground'
              : 'bg-muted text-muted-foreground/40'"
          >
            <span>5</span>
          </div>
          <Button
            size="sm"
            class="shrink-0 h-6 px-2.5 text-[10px]"
            :disabled="!canScan || scanning"
            @click="startScan"
          >
            <Play v-if="!scanning" class="h-3 w-3" />
            <RefreshCw v-else class="h-3 w-3 animate-spin" />
            {{ scanning ? t('resource.scanning') : t('resource.scan') }}
          </Button>
        </div>

        <ChevronRight class="h-4 w-4 text-muted-foreground/30 shrink-0 self-center max-sm:hidden" />

        <!-- Step 6: Chat with Agent -->
        <div
          class="stepper-step flex items-center gap-2 rounded-md px-3 py-2 transition-all"
          :class="canChat
            ? 'bg-primary/8 ring-1 ring-primary/15'
            : 'opacity-30 pointer-events-none'"
        >
          <div
            class="h-5 w-5 rounded-full flex items-center justify-center text-[10px] font-bold shrink-0"
            :class="canChat
              ? 'bg-primary/20 text-primary'
              : 'bg-muted text-muted-foreground/40'"
          >
            <span>6</span>
          </div>
          <Button
            size="sm"
            variant="outline"
            class="shrink-0 h-6 px-2.5 text-[10px] border-primary/30 text-primary hover:bg-primary/10"
            :disabled="!canChat"
            @click="chatWithAgent"
          >
            <MessageSquare class="h-3 w-3" />
            {{ t('resource.analyze') }}
            <span v-if="canChat" class="text-muted-foreground/60 ml-0.5">({{ latestCompletedScan?.finding_count }})</span>
          </Button>
        </div>
      </div>
    </div>

    <!-- ═══════ DASHBOARD STATS ═══════ -->
    <div v-if="dashboard" class="space-y-3">
      <!-- Severity Cards -->
      <div class="grid grid-cols-2 md:grid-cols-4 gap-2">
        <div
          v-for="(sev, key) in { H: dashboard.by_severity.high, M: dashboard.by_severity.medium, L: dashboard.by_severity.low, I: dashboard.by_severity.info }"
          :key="key"
          class="rounded-lg border px-3 py-2.5 transition-colors"
          :class="sevColors[key as string]"
        >
          <div class="flex items-center justify-between">
            <span class="text-[10px] font-semibold uppercase tracking-wider opacity-70">{{ severityLabel(key as string) }}</span>
            <AlertTriangle v-if="key === 'H'" class="h-3 w-3 opacity-50" />
            <ShieldAlert v-else-if="key === 'M'" class="h-3 w-3 opacity-50" />
            <Info v-else-if="key === 'I'" class="h-3 w-3 opacity-50" />
            <CheckCircle2 v-else class="h-3 w-3 opacity-50" />
          </div>
          <div class="text-2xl font-bold mt-0.5">{{ sev }}</div>
        </div>
      </div>

      <!-- Category Cards + Last Scan -->
      <div class="flex items-start gap-2">
        <div class="flex-1 grid grid-cols-5 gap-2">
          <div
            v-for="(cat, key) in {
              security: dashboard.by_category.security,
              cost: dashboard.by_category.cost,
              reliability: dashboard.by_category.reliability,
              performance: dashboard.by_category.performance,
              operations: dashboard.by_category.operations,
            }"
            :key="key"
            class="rounded-lg border border-border/50 bg-card/30 px-3 py-2 text-center"
          >
            <component :is="catIcons[key as string]?.icon" class="h-3.5 w-3.5 mx-auto mb-1" :class="catIcons[key as string]?.color" />
            <div class="text-lg font-bold text-foreground">{{ cat }}</div>
            <div class="text-[9px] text-muted-foreground/60 uppercase tracking-wider">{{ t(`resource.${key}`) }}</div>
          </div>
        </div>

        <!-- Last scan info -->
        <div v-if="dashboard.last_scan" class="shrink-0 rounded-lg border border-border/50 bg-card/30 px-3 py-2 w-[140px]">
          <div class="flex items-center gap-1 text-[10px] text-muted-foreground/60 mb-1">
            <Clock class="h-3 w-3" />
            {{ t('resource.lastScan') }}
          </div>
          <div class="text-xs font-medium text-foreground">{{ lastScanTime }}</div>
          <div class="text-[10px] text-muted-foreground/50 mt-0.5">{{ dashboard.last_scan.finding_count }} {{ t('resource.findings').toLowerCase() }}</div>
        </div>
      </div>

      <!-- Top Services Bar -->
      <div v-if="dashboard.by_service.length > 0" class="rounded-lg border border-border/40 bg-card/20 p-3">
        <div class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60 mb-2">{{ t('resource.findings') }} by {{ t('resource.service') }}</div>
        <div class="flex gap-1.5 flex-wrap">
          <div
            v-for="svc in dashboard.by_service"
            :key="svc.service"
            class="flex items-center gap-1 rounded-md bg-secondary/50 px-2 py-1"
          >
            <Server class="h-3 w-3 text-muted-foreground/50" />
            <span class="text-[11px] font-medium text-foreground">{{ svc.service }}</span>
            <Badge variant="secondary" class="h-4 min-w-4 justify-center text-[9px] px-1">{{ svc.count }}</Badge>
          </div>
        </div>
      </div>
    </div>

    <!-- ═══════ FINDINGS TABLE ═══════ -->
    <div class="space-y-2">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <ShieldCheck class="h-3.5 w-3.5 text-muted-foreground" />
          <span class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{{ t('resource.findings') }}</span>
          <Badge variant="secondary" class="h-4 min-w-4 justify-center text-[9px] px-1">{{ findings.length }}</Badge>
        </div>
        <!-- Expand All / Collapse All -->
        <div v-if="findings.length > 0" class="flex items-center gap-1">
          <Button
            v-if="!allExpanded"
            variant="ghost"
            size="sm"
            class="h-6 px-2 text-[10px] text-muted-foreground hover:text-foreground"
            @click="expandAll"
          >
            <ChevronsUpDown class="h-3 w-3 mr-1" />
            {{ t('resource.expandAll') }}
          </Button>
          <Button
            v-else
            variant="ghost"
            size="sm"
            class="h-6 px-2 text-[10px] text-muted-foreground hover:text-foreground"
            @click="collapseAll"
          >
            <ChevronsDownUp class="h-3 w-3 mr-1" />
            {{ t('resource.collapseAll') }}
          </Button>
        </div>
      </div>

      <!-- Filters Row -->
      <div class="flex items-center gap-1.5 flex-wrap">
        <Input
          v-model="searchQuery"
          :placeholder="t('common.search')"
          class="max-w-[180px] h-7 text-xs"
        />
        <Select v-model="filterSeverity">
          <SelectTrigger class="w-[110px] h-7 text-xs">
            <SelectValue :placeholder="t('resource.severity')" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{{ t('resource.severity') }}</SelectItem>
            <SelectItem value="H">{{ t('resource.high') }}</SelectItem>
            <SelectItem value="M">{{ t('resource.medium') }}</SelectItem>
            <SelectItem value="L">{{ t('resource.low') }}</SelectItem>
            <SelectItem value="I">{{ t('resource.info') }}</SelectItem>
          </SelectContent>
        </Select>
        <Select v-model="filterCategory">
          <SelectTrigger class="w-[120px] h-7 text-xs">
            <SelectValue :placeholder="t('resource.category')" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{{ t('resource.category') }}</SelectItem>
            <SelectItem value="S">{{ t('resource.security') }}</SelectItem>
            <SelectItem value="C">{{ t('resource.cost') }}</SelectItem>
            <SelectItem value="R">{{ t('resource.reliability') }}</SelectItem>
            <SelectItem value="P">{{ t('resource.performance') }}</SelectItem>
            <SelectItem value="O">{{ t('resource.operations') }}</SelectItem>
          </SelectContent>
        </Select>
        <Select v-model="filterService">
          <SelectTrigger class="w-[110px] h-7 text-xs">
            <SelectValue :placeholder="t('resource.service')" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{{ t('resource.service') }}</SelectItem>
            <SelectItem v-for="svc in uniqueServices" :key="svc" :value="svc">{{ svc }}</SelectItem>
          </SelectContent>
        </Select>
        <Select v-model="filterRegion">
          <SelectTrigger class="w-[130px] h-7 text-xs">
            <SelectValue :placeholder="t('resource.region')" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{{ t('resource.region') }}</SelectItem>
            <SelectItem v-for="r in uniqueRegions" :key="r" :value="r">{{ r }}</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <!-- Findings Table with Expandable Rows -->
      <div v-if="findings.length > 0 || loading" class="rounded border border-border/60 bg-card overflow-hidden">
        <div class="overflow-x-auto">
          <table class="w-full text-xs">
            <thead>
              <tr class="border-b border-border/60 bg-secondary/30">
                <th class="h-9 w-8 px-2 text-center align-middle"></th>
                <th class="h-9 px-3 text-left align-middle font-medium text-muted-foreground whitespace-nowrap uppercase tracking-wider text-[11px]">{{ t('resource.severity') }}</th>
                <th class="h-9 px-3 text-left align-middle font-medium text-muted-foreground whitespace-nowrap uppercase tracking-wider text-[11px]">{{ t('resource.service') }}</th>
                <th class="h-9 px-3 text-left align-middle font-medium text-muted-foreground whitespace-nowrap uppercase tracking-wider text-[11px]">{{ t('resource.check') }}</th>
                <th class="h-9 px-3 text-left align-middle font-medium text-muted-foreground whitespace-nowrap uppercase tracking-wider text-[11px]">{{ t('resource.resource') }}</th>
                <th class="h-9 px-3 text-left align-middle font-medium text-muted-foreground whitespace-nowrap uppercase tracking-wider text-[11px]">{{ t('resource.region') }}</th>
                <th class="h-9 px-3 text-left align-middle font-medium text-muted-foreground whitespace-nowrap uppercase tracking-wider text-[11px]">{{ t('resource.category') }}</th>
                <th class="h-9 px-3 text-left align-middle font-medium text-muted-foreground whitespace-nowrap uppercase tracking-wider text-[11px]">{{ t('resource.scanDate') }}</th>
              </tr>
            </thead>
            <tbody>
              <!-- Loading skeleton -->
              <template v-if="loading">
                <tr v-for="i in 5" :key="`skel-${i}`" class="border-b border-border/40">
                  <td class="px-2 py-2"></td>
                  <td v-for="j in 7" :key="`skel-${i}-${j}`" class="px-3 py-2">
                    <div class="h-4 w-3/4 rounded-sm bg-secondary/50 animate-pulse" />
                  </td>
                </tr>
              </template>

              <!-- Data rows with expand -->
              <template v-else>
                <template v-for="f in findings" :key="f.id">
                  <tr
                    class="border-b border-border/40 transition-colors hover:bg-accent/50 cursor-pointer group"
                    :class="expandedRows.has(f.id) ? 'bg-accent/30' : ''"
                    @click="toggleRow(f.id)"
                  >
                    <!-- Expand chevron -->
                    <td class="px-2 py-2 text-center align-middle">
                      <ChevronDown
                        class="h-3 w-3 text-muted-foreground/50 transition-transform duration-150 mx-auto"
                        :class="expandedRows.has(f.id) ? 'rotate-0' : '-rotate-90'"
                      />
                    </td>
                    <td class="px-3 py-2 align-middle">
                      <Badge :variant="severityVariant(f.severity)" class="text-[9px] font-bold">{{ severityLabel(f.severity) }}</Badge>
                    </td>
                    <td class="px-3 py-2 align-middle">
                      <code class="rounded-sm bg-secondary px-1.5 py-0.5 text-[11px] font-mono text-muted-foreground">{{ f.service }}</code>
                    </td>
                    <td class="px-3 py-2 align-middle">
                      <div class="max-w-[300px]">
                        <span class="font-medium text-foreground text-xs">{{ f.short_desc }}</span>
                      </div>
                    </td>
                    <td class="px-3 py-2 align-middle">
                      <span class="text-[11px] font-mono text-muted-foreground truncate max-w-[200px] block">{{ f.resource_name || '-' }}</span>
                    </td>
                    <td class="px-3 py-2 align-middle">
                      <span class="text-muted-foreground text-xs">{{ f.region || '-' }}</span>
                    </td>
                    <td class="px-3 py-2 align-middle">
                      <span class="text-xs font-medium" :class="categoryColor(f.category)">{{ categoryLabel(f.category) }}</span>
                    </td>
                    <td class="px-3 py-2 align-middle">
                      <span class="text-[10px] text-muted-foreground/60 whitespace-nowrap">{{ formatDate(f.created_at) }}</span>
                    </td>
                  </tr>

                  <!-- Expanded detail row -->
                  <tr v-if="expandedRows.has(f.id)" :key="`detail-${f.id}`" class="bg-accent/20">
                    <td colspan="8" class="px-4 py-3">
                      <div class="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs">
                        <!-- Description -->
                        <div v-if="f.description">
                          <div class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60 mb-1">{{ t('resource.description') }}</div>
                          <p class="text-foreground/80 leading-relaxed">{{ f.description }}</p>
                        </div>
                        <!-- Remediation -->
                        <div v-if="f.remediation">
                          <div class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60 mb-1">{{ t('resource.remediation') }}</div>
                          <p class="text-foreground/80 leading-relaxed">{{ f.remediation }}</p>
                        </div>
                        <!-- Resource ARN -->
                        <div v-if="f.resource_arn">
                          <div class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60 mb-1">ARN</div>
                          <code class="text-[10px] font-mono text-muted-foreground break-all bg-secondary/50 px-1.5 py-0.5 rounded-sm">{{ f.resource_arn }}</code>
                        </div>
                        <!-- Check ID -->
                        <div>
                          <div class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60 mb-1">Check ID</div>
                          <code class="text-[10px] font-mono text-muted-foreground bg-secondary/50 px-1.5 py-0.5 rounded-sm">{{ f.check_id }}</code>
                        </div>
                        <!-- Compliance -->
                        <div>
                          <div class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60 mb-1">{{ t('resource.compliance') }}</div>
                          <Badge :variant="f.compliant ? 'success' : 'destructive'" class="text-[9px]">
                            {{ f.compliant ? t('resource.compliant') : t('resource.nonCompliant') }}
                          </Badge>
                        </div>
                      </div>
                    </td>
                  </tr>
                </template>
              </template>
            </tbody>
          </table>
        </div>
      </div>

      <div v-if="!loading && findings.length === 0" class="rounded-lg border border-dashed border-border/50 py-10 text-center">
        <ShieldCheck class="h-8 w-8 text-muted-foreground/30 mx-auto mb-2" />
        <p class="text-xs text-muted-foreground/60">{{ t('resource.noFindings') }}</p>
      </div>
    </div>

    <!-- ═══════ SCAN HISTORY (Collapsible) ═══════ -->
    <Collapsible v-model:open="showHistory">
      <CollapsibleTrigger class="flex items-center gap-2 cursor-pointer group w-full">
        <ChevronRight v-if="!showHistory" class="h-3 w-3 text-muted-foreground transition-transform" />
        <ChevronDown v-else class="h-3 w-3 text-muted-foreground transition-transform" />
        <span class="text-xs font-semibold uppercase tracking-wider text-muted-foreground group-hover:text-foreground transition-colors">
          {{ t('resource.scanHistory') }}
        </span>
        <Badge variant="secondary" class="h-4 min-w-4 justify-center text-[9px] px-1">{{ scans.length }}</Badge>
      </CollapsibleTrigger>

      <CollapsibleContent class="mt-2">
        <div v-if="scans.length > 0" class="rounded-lg border border-border/40 divide-y divide-border/30">
          <div
            v-for="scan in scans"
            :key="scan.id"
            class="flex items-center gap-3 px-3 py-2"
          >
            <!-- Status badge -->
            <Badge :variant="scanStatusVariant(scan.status)" class="text-[9px] w-[72px] justify-center shrink-0">
              {{ scan.status }}
            </Badge>

            <!-- Account -->
            <div class="min-w-0 flex-1">
              <div class="text-xs font-medium text-foreground truncate">{{ scan.account_name || scan.account_id || 'All accounts' }}</div>
              <div class="text-[10px] text-muted-foreground/50">
                {{ scan.regions.length > 0 ? scan.regions.join(', ') : 'All regions' }}
              </div>
            </div>

            <!-- Findings count -->
            <div class="text-right shrink-0">
              <div class="text-sm font-bold text-foreground">{{ scan.finding_count }}</div>
              <div class="text-[9px] text-muted-foreground/50">{{ t('resource.findings').toLowerCase() }}</div>
            </div>

            <!-- Duration -->
            <div class="text-right shrink-0 w-[60px]">
              <div class="text-[10px] text-muted-foreground/60">{{ formatDuration(scan.started_at, scan.completed_at) }}</div>
            </div>

            <!-- Time -->
            <div class="text-right shrink-0 w-[100px]">
              <div class="text-[10px] text-muted-foreground/50">{{ formatDate(scan.created_at) }}</div>
            </div>

            <!-- Download report -->
            <a
              v-if="scan.report_path && scan.status === 'completed'"
              :href="`/api/chat/workspace/${scan.report_path}/output.zip`"
              target="_blank"
              class="shrink-0"
              :title="t('resource.downloadReport')"
            >
              <Download class="h-3 w-3 text-blue-400 hover:text-blue-300 transition-colors" />
            </a>

            <!-- Error indicator -->
            <AlertTriangle
              v-if="scan.error_message"
              class="h-3 w-3 text-amber-400 shrink-0"
              :title="scan.error_message"
            />
          </div>
        </div>

        <div v-else class="rounded-lg border border-dashed border-border/50 py-6 text-center">
          <p class="text-xs text-muted-foreground/60">{{ t('resource.noScans') }}</p>
        </div>
      </CollapsibleContent>
    </Collapsible>
  </div>
</template>

<style scoped>
/* Stepper flow: flex-wrap steps with smooth transitions */
.stepper-step {
  flex-shrink: 0;
}
</style>
