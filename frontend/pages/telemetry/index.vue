<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { toast } from 'vue-sonner'
import { Plus, Pencil, Trash2, Copy, Check, Activity, FileText, GitBranch } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from '@/components/ui/dialog'
import ConfirmDialog from '@/components/shared/ConfirmDialog.vue'
import GrafanaForm from '@/components/telemetry/GrafanaForm.vue'
import DatadogForm from '@/components/telemetry/DatadogForm.vue'
import DynatraceForm from '@/components/telemetry/DynatraceForm.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const runtimeConfig = useRuntimeConfig()

// ─── Types ───────────────────────────────────────────────────
interface TelemetryConfig {
  id: string
  name: string
  provider: string
  config: Record<string, string>
  routing: { signals: string[]; scope: string }
  enabled: boolean
  tenant_id: string | null
  created_at: string
  updated_at: string
}

type ProviderKey = 'grafana' | 'datadog' | 'dynatrace'

const providers: { value: ProviderKey; label: string }[] = [
  { value: 'grafana', label: 'Grafana' },
  { value: 'datadog', label: 'Datadog' },
  { value: 'dynatrace', label: 'Dynatrace' },
]

// ─── State ───────────────────────────────────────────────────
const configs = ref<TelemetryConfig[]>([])
const loading = ref(true)
const saving = ref(false)
const testing = ref(false)
const copied = ref<string | null>(null)

const showFormDialog = ref(false)
const editingConfig = ref<TelemetryConfig | null>(null)
const showDeleteDialog = ref(false)
const deletingConfig = ref<TelemetryConfig | null>(null)

const isEditing = computed(() => !!editingConfig.value)
const formTitle = computed(() => isEditing.value ? t('telemetry.editConfig') : t('telemetry.addConfig'))

// ─── Form state ──────────────────────────────────────────────
const hasExistingToken = ref(false)
const hasExistingDatadogKey = ref(false)
const hasExistingDtToken = ref(false)

const formName = ref('')
const formProvider = ref<ProviderKey>('grafana')
const formEnabled = ref(true)
const formSignals = ref<string[]>(['metrics', 'logs', 'traces'])

const grafanaForm = ref({
  enabled: true,
  mode: 'cloud' as 'cloud' | 'self-hosted',
  api_token: '',
  loki_user_id: '',
  loki_endpoint_url: '',
  tempo_user_id: '',
  tempo_endpoint_url: '',
  mimir_user_id: '',
  mimir_endpoint_url: '',
  alloy_endpoint: '',
})

const datadogForm = ref({
  enabled: true,
  api_key: '',
  app_key: '',
  site: 'datadoghq.com',
})

const dynatraceForm = ref({
  enabled: true,
  environment_id: '',
  api_token: '',
  base_url: '',
})

// ─── Webhook URL ─────────────────────────────────────────────
const webhookBase = computed(() => runtimeConfig.public.apiBase || window.location.origin)

function getWebhookUrl(provider: string) {
  switch (provider) {
    case 'datadog': return `${webhookBase.value}/api/alerts/datadog`
    case 'dynatrace': return `${webhookBase.value}/api/alerts/dynatrace`
    default: return `${webhookBase.value}/api/alerts`
  }
}

// ─── Fetch ───────────────────────────────────────────────────
async function fetchConfigs() {
  loading.value = true
  try {
    configs.value = await api.get<TelemetryConfig[]>('/api/telemetry')
  } catch {
    // no configs yet
  } finally {
    loading.value = false
  }
}

onMounted(() => { fetchConfigs() })

// ─── Open create/edit ────────────────────────────────────────
function openCreate() {
  editingConfig.value = null
  formName.value = ''
  formProvider.value = 'grafana'
  formEnabled.value = true
  formSignals.value = ['metrics', 'logs', 'traces']
  hasExistingToken.value = false
  hasExistingDatadogKey.value = false
  hasExistingDtToken.value = false
  grafanaForm.value = { enabled: true, mode: 'cloud', api_token: '', loki_user_id: '', loki_endpoint_url: '', tempo_user_id: '', tempo_endpoint_url: '', mimir_user_id: '', mimir_endpoint_url: '', alloy_endpoint: '' }
  datadogForm.value = { enabled: true, api_key: '', app_key: '', site: 'datadoghq.com' }
  dynatraceForm.value = { enabled: true, environment_id: '', api_token: '', base_url: '' }
  showFormDialog.value = true
}

