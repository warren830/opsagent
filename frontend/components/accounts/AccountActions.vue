<script setup lang="ts">
import { ref, computed } from 'vue'
import { Plus, RefreshCw } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'

const emit = defineEmits<{
  create: []
  synced: []
}>()

const { t } = useI18n()
const api = useApi()

const syncing = ref(false)
const lastSyncTime = ref<Date | null>(null)

interface SyncResult {
  added: number
  updated: number
  removed: number
}

const lastSyncLabel = computed(() => {
  if (!lastSyncTime.value) return ''
  const now = Date.now()
  const diff = now - lastSyncTime.value.getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return t('account.lastSync') + ': <1m'
  if (mins < 60) return `${t('account.lastSync')}: ${mins}m`
  const hours = Math.floor(mins / 60)
  return `${t('account.lastSync')}: ${hours}h${mins % 60}m`
})

async function syncAccounts() {
  syncing.value = true
  try {
    const result = await api.post<SyncResult>('/api/accounts/sync', {})
    lastSyncTime.value = new Date()
    const total = result.added + result.updated + result.removed
    if (total > 0) {
      toast.success(t('account.syncSuccess', { added: result.added, updated: result.updated, removed: result.removed }))
    } else {
      toast.info(t('account.syncEmpty'))
    }
    emit('synced')
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : 'Sync failed'
    toast.error(msg)
  } finally {
    syncing.value = false
  }
}
</script>

<template>
  <div class="flex items-center gap-2">
    <span v-if="lastSyncLabel" class="text-[10px] text-muted-foreground/60">{{ lastSyncLabel }}</span>
    <Button variant="outline" size="sm" :disabled="syncing" @click="syncAccounts">
      <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': syncing }" />
      {{ syncing ? t('account.syncing') : t('account.syncOrg') }}
    </Button>
    <Button size="sm" @click="$emit('create')">
      <Plus class="h-3.5 w-3.5" />
      {{ t('account.addAccount') }}
    </Button>
  </div>
</template>
