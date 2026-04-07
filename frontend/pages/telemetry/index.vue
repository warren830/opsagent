<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { toast } from 'vue-sonner'
import { Copy, Check } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import GrafanaForm from '@/components/telemetry/GrafanaForm.vue'
import DatadogForm from '@/components/telemetry/DatadogForm.vue'
import DynatraceForm from '@/components/telemetry/DynatraceForm.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const config = useRuntimeConfig()

const loading = ref(true)
const saving = ref(false)
const testing = ref(false)
const copied = ref(false)

// ─── Provider definitions ─────────────────────────────────────
type ProviderKey = 'grafana' | 'datadog' | 'dynatrace'

const providers: { value: ProviderKey; label: string }[] = [
  { value: 'grafana', label: 'Grafana' },
  { value: 'datadog', label: 'Datadog' },
  { value: 'dynatrace', label: 'Dynatrace' },
]

const activeProvider = ref<ProviderKey>('grafana')

// ─── Form state ───────────────────────────────────────────────
const hasExistingToken = ref(false)
const hasExistingDatadogKey = ref(false)
const hasExistingDtToken = ref(false)

const grafanaForm = ref({
  enabled: false,
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
  enabled: false,
  api_key: '',
  app_key: '',
  site: 'datadoghq.com',
})

const dynatraceForm = ref({
  enabled: false,
  environment_id: '',
  api_token: '',
  base_url: '',
})

// ─── Webhook URL ──────────────────────────────────────────────
const webhookBase = computed(() => config.public.apiBase || window.location.origin)

const webhookUrl = computed(() => {
  switch (activeProvider.value) {
    case 'datadog': return `${webhookBase.value}/api/alerts/datadog`
    case 'dynatrace': return `${webhookBase.value}/api/alerts/dynatrace`
    default: return `${webhookBase.value}/api/alerts`
  }
})

const webhookHint = computed(() => {
  switch (activeProvider.value) {
    case 'datadog': return t('telemetry.webhookHintDatadog')
    case 'dynatrace': return t('telemetry.webhookHintDynatrace')
    default: return t('telemetry.webhookHint')
  }
})