function openEdit(cfg: TelemetryConfig) {
  editingConfig.value = cfg
  formName.value = cfg.name
  formEnabled.value = cfg.enabled
  formSignals.value = [...(cfg.routing?.signals || ['metrics', 'logs', 'traces'])]

  const c = cfg.config || {}
  const p = cfg.provider

  if (p === 'datadog') {
    formProvider.value = 'datadog'
    hasExistingDatadogKey.value = !!c.api_key
    datadogForm.value = { enabled: cfg.enabled, api_key: '', app_key: '', site: c.site || 'datadoghq.com' }
  } else if (p === 'dynatrace') {
    formProvider.value = 'dynatrace'
    hasExistingDtToken.value = !!c.api_token
    dynatraceForm.value = { enabled: cfg.enabled, environment_id: c.environment_id || '', api_token: '', base_url: c.base_url || '' }
  } else {
    formProvider.value = 'grafana'
    grafanaForm.value.mode = p === 'self-hosted' ? 'self-hosted' : 'cloud'
    grafanaForm.value.enabled = cfg.enabled
    hasExistingToken.value = !!c.api_token
    grafanaForm.value.api_token = ''
    grafanaForm.value.loki_user_id = c.loki_user_id || ''
    grafanaForm.value.loki_endpoint_url = c.loki_endpoint_url || c.loki_endpoint || ''
    grafanaForm.value.tempo_user_id = c.tempo_user_id || ''
    grafanaForm.value.tempo_endpoint_url = c.tempo_endpoint_url || c.tempo_endpoint || ''
    grafanaForm.value.mimir_user_id = c.mimir_user_id || ''
    grafanaForm.value.mimir_endpoint_url = c.mimir_endpoint_url || c.mimir_endpoint || ''
    grafanaForm.value.alloy_endpoint = c.alloy_endpoint || ''
  }

  showFormDialog.value = true
}

function openDelete(cfg: TelemetryConfig) {
  deletingConfig.value = cfg
  showDeleteDialog.value = true
}

