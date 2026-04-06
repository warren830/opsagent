<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Plus, Pencil, Trash2 } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from '@/components/ui/dialog'
import DataTable from '@/components/shared/DataTable.vue'
import ConfirmDialog from '@/components/shared/ConfirmDialog.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

interface Channel {
  id: string
  platform: 'slack' | 'feishu' | 'teams'
  name: string
  credentials: Record<string, unknown> | null
  enabled: boolean
  created_at: string
}

const channels = ref<Channel[]>([])
const loading = ref(true)
const saving = ref(false)

const showFormDialog = ref(false)
const editingChannel = ref<Channel | null>(null)
const showDeleteDialog = ref(false)
const deletingChannel = ref<Channel | null>(null)

const form = ref({
  platform: 'slack' as 'slack' | 'feishu' | 'teams',
  name: '',
  credentials: '',
  enabled: true,
})

const isEditing = computed(() => !!editingChannel.value)
const formTitle = computed(() => isEditing.value ? t('common.edit') : t('channel.addIntegration'))

const columns = computed(() => [
  { key: 'platform', label: t('channel.platform') },
  { key: 'name', label: t('tenant.name') },
  { key: 'enabled', label: t('channel.enabled') },
  { key: 'created_at', label: t('tenant.createdAt') },
])

function platformLabel(p: string): string {
  const map: Record<string, string> = { slack: t('channel.slack'), feishu: t('channel.feishu'), teams: t('channel.teams') }
  return map[p] || p
}

function formatDate(dateStr: string): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })
}

async function fetchChannels() {
  loading.value = true
  try {
    channels.value = await api.get<Channel[]>('/api/channels')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(() => { fetchChannels() })

function openCreate() {
  editingChannel.value = null
  form.value = { platform: 'slack', name: '', credentials: '', enabled: true }
  showFormDialog.value = true
}

function openEdit(channel: Channel) {
  editingChannel.value = channel
  form.value = {
    platform: channel.platform,
    name: channel.name,
    credentials: channel.credentials ? JSON.stringify(channel.credentials, null, 2) : '',
    enabled: channel.enabled,
  }
  showFormDialog.value = true
}

function openDelete(channel: Channel) {
  deletingChannel.value = channel
  showDeleteDialog.value = true
}

async function toggleEnabled(channel: Channel) {
  try {
    await api.put(`/api/channels/${channel.id}`, { ...channel, enabled: !channel.enabled })
    await fetchChannels()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function saveChannel() {
  saving.value = true
  try {
    let credObj = null
    if (form.value.credentials.trim()) {
      credObj = JSON.parse(form.value.credentials)
    }
    const payload = {
      platform: form.value.platform,
      name: form.value.name,
      credentials: credObj,
      enabled: form.value.enabled,
    }
    if (isEditing.value) {
      await api.put(`/api/channels/${editingChannel.value!.id}`, payload)
      toast.success(t('common.success'))
    } else {
      await api.post('/api/channels', payload)
      toast.success(t('common.success'))
    }
    showFormDialog.value = false
    await fetchChannels()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteChannel() {
  if (!deletingChannel.value) return
  try {
    await api.del(`/api/channels/${deletingChannel.value.id}`)
    toast.success(t('common.success'))
    showDeleteDialog.value = false
    deletingChannel.value = null
    await fetchChannels()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('channel.title') }}</h1>
      <Button size="sm" @click="openCreate">
        <Plus class="h-3.5 w-3.5" />
        {{ t('channel.addIntegration') }}
      </Button>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="channels" :loading="loading">
      <template #cell-platform="{ row }">
        <Badge variant="info">{{ platformLabel((row as Channel).platform) }}</Badge>
      </template>

      <template #cell-name="{ row }">
        <span class="font-medium text-foreground">{{ (row as Channel).name }}</span>
      </template>

      <template #cell-enabled="{ row }">
        <Switch :checked="(row as Channel).enabled" @update:checked="toggleEnabled(row as Channel)" />
      </template>

      <template #cell-created_at="{ row }">
        <span class="text-muted-foreground">{{ formatDate((row as Channel).created_at) }}</span>
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" @click="openEdit(row as Channel)">
          <Pencil class="h-3 w-3" />
        </Button>
        <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(row as Channel)">
          <Trash2 class="h-3 w-3" />
        </Button>
      </template>
    </DataTable>

    <!-- Create/Edit Dialog -->
    <Dialog :open="showFormDialog" @update:open="(val) => { showFormDialog = val }">
      <DialogContent class="max-w-sm">
        <DialogHeader>
          <DialogTitle>{{ formTitle }}</DialogTitle>
          <DialogDescription>{{ formTitle }}</DialogDescription>
        </DialogHeader>

        <form class="space-y-3" @submit.prevent="saveChannel">
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('channel.platform') }}</label>
            <select
              v-model="form.platform"
              class="flex h-8 w-full rounded border border-border/60 bg-secondary/50 px-2.5 py-1.5 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 transition-colors"
            >
              <option value="slack">{{ t('channel.slack') }}</option>
              <option value="feishu">{{ t('channel.feishu') }}</option>
              <option value="teams">{{ t('channel.teams') }}</option>
            </select>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('tenant.name') }}</label>
            <Input v-model="form.name" :placeholder="t('tenant.name')" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('channel.credentials') }} (JSON)</label>
            <Textarea v-model="form.credentials" placeholder='{"token": "..."}' class="font-mono min-h-[80px]" />
          </div>

          <div class="flex items-center justify-between rounded border border-border/60 px-3 py-2">
            <label class="text-xs font-medium">{{ t('channel.enabled') }}</label>
            <Switch v-model:checked="form.enabled" />
          </div>

          <DialogFooter class="gap-1.5 pt-1">
            <Button type="button" variant="outline" size="sm" @click="showFormDialog = false">{{ t('common.cancel') }}</Button>
            <Button type="submit" size="sm" :disabled="saving">
              {{ saving ? t('common.loading') : (isEditing ? t('common.save') : t('common.create')) }}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>

    <ConfirmDialog
      :open="showDeleteDialog"
      :title="t('common.delete')"
      :description="t('common.confirmDelete')"
      :confirm-text="t('common.delete')"
      variant="destructive"
      @confirm="deleteChannel"
      @cancel="showDeleteDialog = false; deletingChannel = null"
    />
  </div>
</template>
