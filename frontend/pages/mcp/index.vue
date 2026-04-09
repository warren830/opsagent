<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Plus, Pencil, Trash2, X, FlaskConical } from 'lucide-vue-next'
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
import ConfirmDialog from '@/components/shared/ConfirmDialog.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

interface McpServer {
  id: string
  name: string
  command: string
  args: string[] | null
  env: Record<string, string> | null
  enabled: boolean
  visibility: string
  user_id: string | null
  created_by: string | null
  transport_type: string
  url: string | null
  headers: Record<string, string> | null
  description: string | null
}

const servers = ref<McpServer[]>([])
const loading = ref(true)
const saving = ref(false)
const testing = ref(false)

const showFormDialog = ref(false)
const editingServer = ref<McpServer | null>(null)
const showDeleteDialog = ref(false)
const deletingServer = ref<McpServer | null>(null)

const isEditing = computed(() => !!editingServer.value)
const formTitle = computed(() => isEditing.value ? t('mcp.editServer') : t('mcp.addServer'))

const form = ref({
  name: '',
  transport_type: 'stdio' as 'stdio' | 'sse' | 'http',
  command: '',
  args: '',
  url: '',
  headers: [] as { key: string; value: string }[],
  envVars: [] as { key: string; value: string }[],
  description: '',
  scope: 'shared' as 'shared' | 'private',
  enabled: true,
})

const isStdio = computed(() => form.value.transport_type === 'stdio')

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

function openCreate() {
  editingServer.value = null
  form.value = {
    name: '',
    transport_type: 'stdio',
    command: '',
    args: '',
    url: '',
    headers: [],
    envVars: [],
    description: '',
    scope: 'shared',
    enabled: true,
  }
  showFormDialog.value = true
}

function openEdit(server: McpServer) {
  editingServer.value = server
  form.value = {
    name: server.name,
    transport_type: (server.transport_type === 'sse' ? 'http' : server.transport_type as 'stdio' | 'http') || 'stdio',
    command: server.command || '',
    args: (server.args || []).join('\n'),
    url: server.url || '',
    headers: server.headers
      ? Object.entries(server.headers).map(([key, value]) => ({ key, value }))
      : [],
    envVars: server.env
      ? Object.entries(server.env).map(([key, value]) => ({ key, value }))
      : [],
    description: server.description || '',
    scope: server.visibility === 'private' ? 'private' : 'shared',
    enabled: server.enabled,
  }
  showFormDialog.value = true
}

function openDelete(server: McpServer) {
  deletingServer.value = server
  showDeleteDialog.value = true
}

function addHeader() {
  form.value.headers.push({ key: '', value: '' })
}
function removeHeader(i: number) {
  form.value.headers.splice(i, 1)
}
function addEnvVar() {
  form.value.envVars.push({ key: '', value: '' })
}
function removeEnvVar(i: number) {
  form.value.envVars.splice(i, 1)
}

