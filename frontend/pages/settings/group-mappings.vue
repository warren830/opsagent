<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Plus, Pencil, Trash2, ShieldCheck } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
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

interface GroupMapping {
  id: string
  group_id: string
  group_name: string
  role: string
  tenant_id: string | null
  created_at: string
  updated_at: string
}

const mappings = ref<GroupMapping[]>([])
const loading = ref(true)
const saving = ref(false)

const showFormDialog = ref(false)
const editingMapping = ref<GroupMapping | null>(null)
const showDeleteDialog = ref(false)
const deletingMapping = ref<GroupMapping | null>(null)

const form = ref({
  group_id: '',
  group_name: '',
  role: 'readonly' as string,
  tenant_id: '' as string,
})

const isEditing = computed(() => !!editingMapping.value)

const ROLE_VARIANTS: Record<string, string> = {
  super_admin: 'default',
  tenant_admin: 'info',
  user: 'secondary',
  readonly: 'secondary',
}

const columns = computed(() => [
  { key: 'group_name', label: t('groupMapping.groupName') },
  { key: 'group_id', label: t('groupMapping.groupId') },
  { key: 'role', label: t('groupMapping.role') },
  { key: 'tenant_id', label: t('groupMapping.tenantId') },
])

async function fetchMappings() {
  loading.value = true
  try {
    mappings.value = await api.get<GroupMapping[]>('/api/entra-group-mappings')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  if (authStore.isSuperAdmin) {
    fetchMappings()
  }
})

function openCreate() {
  editingMapping.value = null
  form.value = { group_id: '', group_name: '', role: 'readonly', tenant_id: '' }
  showFormDialog.value = true
}

function openEdit(mapping: GroupMapping) {
  editingMapping.value = mapping
  form.value = {
    group_id: mapping.group_id,
    group_name: mapping.group_name || '',
    role: mapping.role,
    tenant_id: mapping.tenant_id || '',
  }
  showFormDialog.value = true
}

function openDelete(mapping: GroupMapping) {
  deletingMapping.value = mapping
  showDeleteDialog.value = true
}

function getRoleLabel(role: string): string {
  const map: Record<string, string> = {
    super_admin: t('role.superAdmin'),
    tenant_admin: t('role.tenantAdmin'),
    user: t('role.user'),
    readonly: t('role.readonly'),
  }
  return map[role] || role
}

function truncateId(id: string): string {
  if (id.length <= 16) return id
  return `${id.slice(0, 8)}…${id.slice(-4)}`
}

async function saveMapping() {
  if (!form.value.group_id) {
    toast.error(t('common.requiredFields'))
    return
  }
  saving.value = true
  try {
    const payload = {
      group_id: form.value.group_id,
      group_name: form.value.group_name || undefined,
      role: form.value.role,
      tenant_id: form.value.tenant_id || null,
    }

    if (isEditing.value) {
      await api.put(`/api/entra-group-mappings/${editingMapping.value!.id}`, payload)
      toast.success(t('groupMapping.updateSuccess'))
    } else {
      await api.post('/api/entra-group-mappings', payload)
      toast.success(t('groupMapping.createSuccess'))
    }
    showFormDialog.value = false
    await fetchMappings()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteMapping() {
  if (!deletingMapping.value) return
  try {
    await api.del(`/api/entra-group-mappings/${deletingMapping.value.id}`)
    toast.success(t('groupMapping.deleteSuccess'))
    showDeleteDialog.value = false
    deletingMapping.value = null
    await fetchMappings()
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
        <h1 class="text-base font-semibold text-foreground">{{ t('groupMapping.title') }}</h1>
        <p class="text-xs text-muted-foreground mt-0.5">{{ t('groupMapping.description') }}</p>
      </div>
      <Button v-if="authStore.isSuperAdmin" size="sm" @click="openCreate">
        <Plus class="h-3.5 w-3.5 mr-1" />
        {{ t('groupMapping.create') }}
      </Button>
    </div>

    <!-- No permission -->
    <div v-if="!authStore.isSuperAdmin" class="flex flex-col items-center justify-center gap-3 py-16 text-center">
      <ShieldCheck class="h-10 w-10 text-muted-foreground/30" />
      <p class="text-sm text-muted-foreground">{{ t('common.noPermission') }}</p>
    </div>

    <!-- Data Table -->
    <DataTable v-else :columns="columns" :data="mappings" :loading="loading">
      <template #cell-group_name="{ row }">
        <span class="font-medium text-foreground">
          {{ (row as GroupMapping).group_name || '-' }}
        </span>
      </template>

      <template #cell-group_id="{ row }">
        <code
          class="text-xs text-muted-foreground bg-muted/50 px-1.5 py-0.5 rounded font-mono"
          :title="(row as GroupMapping).group_id"
        >
          {{ truncateId((row as GroupMapping).group_id) }}
        </code>
      </template>

      <template #cell-role="{ row }">
        <Badge :variant="(ROLE_VARIANTS[(row as GroupMapping).role] || 'secondary') as any">
          {{ getRoleLabel((row as GroupMapping).role) }}
        </Badge>
      </template>

      <template #cell-tenant_id="{ row }">
        <span class="text-muted-foreground text-xs">
          {{ (row as GroupMapping).tenant_id || '-' }}
        </span>
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" @click="openEdit(row as GroupMapping)">
          <Pencil class="h-3 w-3" />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          class="text-destructive hover:text-destructive"
          @click="openDelete(row as GroupMapping)"
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
            {{ isEditing ? t('groupMapping.edit') : t('groupMapping.create') }}
          </DialogTitle>
          <DialogDescription>
            {{ isEditing ? t('groupMapping.editDescription') : t('groupMapping.createDescription') }}
          </DialogDescription>
        </DialogHeader>

        <form class="space-y-3" @submit.prevent="saveMapping">
          <!-- Group Object ID -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">
              {{ t('groupMapping.groupId') }}
              <span class="text-destructive">*</span>
            </label>
            <Input
              v-model="form.group_id"
              :placeholder="t('groupMapping.groupIdPlaceholder')"
              class="font-mono"
              required
            />
            <p class="text-[11px] text-muted-foreground">{{ t('groupMapping.groupIdHint') }}</p>
          </div>

          <!-- Group Display Name -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('groupMapping.groupName') }}</label>
            <Input
              v-model="form.group_name"
              :placeholder="t('groupMapping.groupNamePlaceholder')"
            />
          </div>

          <!-- Role -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('groupMapping.role') }}</label>
            <Select v-model="form.role">
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="super_admin">{{ t('role.superAdmin') }}</SelectItem>
                <SelectItem value="tenant_admin">{{ t('role.tenantAdmin') }}</SelectItem>
                <SelectItem value="user">{{ t('role.user') }}</SelectItem>
                <SelectItem value="readonly">{{ t('role.readonly') }}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <!-- Tenant ID (optional) -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">
              {{ t('groupMapping.tenantId') }}
            </label>
            <Input
              v-model="form.tenant_id"
              :placeholder="t('groupMapping.tenantIdPlaceholder')"
              class="font-mono"
            />
            <p class="text-[11px] text-muted-foreground">{{ t('groupMapping.tenantIdHint') }}</p>
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
      :title="t('groupMapping.deleteTitle')"
      :description="t('groupMapping.confirmDelete')"
      :confirm-text="t('common.delete')"
      variant="destructive"
      @confirm="deleteMapping"
      @cancel="showDeleteDialog = false; deletingMapping = null"
    />
  </div>
</template>
