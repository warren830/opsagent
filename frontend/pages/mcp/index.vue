<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Pencil } from 'lucide-vue-next'
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

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

interface McpServer {
  id: string
  name: string
  server_type: string
  command: string
  args: string[] | null
  env: Record<string, string> | null
  enabled: boolean
}

const servers = ref<McpServer[]>([])
const loading = ref(true)
const saving = ref(false)

const showEditDialog = ref(false)
const editingServer = ref<McpServer | null>(null)

const form = ref({
  name: '',
  command: '',
  args: '',
  env: '',
  enabled: true,
})

const columns = computed(() => [
  { key: 'name', label: t('mcp.name') },
  { key: 'server_type', label: t('mcp.type') },
  { key: 'command', label: t('mcp.command') },
  { key: 'enabled', label: t('mcp.enabled') },
])

async function fetchServers() {
  loading.value = true
  try {
    servers.value = await api.get<McpServer[]>('/api/mcp')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(() => { fetchServers() })

function openEdit(server: McpServer) {
  editingServer.value = server
  form.value = {
    name: server.name,
    command: server.command,
    args: (server.args || []).join('\n'),
    env: server.env ? JSON.stringify(server.env, null, 2) : '',
    enabled: server.enabled,
  }
  showEditDialog.value = true
}

async function toggleEnabled(server: McpServer) {
  try {
    await api.put(`/api/mcp/${server.id}`, { ...server, enabled: !server.enabled })
    await fetchServers()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function saveServer() {
  if (!editingServer.value) return
  saving.value = true
  try {
    let envObj = null
    if (form.value.env.trim()) {
      envObj = JSON.parse(form.value.env)
    }
    const payload = {
      name: form.value.name,
      command: form.value.command,
      args: form.value.args.split('\n').map((s) => s.trim()).filter(Boolean),
      env: envObj,
      enabled: form.value.enabled,
    }
    await api.put(`/api/mcp/${editingServer.value.id}`, payload)
    toast.success(t('common.success'))
    showEditDialog.value = false
    await fetchServers()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('mcp.title') }}</h1>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="servers" :loading="loading">
      <template #cell-name="{ row }">
        <span class="font-medium text-foreground">{{ (row as McpServer).name }}</span>
      </template>

      <template #cell-server_type="{ row }">
        <Badge variant="secondary">{{ (row as McpServer).server_type }}</Badge>
      </template>

      <template #cell-command="{ row }">
        <code class="rounded-sm bg-secondary px-1.5 py-0.5 text-[11px] font-mono text-muted-foreground">{{ (row as McpServer).command }}</code>
      </template>

      <template #cell-enabled="{ row }">
        <Switch :checked="(row as McpServer).enabled" @update:checked="toggleEnabled(row as McpServer)" />
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" @click="openEdit(row as McpServer)">
          <Pencil class="h-3 w-3" />
        </Button>
      </template>
    </DataTable>

    <!-- Edit Dialog -->
    <Dialog :open="showEditDialog" @update:open="(val) => { showEditDialog = val }">
      <DialogContent class="max-w-sm">
        <DialogHeader>
          <DialogTitle>{{ t('common.edit') }}</DialogTitle>
          <DialogDescription>{{ t('mcp.title') }}</DialogDescription>
        </DialogHeader>

        <form class="space-y-3" @submit.prevent="saveServer">
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.name') }}</label>
            <Input v-model="form.name" :placeholder="t('mcp.name')" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.command') }}</label>
            <Input v-model="form.command" placeholder="npx" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.args') }}</label>
            <Textarea v-model="form.args" placeholder="One argument per line" class="font-mono min-h-[60px]" />
            <p class="text-[10px] text-muted-foreground">One argument per line</p>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.env') }} (JSON)</label>
            <Textarea v-model="form.env" placeholder='{"KEY": "value"}' class="font-mono min-h-[60px]" />
          </div>

          <div class="flex items-center justify-between rounded border border-border/60 px-3 py-2">
            <label class="text-xs font-medium">{{ t('mcp.enabled') }}</label>
            <Switch v-model:checked="form.enabled" />
          </div>

          <DialogFooter class="gap-1.5 pt-1">
            <Button type="button" variant="outline" size="sm" @click="showEditDialog = false">{{ t('common.cancel') }}</Button>
            <Button type="submit" size="sm" :disabled="saving">
              {{ saving ? t('common.loading') : t('common.save') }}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  </div>
</template>
