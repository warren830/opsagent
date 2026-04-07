<script setup lang="ts">
import { Input } from '@/components/ui/input'

const { t } = useI18n()

const form = defineModel<{
  enabled: boolean
  environment_id: string
  api_token: string
  base_url: string
}>({ required: true })

defineProps<{
  hasExistingToken: boolean
}>()
</script>

<template>
  <div class="space-y-4">
    <div>
      <h2 class="text-sm font-semibold text-foreground">{{ t('telemetry.dynatrace') }}</h2>
      <p class="text-[11px] text-muted-foreground">{{ t('telemetry.dynatraceDesc') }}</p>
    </div>

    <!-- Environment ID -->
    <div class="space-y-1">
      <label class="text-xs font-medium">{{ t('telemetry.dtEnvironmentId') }}</label>
      <Input
        v-model="form.environment_id"
        placeholder="abc12345"
        class="font-mono"
      />
    </div>

    <!-- Base URL -->
    <div class="space-y-1">
      <label class="text-xs font-medium">{{ t('telemetry.dtBaseUrl') }}</label>
      <Input
        v-model="form.base_url"
        placeholder="https://abc12345.live.dynatrace.com"
        class="font-mono"
      />
      <p class="text-[10px] text-muted-foreground/60">{{ t('telemetry.dtBaseUrlHint') }}</p>
    </div>

    <!-- API Token -->
    <div class="space-y-1">
      <label class="text-xs font-medium">{{ t('telemetry.dtApiToken') }}</label>
      <Input
        v-model="form.api_token"
        type="password"
        :placeholder="hasExistingToken ? t('telemetry.saved') : 'dt0c01.xxx'"
      />
      <p class="text-[10px] text-muted-foreground/60">{{ t('telemetry.dtApiTokenHint') }}</p>
    </div>
  </div>
</template>