// ─── Fetch ────────────────────────────────────────────────────
async function fetchConfig() {
  loading.value = true
  try {
    const data = await api.get<{ provider: string; config: Record<string, string>; enabled: boolean } | null>('/api/telemetry')
    if (data?.config) {
      const c = data.config
      const p = data.provider

      if (p === 'datadog') {
        activeProvider.value = 'datadog'
        hasExistingDatadogKey.value = !!c.api_key
        datadogForm.value = {
          enabled: data.enabled ?? false,
          api_key: '',
          app_key: '',
          site: c.site || 'datadoghq.com',
        }
      } else if (p === 'dynatrace') {
        activeProvider.value = 'dynatrace'
        hasExistingDtToken.value = !!c.api_token
        dynatraceForm.value = {
          enabled: data.enabled ?? false,
          environment_id: c.environment_id || '',
          api_token: '',
          base_url: c.base_url || '',
        }
      } else {
        // grafana or self-hosted → both go under grafana provider
        activeProvider.value = 'grafana'
        grafanaForm.value.mode = p === 'self-hosted' ? 'self-hosted' : 'cloud'
        grafanaForm.value.enabled = data.enabled ?? false
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
    }
  } catch {
    // no config yet
  } finally {
    loading.value = false
  }
}

// ─── Save ─────────────────────────────────────────────────────
async function saveConfig() {
  saving.value = true
  try {
    let provider: string
    let configPayload: Record<string, string> = {}
    let enabled = false

    if (activeProvider.value === 'grafana') {
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
      enabled = grafanaForm.value.enabled
    } else if (activeProvider.value === 'datadog') {
      provider = 'datadog'
      configPayload = { site: datadogForm.value.site }
      if (datadogForm.value.api_key) configPayload.api_key = datadogForm.value.api_key
      if (datadogForm.value.app_key) configPayload.app_key = datadogForm.value.app_key
      enabled = datadogForm.value.enabled
    } else {
      provider = 'dynatrace'
      configPayload = {
        environment_id: dynatraceForm.value.environment_id,
        base_url: dynatraceForm.value.base_url,
      }
      if (dynatraceForm.value.api_token) configPayload.api_token = dynatraceForm.value.api_token
      enabled = dynatraceForm.value.enabled
    }

    await api.put('/api/telemetry', { provider, config: configPayload, enabled })

    // Clear secrets after save
    if (activeProvider.value === 'grafana' && grafanaForm.value.mode === 'cloud') {
      hasExistingToken.value = hasExistingToken.value || !!grafanaForm.value.api_token
      grafanaForm.value.api_token = ''
    } else if (activeProvider.value === 'datadog') {
      hasExistingDatadogKey.value = hasExistingDatadogKey.value || !!datadogForm.value.api_key
      datadogForm.value.api_key = ''
      datadogForm.value.app_key = ''
    } else if (activeProvider.value === 'dynatrace') {
      hasExistingDtToken.value = hasExistingDtToken.value || !!dynatraceForm.value.api_token
      dynatraceForm.value.api_token = ''
    }

    toast.success(t('common.success'))
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
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

async function copyWebhookUrl() {
  try {
    await navigator.clipboard.writeText(webhookUrl.value)
    copied.value = true
    toast.success(t('telemetry.copied'))
    setTimeout(() => { copied.value = false }, 2000)
  } catch {
    toast.error('Failed to copy')
  }
}

onMounted(() => { fetchConfig() })
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div>
      <h1 class="text-base font-semibold text-foreground">{{ t('telemetry.title') }}</h1>
      <p class="text-[11px] text-muted-foreground mt-0.5">
        Connect observability platforms. Alerts fire webhook → OpsAgent creates issue + runs RCA.
      </p>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="max-w-2xl space-y-3">
      <div class="rounded-lg border border-border/60 bg-card p-5">
        <div class="space-y-3">
          <div class="h-5 w-32 animate-pulse rounded bg-secondary/50" />
          <div class="h-8 w-full animate-pulse rounded bg-secondary/50" />
          <div class="h-8 w-full animate-pulse rounded bg-secondary/50" />
        </div>
      </div>
    </div>

    <div v-else class="max-w-2xl space-y-3">
      <!-- Provider Selector -->
      <div class="flex items-center gap-2">
        <label class="text-xs font-medium text-muted-foreground">{{ t('telemetry.provider') }}</label>
        <Select v-model="activeProvider">
          <SelectTrigger class="w-44">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="p in providers" :key="p.value" :value="p.value">
              {{ p.label }}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      <!-- Form Card -->
      <form class="space-y-3" @submit.prevent="saveConfig">
        <div class="rounded-lg border border-border/60 bg-card p-5 space-y-4">
          <!-- Dynamic form -->
          <GrafanaForm
            v-if="activeProvider === 'grafana'"
            v-model="grafanaForm"
            :has-existing-token="hasExistingToken"
          />
          <DatadogForm
            v-else-if="activeProvider === 'datadog'"
            v-model="datadogForm"
            :has-existing-api-key="hasExistingDatadogKey"
          />
          <DynatraceForm
            v-else-if="activeProvider === 'dynatrace'"
            v-model="dynatraceForm"
            :has-existing-token="hasExistingDtToken"
          />

          <!-- Incoming Webhook URL -->
          <div class="rounded-md border border-primary/20 bg-primary/5 p-3 space-y-1.5">
            <label class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{{ t('telemetry.incomingWebhook') }}</label>
            <div class="flex items-center gap-2">
              <code class="flex-1 text-xs text-primary font-mono break-all select-all">{{ webhookUrl }}</code>
              <Button type="button" variant="ghost" size="icon-sm" @click="copyWebhookUrl">
                <Check v-if="copied" class="h-3.5 w-3.5 text-green-500" />
                <Copy v-else class="h-3.5 w-3.5" />
              </Button>
            </div>
            <p class="text-[10px] text-muted-foreground/60">{{ webhookHint }}</p>
          </div>

          <!-- Actions -->
          <div class="flex items-center gap-2 pt-1">
            <Button type="submit" size="sm" :disabled="saving">
              {{ saving ? t('common.loading') : t('telemetry.save') }}
            </Button>
            <Button type="button" variant="outline" size="sm" :disabled="testing" @click="sendTestAlert">
              {{ testing ? t('common.loading') : t('telemetry.sendTestAlert') }}
            </Button>
          </div>
        </div>
      </form>
    </div>
  </div>
</template>
