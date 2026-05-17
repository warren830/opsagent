<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Plus, Pencil, Trash2, X, ShieldCheck } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
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
import DataTable from '@/components/shared/DataTable.vue'
import ConfirmDialog from '@/components/shared/ConfirmDialog.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const authStore = useAuthStore()

interface EntraIdConnection {
  id: string
  name: string
  entra_tenant_id: string
  client_id: string
  auto_provision: boolean
  default_role: string
  enabled: boolean
  allowed_domains: string[]
  created_at: string
  updated_at: string
}

const connections = ref<EntraIdConnection[]>([])
const loading = ref(true)
const saving = ref(false)

const showFormDialog = ref(false)
const editingConnection = ref<EntraIdConnection | null>(null)
const showDeleteDialog = ref(false)
const deletingConnection = ref<EntraIdConnection | null>(null)

const domainInput = ref('')

const form = ref({
  name: '',
  entra_tenant_id: '',
  client_id: '',
  client_secret: '',
  auto_provision: true,
  default_role: 'readonly' as string,
  enabled: true,
  allowed_domains: [] as string[],
})

const isEditing = computed(() => !!editingConnection.value)

const columns = computed(() => [
  { key: 'name', label: t('entraConnection.name') },
  { key: 'entra_tenant_id', label: t('entraConnection.entraTenantId') },
  { key: 'enabled', label: t('entraConnection.enabled') },
  { key: 'auto_provision', label: t('entraConnection.autoProvision') },
  { key: 'allowed_domains', label: t('entraConnection.allowedDomains') },
])

async function fetchConnections() {
  loading.value = true
  try {
    connections.value = await api.get<EntraIdConnection[]>('/api/entra-id-connections')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  if (authStore.isSuperAdmin) {
    fetchConnections()
  }
})

function openCreate() {
  editingConnection.value = null
  domainInput.value = ''
  form.value = {
    name: '',
    entra_tenant_id: '',
    client_id: '',
    client_secret: '',
    auto_provision: true,
    default_role: 'readonly',
    enabled: true,
    allowed_domains: [],
  }
  showFormDialog.value = true
}

function openEdit(conn: EntraIdConnection) {
  editingConnection.value = conn
  domainInput.value = ''
  form.value = {
    name: conn.name,
    entra_tenant_id: conn.entra_tenant_id,
    client_id: conn.client_id,
    client_secret: '',
    auto_provision: conn.auto_provision,
    default_role: conn.default_role,
    enabled: conn.enabled,
    allowed_domains: [...conn.allowed_domains],
  }
  showFormDialog.value = true
}

function openDelete(conn: EntraIdConnection) {
  deletingConnection.value = conn
  showDeleteDialog.value = true
}

function addDomain() {
  const d = domainInput.value.trim().toLowerCase()
  if (d && !form.value.allowed_domains.includes(d)) {
    form.value.allowed_domains.push(d)
  }
  domainInput.value = ''
}

function removeDomain(index: number) {
  form.value.allowed_domains.splice(index, 1)
}