async function toggleEnabled(server: McpServer) {
  try {
    await api.put(`/api/mcp/${server.id}`, { enabled: !server.enabled })
    await fetchServers()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function saveServer() {
  saving.value = true
  try {
    const headersObj: Record<string, string> = {}
    for (const h of form.value.headers) {
      if (h.key.trim()) headersObj[h.key.trim()] = h.value
    }
    const envObj: Record<string, string> = {}
    for (const e of form.value.envVars) {
      if (e.key.trim()) envObj[e.key.trim()] = e.value
    }

    const payload = {
      name: form.value.name,
      transport_type: form.value.transport_type,
      command: form.value.command || '',
      args: form.value.args.split('\n').map(s => s.trim()).filter(Boolean),
      url: form.value.url || null,
      headers: headersObj,
      env: envObj,
      description: form.value.description || null,
      visibility: form.value.scope === 'private' ? 'private' : 'public',
      enabled: form.value.enabled,
    }

    if (isEditing.value) {
      await api.put(`/api/mcp/${editingServer.value!.id}`, payload)
    } else {
      await api.post('/api/mcp', payload)
    }
    toast.success(t('common.success'))
    showFormDialog.value = false
    await fetchServers()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteServer() {
  if (!deletingServer.value) return
  try {
    await api.del(`/api/mcp/${deletingServer.value.id}`)
    toast.success(t('common.success'))
    showDeleteDialog.value = false
    deletingServer.value = null
    await fetchServers()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function testServer() {
  testing.value = true
  try {
    const payload = {
      name: form.value.name,
      transport_type: form.value.transport_type,
      command: form.value.command || '',
      args: form.value.args.split('\n').map(s => s.trim()).filter(Boolean),
      url: form.value.url || null,
      headers: Object.fromEntries(form.value.headers.filter(h => h.key.trim()).map(h => [h.key.trim(), h.value])),
      env: Object.fromEntries(form.value.envVars.filter(e => e.key.trim()).map(e => [e.key.trim(), e.value])),
      server_id: editingServer.value?.id || null,
    }
    const res = await api.post<{ success: boolean; tools: { name: string; description: string }[]; error?: string; message?: string }>('/api/mcp/test', payload)
    if (res.success) {
      const toolCount = res.tools?.length || 0
      toast.success(res.message || `${t('mcp.testSuccess')} — ${toolCount} tools`)
      // Refresh server list to show updated tools
      if (editingServer.value?.id) await fetchServers()
    } else {
      toast.error(res.error || t('mcp.testFailed'))
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('mcp.testFailed')
    toast.error(msg)
  } finally {
    testing.value = false
  }
}

function getTransportColor(type: string) {
  switch (type) {
    case 'sse': return 'default'
    case 'http': return 'secondary'
    default: return 'outline'
  }
}
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-base font-semibold text-foreground">{{ t('mcp.title') }}</h1>
      </div>
      <Button size="sm" @click="openCreate">
        <Plus class="h-3.5 w-3.5" />
        {{ t('mcp.addServer') }}
      </Button>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
      <div v-for="i in 3" :key="i" class="rounded-lg border border-border/60 bg-card p-4">
        <div class="space-y-2">
          <div class="h-4 w-24 animate-pulse rounded bg-secondary/50" />
          <div class="h-3 w-40 animate-pulse rounded bg-secondary/50" />
        </div>
      </div>
    </div>

    <!-- Empty State -->
    <div v-else-if="servers.length === 0" class="rounded-lg border border-dashed border-border/60 bg-card/50 p-8 text-center">
      <p class="text-xs text-muted-foreground">{{ t('mcp.noServers') }}</p>
      <Button size="sm" class="mt-3" @click="openCreate">
        <Plus class="h-3.5 w-3.5" />
        {{ t('mcp.addServer') }}
      </Button>
    </div>

    <!-- Server Cards -->
    <div v-else class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
      <div
        v-for="s in servers"
        :key="s.id"
        class="group rounded-lg border border-border/60 bg-card p-4 hover:border-border transition-colors"
      >
        <!-- Name + badges -->
        <div class="flex items-center gap-2 mb-1.5">
          <span class="text-sm font-medium text-foreground truncate">{{ s.name }}</span>
          <Badge :variant="getTransportColor(s.transport_type)" class="text-[10px] shrink-0 uppercase">{{ s.transport_type }}</Badge>
          <Badge
            :variant="s.visibility === 'public' ? 'default' : 'secondary'"
            class="text-[10px] shrink-0"
          >
            {{ s.visibility === 'public' ? t('mcp.shared') : t('mcp.private') }}
          </Badge>
        </div>

        <!-- Command or URL -->
        <p class="text-xs text-muted-foreground font-mono truncate mb-1">
          {{ s.transport_type === 'stdio' ? s.command : s.url }}
        </p>

        <!-- Description -->
        <p v-if="s.description" class="text-[11px] text-muted-foreground/60 truncate mb-2">{{ s.description }}</p>
        <div v-else class="mb-2" />

        <!-- Actions row -->
        <div class="flex items-center justify-between">
          <Switch :checked="s.enabled" size="sm" @update:checked="toggleEnabled(s)" />
          <div class="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
            <Button variant="ghost" size="icon-sm" @click="openEdit(s)">
              <Pencil class="h-3 w-3" />
            </Button>
            <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(s)">
              <Trash2 class="h-3 w-3" />
            </Button>
          </div>
        </div>
      </div>
    </div>

    <!-- Create/Edit Dialog -->
    <Dialog :open="showFormDialog" @update:open="(val) => { showFormDialog = val }">
      <DialogContent class="max-w-md max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{{ formTitle }}</DialogTitle>
          <DialogDescription>{{ formTitle }}</DialogDescription>
        </DialogHeader>

        <form class="space-y-3" @submit.prevent="saveServer">
          <!-- Name -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.name') }} <span class="text-destructive">*</span></label>
            <Input v-model="form.name" :placeholder="t('mcp.namePlaceholder')" required />
          </div>

          <!-- Transport Type -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.transportType') }}</label>
            <div class="flex rounded-md border border-border/60 overflow-hidden">
              <button
                v-for="tp in ['stdio', 'http'] as const"
                :key="tp"
                type="button"
                class="flex-1 py-1.5 text-xs font-medium transition-colors"
                :class="form.transport_type === tp
                  ? 'bg-primary/10 text-primary border-primary/30'
                  : 'text-muted-foreground hover:bg-secondary/50'"
                @click="form.transport_type = tp"
              >
                {{ tp.toUpperCase() }}
              </button>
            </div>
          </div>

          <!-- STDIO: Command -->
          <div v-if="isStdio" class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.command') }} <span class="text-destructive">*</span></label>
            <Input v-model="form.command" placeholder="npx" :required="isStdio" />
          </div>

          <!-- STDIO: Args -->
          <div v-if="isStdio" class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.args') }}</label>
            <Textarea v-model="form.args" placeholder="One argument per line" class="font-mono min-h-[60px] text-xs" />
          </div>

          <!-- SSE/HTTP: URL -->
          <div v-if="!isStdio" class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.url') }} <span class="text-destructive">*</span></label>
            <Input v-model="form.url" :placeholder="t('mcp.urlPlaceholder')" :required="!isStdio" />
          </div>

          <!-- Headers (optional, key-value) -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.headers') }} <span class="text-muted-foreground/60 font-normal">({{ t('common.optional') || 'Optional' }})</span></label>
            <div v-for="(h, i) in form.headers" :key="i" class="flex gap-1.5 items-center">
              <Input v-model="h.key" placeholder="KEY" class="flex-1 font-mono text-xs" />
              <Input v-model="h.value" placeholder="value" class="flex-1 text-xs" />
              <Button type="button" variant="ghost" size="icon-sm" @click="removeHeader(i)">
                <X class="h-3 w-3" />
              </Button>
            </div>
            <button type="button" class="text-[11px] text-primary hover:underline" @click="addHeader">{{ t('mcp.addHeader') }}</button>
          </div>

          <!-- Env Vars (optional, key-value) -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.envVars') }} <span class="text-muted-foreground/60 font-normal">({{ t('common.optional') || 'Optional' }})</span></label>
            <div v-for="(e, i) in form.envVars" :key="i" class="flex gap-1.5 items-center">
              <Input v-model="e.key" placeholder="KEY" class="flex-1 font-mono text-xs" />
              <Input v-model="e.value" placeholder="value" class="flex-1 text-xs" />
              <Button type="button" variant="ghost" size="icon-sm" @click="removeEnvVar(i)">
                <X class="h-3 w-3" />
              </Button>
            </div>
            <button type="button" class="text-[11px] text-primary hover:underline" @click="addEnvVar">{{ t('mcp.addEnvVar') }}</button>
          </div>

          <!-- Description -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.description') }}</label>
            <Textarea v-model="form.description" :placeholder="t('mcp.descPlaceholder')" class="min-h-[60px] text-xs" />
          </div>

          <!-- Scope -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('mcp.scope') }}</label>
            <div class="flex rounded-md border border-border/60 overflow-hidden">
              <button
                type="button"
                class="flex-1 py-1.5 text-xs font-medium transition-colors"
                :class="form.scope === 'shared'
                  ? 'bg-primary/10 text-primary'
                  : 'text-muted-foreground hover:bg-secondary/50'"
                @click="form.scope = 'shared'"
              >
                {{ t('mcp.shared') }}
              </button>
              <button
                type="button"
                class="flex-1 py-1.5 text-xs font-medium transition-colors"
                :class="form.scope === 'private'
                  ? 'bg-primary/10 text-primary'
                  : 'text-muted-foreground hover:bg-secondary/50'"
                @click="form.scope = 'private'"
              >
                {{ t('mcp.private') }}
              </button>
            </div>
          </div>

          <DialogFooter class="gap-1.5 pt-1">
            <Button type="button" variant="outline" size="sm" :disabled="testing" @click="testServer">
              <FlaskConical class="h-3.5 w-3.5 mr-1" />
              {{ testing ? t('mcp.testing') : t('mcp.test') }}
            </Button>
            <Button type="submit" size="sm" :disabled="saving">
              {{ saving ? t('common.loading') : (isEditing ? t('common.save') : t('mcp.addServer')) }}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>

    <ConfirmDialog
      :open="showDeleteDialog"
      :title="t('common.delete')"
      :description="t('mcp.confirmDelete')"
      :confirm-text="t('common.delete')"
      variant="destructive"
      @confirm="deleteServer"
      @cancel="showDeleteDialog = false; deletingServer = null"
    />
  </div>
</template>
