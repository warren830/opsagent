<script setup lang="ts">
import { Input } from '@/components/ui/input'

const { t } = useI18n()

const form = defineModel<{
  enabled: boolean
  api_key: string
  app_key: string
  site: string
}>({ required: true })

defineProps<{
  hasExistingApiKey: boolean
}>()
</script>

<template>
  <div class="space-y-4">
    <div>
      <h2 class="text-sm font-semibold text-foreground">{{ t('telemetry.datadog') }}</h2>
      <p class="text-[11px] text-muted-foreground">{{ t('telemetry.datadogDesc') }}</p>
    </div>

    <!-- API Key -->
    <div class="space-y-1">
      <label class="text-xs font-medium">{{ t('telemetry.datadogApiKey') }}</label>
      <Input
        v-model="form.api_key"
        type="password"
        :placeholder="hasExistingApiKey ? t('telemetry.saved') : ''"
      />
      <p class="text-[10px] text-muted-foreground/60">{{ t('telemetry.datadogApiKeyHint') }}</p>
    </div>

    <!-- App Key -->
    <div class="space-y-1">
      <label class="text-xs font-medium">{{ t('telemetry.datadogAppKey') }}</label>
      <Input
        v-model="form.app_key"
        type="password"
        placeholder=""
      />
      <p class="text-[10px] text-muted-foreground/60">{{ t('telemetry.datadogAppKeyHint') }}</p>
    </div>

    <!-- Site -->
    <div class="space-y-1">
      <label class="text-xs font-medium">{{ t('telemetry.datadogSite') }}</label>
      <Input
        v-model="form.site"
        placeholder="datadoghq.com"
        class="font-mono"
      />
      <p class="text-[10px] text-muted-foreground/60">{{ t('telemetry.datadogSiteHint') }}</p>
    </div>
  </div>
</template>