async function saveConnection() {
  if (!form.value.name || !form.value.entra_tenant_id || !form.value.client_id) {
    toast.error(t('common.requiredFields'))
    return
  }
  if (!isEditing.value && !form.value.client_secret) {
    toast.error(t('common.requiredFields'))
    return
  }
  saving.value = true
  try {
    const payload: Record<string, unknown> = {
      name: form.value.name,
      entra_tenant_id: form.value.entra_tenant_id,
      client_id: form.value.client_id,
      auto_provision: form.value.auto_provision,
      default_role: form.value.default_role,
      enabled: form.value.enabled,
      allowed_domains: form.value.allowed_domains,
    }
    if (form.value.client_secret) {
      payload.client_secret = form.value.client_secret
    }

    if (isEditing.value) {
      await api.put(`/api/entra-id-connections/${editingConnection.value!.id}`, payload)
      toast.success(t('entraConnection.updateSuccess'))
    } else {
      await api.post('/api/entra-id-connections', payload)
      toast.success(t('entraConnection.createSuccess'))
    }
    showFormDialog.value = false
    await fetchConnections()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteConnection() {
  if (!deletingConnection.value) return
  try {
    await api.del(`/api/entra-id-connections/${deletingConnection.value.id}`)
    toast.success(t('entraConnection.deleteSuccess'))
    showDeleteDialog.value = false
    deletingConnection.value = null
    await fetchConnections()
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
      <div>
        <h1 class="text-base font-semibold text-foreground">{{ t('entraConnection.title') }}</h1>
        <p class="text-xs text-muted-foreground mt-0.5">{{ t('entraConnection.description') }}</p>
      </div>
      <Button v-if="authStore.isSuperAdmin" size="sm" @click="openCreate">
        <Plus class="h-3.5 w-3.5 mr-1" />
        {{ t('entraConnection.create') }}
      </Button>
    </div>

    <!-- No permission -->
    <div v-if="!authStore.isSuperAdmin" class="flex flex-col items-center justify-center gap-3 py-16 text-center">
      <ShieldCheck class="h-10 w-10 text-muted-foreground/30" />
      <p class="text-sm text-muted-foreground">{{ t('common.noPermission') }}</p>
    </div>

    <!-- Data Table -->
    <DataTable v-else :columns="columns" :data="connections" :loading="loading">
      <template #cell-name="{ row }">
        <span class="font-medium text-foreground">{{ (row as EntraIdConnection).name }}</span>
      </template>

      <template #cell-entra_tenant_id="{ row }">
        <code class="text-xs text-muted-foreground bg-muted/50 px-1.5 py-0.5 rounded font-mono">
          {{ (row as EntraIdConnection).entra_tenant_id }}
        </code>
      </template>

      <template #cell-enabled="{ row }">
        <Badge :variant="(row as EntraIdConnection).enabled ? 'success' : 'secondary'">
          {{ (row as EntraIdConnection).enabled ? t('entraConnection.enabled') : t('entraConnection.disabled') }}
        </Badge>
      </template>

      <template #cell-auto_provision="{ row }">
        <Badge :variant="(row as EntraIdConnection).auto_provision ? 'info' : 'secondary'">
          {{ (row as EntraIdConnection).auto_provision ? t('entraConnection.autoProvisionOn') : t('entraConnection.autoProvisionOff') }}
        </Badge>
      </template>

      <template #cell-allowed_domains="{ row }">
        <div v-if="(row as EntraIdConnection).allowed_domains?.length" class="flex flex-wrap gap-1">
          <Badge
            v-for="domain in (row as EntraIdConnection).allowed_domains"
            :key="domain"
            variant="outline"
            class="text-[10px]"
          >
            {{ domain }}
          </Badge>
        </div>
        <span v-else class="text-muted-foreground">-</span>
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" @click="openEdit(row as EntraIdConnection)">
          <Pencil class="h-3 w-3" />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          class="text-destructive hover:text-destructive"
          @click="openDelete(row as EntraIdConnection)"
        >
          <Trash2 class="h-3 w-3" />
        </Button>
      </template>
    </DataTable>

    <!-- Create / Edit Dialog -->
    <Dialog :open="showFormDialog" @update:open="(val) => { showFormDialog = val }">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>
            {{ isEditing ? t('entraConnection.edit') : t('entraConnection.create') }}
          </DialogTitle>
          <DialogDescription>
            {{ isEditing ? t('entraConnection.editDescription') : t('entraConnection.createDescription') }}
          </DialogDescription>
        </DialogHeader>

        <form class="space-y-3" @submit.prevent="saveConnection">
          <!-- Name -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">
              {{ t('entraConnection.name') }}
              <span class="text-destructive">*</span>
            </label>
            <Input
              v-model="form.name"
              :placeholder="t('entraConnection.namePlaceholder')"
              required
            />
          </div>

          <!-- Azure Tenant ID -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">
              {{ t('entraConnection.entraTenantId') }}
              <span class="text-destructive">*</span>
            </label>
            <Input
              v-model="form.entra_tenant_id"
              :placeholder="t('entraConnection.entraTenantIdPlaceholder')"
              class="font-mono"
              required
            />
          </div>

          <!-- Client ID -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">
              {{ t('entraConnection.clientId') }}
              <span class="text-destructive">*</span>
            </label>
            <Input
              v-model="form.client_id"
              :placeholder="t('entraConnection.clientIdPlaceholder')"
              class="font-mono"
              required
            />
          </div>

          <!-- Client Secret -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">
              {{ t('entraConnection.clientSecret') }}
              <span v-if="!isEditing" class="text-destructive">*</span>
            </label>
            <Input
              v-model="form.client_secret"
              type="password"
              :placeholder="isEditing ? t('entraConnection.clientSecretEditPlaceholder') : t('entraConnection.clientSecretPlaceholder')"
              :required="!isEditing"
            />
            <p v-if="isEditing" class="text-[11px] text-muted-foreground">
              {{ t('entraConnection.clientSecretHint') }}
            </p>
          </div>

          <!-- Default Role -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('entraConnection.defaultRole') }}</label>
            <Select v-model="form.default_role">
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="super_admin">{{ t('role.superAdmin') }}</SelectItem>
                <SelectItem value="tenant_admin">{{ t('role.tenantAdmin') }}</SelectItem>
                <SelectItem value="user">{{ t('role.user') }}</SelectItem>
                <SelectItem value="readonly">{{ t('role.readonly') }}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <!-- Allowed Domains -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('entraConnection.allowedDomains') }}</label>
            <div class="flex gap-1.5">
              <Input
                v-model="domainInput"
                :placeholder="t('entraConnection.allowedDomainsPlaceholder')"
                class="flex-1"
                @keydown.enter.prevent="addDomain"
              />
              <Button type="button" variant="outline" size="sm" @click="addDomain">
                <Plus class="h-3 w-3" />
              </Button>
            </div>
            <p class="text-[11px] text-muted-foreground">
              {{ t('entraConnection.allowedDomainsHint') }}
            </p>
            <div v-if="form.allowed_domains.length" class="flex flex-wrap gap-1">
              <Badge
                v-for="(domain, i) in form.allowed_domains"
                :key="domain"
                variant="secondary"
                class="text-[11px] gap-1 pr-1"
              >
                {{ domain }}
                <button type="button" class="hover:text-destructive" @click="removeDomain(i)">
                  <X class="h-2.5 w-2.5" />
                </button>
              </Badge>
            </div>
          </div>

          <!-- Auto Provision toggle -->
          <div class="flex items-center justify-between rounded border border-border/60 px-3 py-2">
            <div>
              <label class="text-xs font-medium">{{ t('entraConnection.autoProvision') }}</label>
              <p class="text-[11px] text-muted-foreground">{{ t('entraConnection.autoProvisionHint') }}</p>
            </div>
            <Switch v-model:checked="form.auto_provision" />
          </div>

          <!-- Enabled toggle -->
          <div class="flex items-center justify-between rounded border border-border/60 px-3 py-2">
            <label class="text-xs font-medium">{{ t('entraConnection.enabled') }}</label>
            <Switch v-model:checked="form.enabled" />
          </div>

          <DialogFooter class="gap-1.5 pt-1">
            <Button type="button" variant="outline" size="sm" @click="showFormDialog = false">
              {{ t('common.cancel') }}
            </Button>
            <Button type="submit" size="sm" :disabled="saving">
              {{ saving ? t('common.loading') : (isEditing ? t('common.save') : t('common.create')) }}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>

    <!-- Delete Confirm -->
    <ConfirmDialog
      :open="showDeleteDialog"
      :title="t('entraConnection.deleteTitle')"
      :description="t('entraConnection.confirmDelete')"
      :confirm-text="t('common.delete')"
      variant="destructive"
      @confirm="deleteConnection"
      @cancel="showDeleteDialog = false; deletingConnection = null"
    />
  </div>
</template>
