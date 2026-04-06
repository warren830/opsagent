<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

const loading = ref(true)
const saving = ref(false)
const testing = ref(false)

const form = ref({
  enabled: false,
  api_token: '',
  webhook_url: '',
  loki_user_id: '',
  loki_endpoint_url: '',
  tempo_user_id: '',
  tempo_endpoint_url: '',
  mimir_user_id: '',
  mimir_endpoint_url: '',
})

async function fetchConfig() {
  loading.value = true
  try {
    const data = await api.get<Record<string, unknown>>('/api/telemetry')
    if (data) {
      form.value = {
        enabled: (data.enabled as boolean) ?? false,
        api_token: (data.api_token as string) || '',
        webhook_url: (data.webhook_url as string) || '',
        loki_user_id: (data.loki_user_id as string) || '',
        loki_endpoint_url: (data.loki_endpoint_url as string) || '',
        tempo_user_id: (data.tempo_user_id as string) || '',
        tempo_endpoint_url: (data.tempo_endpoint_url as string) || '',
        mimir_user_id: (data.mimir_user_id as string) || '',
        mimir_endpoint_url: (data.mimir_endpoint_url as string) || '',
      }
    }
  } catch {
    // no config yet — keep defaults
  } finally {
    loading.value = false
  }
}

async function saveConfig() {
  saving.value = true
  try {
    await api.put('/api/telemetry', form.value)
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

onMounted(() => { fetchConfig() })
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('telemetry.title') }}</h1>
    </div>

    <div v-if="!loading" class="max-w-lg space-y-4">
      <form class="space-y-4" @submit.prevent="saveConfig">
        <!-- Grafana section -->
        <div class="rounded border border-border/60 bg-card p-4 space-y-3">
          <div class="flex items-center justify-between">
            <div>
              <h2 class="text-xs font-semibold text-foreground">{{ t('telemetry.grafana') }}</h2>
              <p class="text-[11px] text-muted-foreground">{{ t('telemetry.grafanaDesc') }}</p>
            </div>
            <Switch v-model:checked="form.enabled" />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('telemetry.apiToken') }}</label>
            <Input v-model="form.api_token" type="password" placeholder="glsa_..." />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('telemetry.webhookUrl') }}</label>
            <Input v-model="form.webhook_url" placeholder="https://..." />
          </div>
        </div>

        <!-- Signal Sources -->
        <div class="rounded border border-border/60 bg-card p-4 space-y-3">
          <h2 class="text-xs font-semibold text-foreground">{{ t('telemetry.signalSources') }}</h2>

          <!-- Loki -->
          <div class="space-y-1.5">
            <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('telemetry.loki') }}</label>
            <div class="grid grid-cols-2 gap-1.5">
              <div class="space-y-1">
                <label class="text-[11px] text-muted-foreground">{{ t('telemetry.userId') }}</label>
                <Input v-model="form.loki_user_id" placeholder="123456" />
              </div>
              <div class="space-y-1">
                <label class="text-[11px] text-muted-foreground">{{ t('telemetry.endpointUrl') }}</label>
                <Input v-model="form.loki_endpoint_url" placeholder="https://logs-..." />
              </div>
            </div>
          </div>

          <!-- Tempo -->
          <div class="space-y-1.5">
            <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('telemetry.tempo') }}</label>
            <div class="grid grid-cols-2 gap-1.5">
              <div class="space-y-1">
                <label class="text-[11px] text-muted-foreground">{{ t('telemetry.userId') }}</label>
                <Input v-model="form.tempo_user_id" placeholder="123456" />
              </div>
              <div class="space-y-1">
                <label class="text-[11px] text-muted-foreground">{{ t('telemetry.endpointUrl') }}</label>
                <Input v-model="form.tempo_endpoint_url" placeholder="https://tempo-..." />
              </div>
            </div>
          </div>

          <!-- Mimir -->
          <div class="space-y-1.5">
            <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('telemetry.mimir') }}</label>
            <div class="grid grid-cols-2 gap-1.5">
              <div class="space-y-1">
                <label class="text-[11px] text-muted-foreground">{{ t('telemetry.userId') }}</label>
                <Input v-model="form.mimir_user_id" placeholder="123456" />
              </div>
              <div class="space-y-1">
                <label class="text-[11px] text-muted-foreground">{{ t('telemetry.endpointUrl') }}</label>
                <Input v-model="form.mimir_endpoint_url" placeholder="https://mimir-..." />
              </div>
            </div>
          </div>
        </div>

        <!-- Actions -->
        <div class="flex items-center gap-1.5">
          <Button type="submit" size="sm" :disabled="saving">
            {{ saving ? t('common.loading') : t('telemetry.save') }}
          </Button>
          <Button type="button" variant="outline" size="sm" :disabled="testing" @click="sendTestAlert">
            {{ testing ? t('common.loading') : t('telemetry.sendTestAlert') }}
          </Button>
        </div>
      </form>
    </div>

    <!-- Loading -->
    <div v-else class="max-w-lg space-y-4">
      <div class="rounded border border-border/60 bg-card p-4 space-y-3">
        <div class="h-8 w-full animate-pulse rounded bg-secondary/50" />
        <div class="h-8 w-full animate-pulse rounded bg-secondary/50" />
        <div class="h-8 w-full animate-pulse rounded bg-secondary/50" />
      </div>
    </div>
  </div>
</template>