// ─── Save ────────────────────────────────────────────────────
async function saveConfig() {
  saving.value = true
  try {
    let provider: string
    let configPayload: Record<string, string> = {}

    if (formProvider.value === 'grafana') {
      provider = grafanaForm.value.mode === 'cloud' ? 'grafana' : 'self-hosted'
      configPayload = {
        loki_endpoint_url: grafanaForm.value.loki_endpoint_url,
        tempo_endpoint_url: grafanaForm.value.tempo_endpoint_url,
        mimir_endpoint_url: grafanaForm.value.mimir_endpoint_url,
        alloy_endpoint: grafanaForm.value.alloy_endpoint,
      }
      if (grafanaForm.value.mode === 'cloud') {
        configPayload.loki_user_id = grafanaForm.value.loki_user_id
        configPayload.tempo_user_id = grafanaForm.value.tempo_user_id
        configPayload.mimir_user_id = grafanaForm.value.mimir_user_id
        if (grafanaForm.value.api_token) configPayload.api_token = grafanaForm.value.api_token
      }
    } else if (formProvider.value === 'datadog') {
      provider = 'datadog'
      configPayload = { site: datadogForm.value.site }
      if (datadogForm.value.api_key) configPayload.api_key = datadogForm.value.api_key
      if (datadogForm.value.app_key) configPayload.app_key = datadogForm.value.app_key
    } else {
      provider = 'dynatrace'
      configPayload = {
        environment_id: dynatraceForm.value.environment_id,
        base_url: dynatraceForm.value.base_url,
      }
      if (dynatraceForm.value.api_token) configPayload.api_token = dynatraceForm.value.api_token
    }

    const payload = {
      name: formName.value,
      provider,
      config: configPayload,
      routing: { signals: formSignals.value, scope: 'all' },
      enabled: formEnabled.value,
    }

    if (isEditing.value) {
      await api.put(`/api/telemetry/${editingConfig.value!.id}`, payload)
    } else {
      await api.post('/api/telemetry', payload)
    }

    // Clear secrets after save
    if (formProvider.value === 'grafana' && grafanaForm.value.mode === 'cloud') {
      hasExistingToken.value = hasExistingToken.value || !!grafanaForm.value.api_token
      grafanaForm.value.api_token = ''
    } else if (formProvider.value === 'datadog') {
      hasExistingDatadogKey.value = hasExistingDatadogKey.value || !!datadogForm.value.api_key
      datadogForm.value.api_key = ''
      datadogForm.value.app_key = ''
    } else if (formProvider.value === 'dynatrace') {
      hasExistingDtToken.value = hasExistingDtToken.value || !!dynatraceForm.value.api_token
      dynatraceForm.value.api_token = ''
    }

    toast.success(t('common.success'))
    showFormDialog.value = false
    await fetchConfigs()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteConfig() {
  if (!deletingConfig.value) return
  try {
    await api.del(`/api/telemetry/${deletingConfig.value.id}`)
    toast.success(t('common.success'))
    showDeleteDialog.value = false
    deletingConfig.value = null
    await fetchConfigs()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function toggleEnabled(cfg: TelemetryConfig) {
  try {
    await api.put(`/api/telemetry/${cfg.id}`, { enabled: !cfg.enabled })
    await fetchConfigs()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function sendTestAlert() {
  testing.value = true
  try {
    await api.post('/api/telemetry/test')
    toast.success(t('common.success'))
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    testing.value = false
  }
}

async function copyWebhookUrl(url: string, id: string) {
  try {
    await navigator.clipboard.writeText(url)
    copied.value = id
    toast.success(t('telemetry.copied'))
    setTimeout(() => { copied.value = null }, 2000)
  } catch {
    toast.error('Failed to copy')
  }
}

function toggleSignal(signal: string) {
  const idx = formSignals.value.indexOf(signal)
  if (idx >= 0) {
    formSignals.value.splice(idx, 1)
  } else {
    formSignals.value.push(signal)
  }
}

function getProviderLabel(provider: string) {
  if (provider === 'self-hosted') return 'Self-Hosted'
  return providers.find(p => p.value === provider)?.label || provider
}

function getProviderColor(provider: string): 'default' | 'secondary' | 'outline' | 'destructive' {
  switch (provider) {
    case 'grafana': return 'default'
    case 'self-hosted': return 'secondary'
    case 'datadog': return 'secondary'
    case 'dynatrace': return 'outline'
    default: return 'outline'
  }
}

const signalIcon = { metrics: Activity, logs: FileText, traces: GitBranch }
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-base font-semibold text-foreground">{{ t('telemetry.title') }}</h1>
        <p class="text-[11px] text-muted-foreground mt-0.5">
          Connect observability platforms. Alerts fire webhook &rarr; OpsAgent creates issue + runs RCA.
        </p>
      </div>
      <Button size="sm" @click="openCreate">
        <Plus class="h-3.5 w-3.5" />
        {{ t('telemetry.addConfig') }}
      </Button>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="grid grid-cols-1 md:grid-cols-2 gap-3">
      <div v-for="i in 2" :key="i" class="rounded-lg border border-border/60 bg-card p-4">
        <div class="space-y-2">
          <div class="h-4 w-32 animate-pulse rounded bg-secondary/50" />
          <div class="h-3 w-48 animate-pulse rounded bg-secondary/50" />
          <div class="h-3 w-24 animate-pulse rounded bg-secondary/50" />
        </div>
      </div>
    </div>

    <!-- Empty State -->
    <div v-else-if="configs.length === 0" class="rounded-lg border border-dashed border-border/60 bg-card/50 p-8 text-center">
      <p class="text-xs text-muted-foreground">{{ t('telemetry.noConfigs') }}</p>
      <Button size="sm" class="mt-3" @click="openCreate">
        <Plus class="h-3.5 w-3.5" />
        {{ t('telemetry.addConfig') }}
      </Button>
    </div>

    <!-- Config Cards -->
    <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-3">
      <div
        v-for="cfg in configs"
        :key="cfg.id"
        class="group rounded-lg border border-border/60 bg-card p-4 hover:border-border transition-colors"
      >
        <!-- Header: name + provider badge -->
        <div class="flex items-center gap-2 mb-2">
          <span class="text-sm font-medium text-foreground truncate">{{ cfg.name }}</span>
          <Badge :variant="getProviderColor(cfg.provider)" class="text-[10px] shrink-0">
            {{ getProviderLabel(cfg.provider) }}
          </Badge>
        </div>

        <!-- Routing signals -->
        <div class="flex items-center gap-1.5 mb-2">
          <span class="text-[10px] text-muted-foreground/60 uppercase tracking-wider mr-1">{{ t('telemetry.routing') }}:</span>
          <Badge
            v-for="sig in (cfg.routing?.signals || [])"
            :key="sig"
            variant="outline"
            class="text-[10px] gap-1"
          >
            <component :is="signalIcon[sig as keyof typeof signalIcon]" class="h-2.5 w-2.5" />
            {{ sig }}
          </Badge>
        </div>

        <!-- Webhook URL (compact) -->
        <div class="flex items-center gap-1.5 mb-3">
          <code class="text-[10px] text-muted-foreground/50 font-mono truncate flex-1">{{ getWebhookUrl(cfg.provider) }}</code>
          <button
            class="shrink-0 p-0.5 rounded hover:bg-secondary/50 transition-colors"
            @click="copyWebhookUrl(getWebhookUrl(cfg.provider), cfg.id)"
          >
            <Check v-if="copied === cfg.id" class="h-3 w-3 text-green-500" />
            <Copy v-else class="h-3 w-3 text-muted-foreground/40" />
          </button>
        </div>

        <!-- Actions row -->
        <div class="flex items-center justify-between">
          <Switch :checked="cfg.enabled" size="sm" @update:checked="toggleEnabled(cfg)" />
          <div class="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
            <Button variant="ghost" size="icon-sm" @click="openEdit(cfg)">
              <Pencil class="h-3 w-3" />
            </Button>
            <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(cfg)">
              <Trash2 class="h-3 w-3" />
            </Button>
          </div>
        </div>
      </div>
    </div>

    <!-- Create/Edit Dialog -->
    <Dialog :open="showFormDialog" @update:open="(val) => { showFormDialog = val }">
      <DialogContent class="max-w-lg max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{{ formTitle }}</DialogTitle>
          <DialogDescription>{{ formTitle }}</DialogDescription>
        </DialogHeader>

        <form class="space-y-4" @submit.prevent="saveConfig">
          <!-- Config Name -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('telemetry.configName') }} <span class="text-destructive">*</span></label>
            <Input v-model="formName" :placeholder="t('telemetry.configNameHint')" required />
          </div>

          <!-- Provider Selector -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('telemetry.provider') }}</label>
            <Select v-model="formProvider">
              <SelectTrigger class="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem v-for="p in providers" :key="p.value" :value="p.value">
                  {{ p.label }}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          <!-- Provider-specific form -->
          <div class="rounded-lg border border-border/40 bg-background/50 p-4">
            <GrafanaForm
              v-if="formProvider === 'grafana'"
              v-model="grafanaForm"
              :has-existing-token="hasExistingToken"
            />
            <DatadogForm
              v-else-if="formProvider === 'datadog'"
              v-model="datadogForm"
              :has-existing-api-key="hasExistingDatadogKey"
            />
            <DynatraceForm
              v-else-if="formProvider === 'dynatrace'"
              v-model="dynatraceForm"
              :has-existing-token="hasExistingDtToken"
            />
          </div>

          <!-- Data Routing -->
          <div class="space-y-2">
            <span class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{{ t('telemetry.routing') }}</span>
            <div class="flex items-center gap-4">
              <div
                v-for="sig in ['metrics', 'logs', 'traces']"
                :key="sig"
                class="flex items-center gap-1.5 cursor-pointer select-none"
                @click.prevent="toggleSignal(sig)"
              >
                <div
                  class="h-4 w-4 shrink-0 rounded-sm border border-primary flex items-center justify-center transition-colors"
                  :class="formSignals.includes(sig) ? 'bg-primary text-primary-foreground' : ''"
                >
                  <Check v-if="formSignals.includes(sig)" class="h-3 w-3" />
                </div>
                <component :is="signalIcon[sig as keyof typeof signalIcon]" class="h-3 w-3 text-muted-foreground" />
                <span class="text-xs">{{ t(`telemetry.${sig}`) }}</span>
              </div>
            </div>
            <p class="text-[10px] text-muted-foreground/50">{{ t('telemetry.scope') }}: {{ t('telemetry.scopeAll') }}</p>
          </div>

          <!-- Enabled toggle -->
          <div class="flex items-center justify-between">
            <label class="text-xs font-medium">{{ t('telemetry.enabled') }}</label>
            <Switch v-model:checked="formEnabled" size="sm" />
          </div>

          <!-- Webhook URL display -->
          <div class="rounded-md border border-primary/20 bg-primary/5 p-3 space-y-1.5">
            <label class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{{ t('telemetry.incomingWebhook') }}</label>
            <div class="flex items-center gap-2">
              <code class="flex-1 text-xs text-primary font-mono break-all select-all">{{ getWebhookUrl(formProvider) }}</code>
              <Button type="button" variant="ghost" size="icon-sm" @click="copyWebhookUrl(getWebhookUrl(formProvider), 'form')">
                <Check v-if="copied === 'form'" class="h-3.5 w-3.5 text-green-500" />
                <Copy v-else class="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>

          <DialogFooter class="gap-1.5 pt-1">
            <Button type="button" variant="outline" size="sm" :disabled="testing" @click="sendTestAlert">
              {{ testing ? t('common.loading') : t('telemetry.sendTestAlert') }}
            </Button>
            <Button type="submit" size="sm" :disabled="saving">
              {{ saving ? t('common.loading') : (isEditing ? t('common.save') : t('telemetry.addConfig')) }}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>

    <ConfirmDialog
      :open="showDeleteDialog"
      :title="t('telemetry.deleteConfig')"
      :description="t('telemetry.confirmDeleteConfig')"
      :confirm-text="t('common.delete')"
      variant="destructive"
      @confirm="deleteConfig"
      @cancel="showDeleteDialog = false; deletingConfig = null"
    />
  </div>
</template>
