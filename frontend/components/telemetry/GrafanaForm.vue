<script setup lang="ts">
import { Terminal } from 'lucide-vue-next'
import { Input } from '@/components/ui/input'

const { t } = useI18n()

const form = defineModel<{
  enabled: boolean
  mode: 'cloud' | 'self-hosted'
  // Cloud auth
  api_token: string
  // Signal sources (shared — both modes use Loki/Tempo/Mimir/Alloy)
  loki_user_id: string
  loki_endpoint_url: string
  tempo_user_id: string
  tempo_endpoint_url: string
  mimir_user_id: string
  mimir_endpoint_url: string
  alloy_endpoint: string
}>({ required: true })

defineProps<{
  hasExistingToken: boolean
}>()

const isCloud = computed(() => form.value.mode === 'cloud')
</script>

<template>
  <div class="space-y-4">
    <div>
      <h2 class="text-sm font-semibold text-foreground">Grafana</h2>
      <p class="text-[11px] text-muted-foreground">{{ t('telemetry.grafanaDesc') }}</p>
    </div>

    <!-- Mode toggle -->
    <div class="flex items-center gap-1 rounded-md border border-border/40 bg-background/50 p-0.5 w-fit">
      <button
        type="button"
        class="rounded px-2.5 py-1 text-[11px] font-medium transition-all duration-150"
        :class="isCloud
          ? 'bg-primary/15 text-primary shadow-sm'
          : 'text-muted-foreground hover:text-foreground'"
        @click="form.mode = 'cloud'"
      >
        {{ t('telemetry.grafanaCloud') }}
      </button>
      <button
        type="button"
        class="rounded px-2.5 py-1 text-[11px] font-medium transition-all duration-150"
        :class="!isCloud
          ? 'bg-primary/15 text-primary shadow-sm'
          : 'text-muted-foreground hover:text-foreground'"
        @click="form.mode = 'self-hosted'"
      >
        {{ t('telemetry.selfHosted') }}
      </button>
    </div>

    <!-- Cloud: API Token -->
    <div v-if="isCloud" class="space-y-1">
      <label class="text-xs font-medium">{{ t('telemetry.apiToken') }}</label>
      <Input
        v-model="form.api_token"
        type="password"
        :placeholder="hasExistingToken ? t('telemetry.saved') : 'glsa_...'"
      />
      <p class="text-[10px] text-muted-foreground/60">{{ t('telemetry.apiTokenHint') }}</p>
    </div>

    <!-- Helm install hint (both modes need Alloy; self-hosted also needs Mimir/Loki/Tempo) -->
    <div class="rounded-md border border-border/40 bg-background/50 p-3 space-y-1.5">
      <div class="flex items-center gap-1.5">
        <Terminal class="h-3.5 w-3.5 text-muted-foreground" />
        <span class="text-[10px] text-muted-foreground">
          {{ isCloud ? t('telemetry.helmInstallHintCloud') : t('telemetry.helmInstallHint') }}
        </span>
      </div>
      <code class="block text-xs text-success font-mono bg-black/30 rounded px-2.5 py-1.5 select-all">{{ t('telemetry.helmInstallCmd') }}</code>
    </div>

    <!-- Signal Sources (shared) -->
    <div class="space-y-3">
      <div class="flex items-baseline gap-2">
        <label class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{{ t('telemetry.signalSources') }}</label>
        <span class="text-[10px] text-muted-foreground/50">({{ t('telemetry.signalSourcesHint') }})</span>
      </div>

      <!-- Mimir -->
      <div class="rounded-md border border-border/40 bg-background/50 p-3 space-y-2">
        <div class="flex items-baseline gap-2">
          <span class="text-xs font-semibold text-foreground">{{ t('telemetry.mimir') }}</span>
          <span class="text-[10px] text-muted-foreground">{{ t('telemetry.mimirType') }}</span>
        </div>
        <div :class="isCloud ? 'grid grid-cols-[120px_1fr] gap-2' : ''">
          <div v-if="isCloud" class="space-y-0.5">
            <label class="text-[10px] text-muted-foreground">{{ t('telemetry.userId') }}</label>
            <Input v-model="form.mimir_user_id" placeholder="123456" class="font-mono" />
          </div>
          <div class="space-y-0.5">
            <label class="text-[10px] text-muted-foreground">{{ t('telemetry.endpointUrl') }}</label>
            <Input
              v-model="form.mimir_endpoint_url"
              :placeholder="isCloud ? 'https://prometheus-prod-...' : 'http://mimir.monitoring.svc:9009'"
              class="font-mono"
            />
          </div>
        </div>
      </div>

      <!-- Loki -->
      <div class="rounded-md border border-border/40 bg-background/50 p-3 space-y-2">
        <div class="flex items-baseline gap-2">
          <span class="text-xs font-semibold text-foreground">{{ t('telemetry.loki') }}</span>
          <span class="text-[10px] text-muted-foreground">{{ t('telemetry.lokiType') }}</span>
        </div>
        <div :class="isCloud ? 'grid grid-cols-[120px_1fr] gap-2' : ''">
          <div v-if="isCloud" class="space-y-0.5">
            <label class="text-[10px] text-muted-foreground">{{ t('telemetry.userId') }}</label>
            <Input v-model="form.loki_user_id" placeholder="123456" class="font-mono" />
          </div>
          <div class="space-y-0.5">
            <label class="text-[10px] text-muted-foreground">{{ t('telemetry.endpointUrl') }}</label>
            <Input
              v-model="form.loki_endpoint_url"
              :placeholder="isCloud ? 'https://logs-prod-...' : 'http://loki.monitoring.svc:3100'"
              class="font-mono"
            />
          </div>
        </div>
      </div>

      <!-- Tempo -->
      <div class="rounded-md border border-border/40 bg-background/50 p-3 space-y-2">
        <div class="flex items-baseline gap-2">
          <span class="text-xs font-semibold text-foreground">{{ t('telemetry.tempo') }}</span>
          <span class="text-[10px] text-muted-foreground">{{ t('telemetry.tempoType') }}</span>
        </div>
        <div :class="isCloud ? 'grid grid-cols-[120px_1fr] gap-2' : ''">
          <div v-if="isCloud" class="space-y-0.5">
            <label class="text-[10px] text-muted-foreground">{{ t('telemetry.userId') }}</label>
            <Input v-model="form.tempo_user_id" placeholder="123456" class="font-mono" />
          </div>
          <div class="space-y-0.5">
            <label class="text-[10px] text-muted-foreground">{{ t('telemetry.endpointUrl') }}</label>
            <Input
              v-model="form.tempo_endpoint_url"
              :placeholder="isCloud ? 'https://tempo-prod-...' : 'http://tempo.monitoring.svc:3200'"
              class="font-mono"
            />
          </div>
        </div>
      </div>

      <!-- Alloy -->
      <div class="rounded-md border border-border/40 bg-background/50 p-3 space-y-2">
        <div class="flex items-baseline gap-2">
          <span class="text-xs font-semibold text-foreground">{{ t('telemetry.alloy') }}</span>
          <span class="text-[10px] text-muted-foreground">{{ t('telemetry.alloyType') }}</span>
        </div>
        <div class="space-y-0.5">
          <label class="text-[10px] text-muted-foreground">{{ t('telemetry.endpointUrl') }}</label>
          <Input
            v-model="form.alloy_endpoint"
            :placeholder="isCloud ? 'https://alloy-...' : 'http://alloy.monitoring.svc:12345'"
            class="font-mono"
          />
        </div>
      </div>
    </div>
  </div>
</template>
