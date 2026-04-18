<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Plus, Pencil, Trash2, Mail, RotateCw, Copy, Check } from 'lucide-vue-next'
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

interface User {
  id: string
  username: string
  email: string | null
  role: 'super_admin' | 'member'
  tenant_id: string | null
  tenant_name?: string | null
  is_active: boolean
  auth_method: string
  created_at: string
}

interface Tenant {
  id: string
  name: string
  slug: string
}

const users = ref<User[]>([])
const tenants = ref<Tenant[]>([])
const loading = ref(true)
const saving = ref(false)

const showFormDialog = ref(false)
const editingUser = ref<User | null>(null)
const showDeleteDialog = ref(false)
const deletingUser = ref<User | null>(null)
const showInviteLinkDialog = ref(false)
const inviteLink = ref('')
const linkCopied = ref(false)
const resending = ref<string | null>(null)

const isCloud = computed(() => authStore.providers?.is_cloud === true)

const form = ref({
  username: '',
  password: '',
  email: '',
  role: 'member' as 'super_admin' | 'member',
  tenant_id: '' as string,
  is_active: true,
})

const isEditing = computed(() => !!editingUser.value)

const formTitle = computed(() => {
  if (isEditing.value) return t('user.edit')
  return isCloud.value ? t('user.invite') : t('user.create')
})

const formDescription = computed(() => {
  if (isEditing.value) return t('user.editDescription')
  return isCloud.value ? t('user.inviteDescription') : t('user.createDescription')
})

const columns = computed(() => [
  { key: 'username', label: t('user.username') },
  { key: 'email', label: t('user.email') },
  { key: 'role', label: t('user.role') },
  { key: 'tenant_name', label: t('user.tenant') },
  { key: 'is_active', label: t('user.status') },
])

async function fetchUsers() {
  loading.value = true
  try {
    users.value = await api.get<User[]>('/api/users')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

async function fetchTenants() {
  try {
    tenants.value = await api.get<Tenant[]>('/api/tenants')
  } catch {
    // silently fail
  }
}

onMounted(() => {
  fetchUsers()
  fetchTenants()
})

function openCreate() {
  editingUser.value = null
  form.value = { username: '', password: '', email: '', role: 'member', tenant_id: '', is_active: true }
  showFormDialog.value = true
}

function openEdit(user: User) {
  editingUser.value = user
  form.value = { username: user.username, password: '', email: user.email || '', role: user.role, tenant_id: user.tenant_id || '', is_active: user.is_active }
  showFormDialog.value = true
}

function openDelete(user: User) {
  deletingUser.value = user
  showDeleteDialog.value = true
}

function onRoleChange() {
  if (form.value.role === 'super_admin') {
    form.value.tenant_id = ''
  }
}

async function saveUser() {
  saving.value = true
  try {
    if (isEditing.value) {
      // Update existing user
      const payload: Record<string, unknown> = {
        username: form.value.username,
        email: form.value.email || null,
        role: form.value.role,
        tenant_id: form.value.role === 'super_admin' ? null : (form.value.tenant_id || null),
        is_active: form.value.is_active,
      }
      await api.put(`/api/users/${editingUser.value!.id}`, payload)
      toast.success(t('user.updateSuccess'))
    } else if (isCloud.value) {
      // Cloud: invite by email
      const payload = {
        email: form.value.email,
        role: form.value.role,
        tenant_id: form.value.role === 'super_admin' ? null : (form.value.tenant_id || null),
      }
      const result = await api.post<{ user: any; invite_link: string }>('/api/users/invite', payload)
      handleInviteLinkResponse(result)
      toast.success(t('user.inviteSuccess'))
    } else {
      // Local: create with password
      const payload: Record<string, unknown> = {
        username: form.value.username,
        password: form.value.password,
        email: form.value.email || null,
        role: form.value.role,
        tenant_id: form.value.role === 'super_admin' ? null : (form.value.tenant_id || null),
        is_active: form.value.is_active,
      }
      await api.post('/api/users', payload)
      toast.success(t('user.createSuccess'))
    }
    showFormDialog.value = false
    await fetchUsers()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteUser() {
  if (!deletingUser.value) return
  try {
    await api.del(`/api/users/${deletingUser.value.id}`)
    toast.success(t('user.deleteSuccess'))
    showDeleteDialog.value = false
    deletingUser.value = null
    await fetchUsers()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

function handleInviteLinkResponse(result: { invite_link: string }) {
  inviteLink.value = result.invite_link
  linkCopied.value = false
  showInviteLinkDialog.value = true
}

async function copyInviteLink() {
  await navigator.clipboard.writeText(inviteLink.value)
  linkCopied.value = true
  toast.success(t('common.success'))
}

async function resendInvite(user: User) {
  resending.value = user.id
  try {
    const result = await api.post<{ user: any; invite_link: string }>(`/api/users/${user.id}/resend-invite`, {})
    handleInviteLinkResponse(result)
    toast.success(t('user.inviteResent'))
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    resending.value = null
  }
}

function getTenantName(tenantId: string | null): string {
  if (!tenantId) return '-'
  const tenant = tenants.value.find((t) => t.id === tenantId)
  return tenant?.name || tenantId
}

function getRoleLabel(role: string): string {
  if (role === 'super_admin') return t('user.superAdmin')
  return t('user.member')
}
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('user.title') }}</h1>
      <Button v-if="authStore.isSuperAdmin" size="sm" @click="openCreate">
        <component :is="isCloud ? Mail : Plus" class="h-3.5 w-3.5" />
        {{ isCloud ? t('user.invite') : t('user.create') }}
      </Button>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="users" :loading="loading">
      <template #cell-username="{ row }">
        <span class="font-medium text-foreground">{{ (row as User).username }}</span>
      </template>

      <template #cell-email="{ row }">
        <span class="text-muted-foreground">{{ (row as User).email || '-' }}</span>
      </template>

      <template #cell-role="{ row }">
        <Badge :variant="(row as User).role === 'super_admin' ? 'default' : 'secondary'">
          {{ getRoleLabel((row as User).role) }}
        </Badge>
        <Badge v-if="(row as User).auth_method === 'invited'" variant="warning" class="ml-1">
          {{ t('user.pending') }}
        </Badge>
      </template>

      <template #cell-tenant_name="{ row }">
        <span class="text-muted-foreground">{{ getTenantName((row as User).tenant_id) }}</span>
      </template>

      <template #cell-is_active="{ row }">
        <Badge :variant="(row as User).is_active ? 'success' : 'warning'">
          {{ (row as User).is_active ? t('user.active') : t('user.inactive') }}
        </Badge>
      </template>

      <template #actions="{ row }">
        <Button v-if="(row as User).auth_method === 'invited'" variant="ghost" size="icon-sm" :disabled="resending === (row as User).id" @click="resendInvite(row as User)">
          <RotateCw class="h-3 w-3" :class="{ 'animate-spin': resending === (row as User).id }" />
        </Button>
        <Button variant="ghost" size="icon-sm" @click="openEdit(row as User)">
          <Pencil class="h-3 w-3" />
        </Button>
        <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(row as User)">
          <Trash2 class="h-3 w-3" />
        </Button>
      </template>
    </DataTable>

    <!-- Create/Edit/Invite Dialog -->
    <Dialog :open="showFormDialog" @update:open="(val) => { showFormDialog = val }">
      <DialogContent class="max-w-sm">
        <DialogHeader>
          <DialogTitle>{{ formTitle }}</DialogTitle>
          <DialogDescription>{{ formDescription }}</DialogDescription>
        </DialogHeader>

        <form class="space-y-3" @submit.prevent="saveUser">
          <!-- Cloud invite: email only -->
          <template v-if="isCloud && !isEditing">
            <div class="space-y-1.5">
              <label class="text-xs font-medium">{{ t('user.email') }}</label>
              <Input v-model="form.email" type="email" :placeholder="t('user.email')" required />
            </div>
          </template>

          <!-- Local create / Edit: full form -->
          <template v-else>
            <div class="space-y-1.5">
              <label class="text-xs font-medium">{{ t('user.username') }}</label>
              <Input v-model="form.username" :placeholder="t('user.username')" required />
            </div>

            <div v-if="!isEditing" class="space-y-1.5">
              <label class="text-xs font-medium">{{ t('user.password') }}</label>
              <Input v-model="form.password" type="password" :placeholder="t('user.password')" required />
            </div>

            <div class="space-y-1.5">
              <label class="text-xs font-medium">{{ t('user.email') }}</label>
              <Input v-model="form.email" type="email" :placeholder="t('user.email')" />
            </div>
          </template>

          <!-- Role (always shown) -->
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('user.role') }}</label>
            <Select v-model="form.role" @update:model-value="onRoleChange">
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="super_admin">{{ t('user.superAdmin') }}</SelectItem>
                <SelectItem value="member">{{ t('user.member') }}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <!-- Tenant (member only) -->
          <div v-if="form.role === 'member'" class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('user.tenant') }} <span class="text-destructive">*</span></label>
            <Select v-model="form.tenant_id">
              <SelectTrigger><SelectValue :placeholder="t('user.selectTenant')" /></SelectTrigger>
              <SelectContent>
                <SelectItem v-for="tenant in tenants" :key="tenant.id" :value="tenant.id">{{ tenant.name }}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <!-- Active toggle (not shown for cloud invite) -->
          <div v-if="isEditing || !isCloud" class="flex items-center justify-between rounded border border-border/60 px-3 py-2">
            <label class="text-xs font-medium">{{ t('user.isActive') }}</label>
            <Switch v-model:checked="form.is_active" />
          </div>

          <DialogFooter class="gap-1.5 pt-1">
            <Button type="button" variant="outline" size="sm" @click="showFormDialog = false">{{ t('common.cancel') }}</Button>
            <Button type="submit" size="sm" :disabled="saving">
              {{ saving ? t('common.loading') : (isEditing ? t('common.save') : (isCloud ? t('user.invite') : t('common.create'))) }}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>

    <ConfirmDialog
      :open="showDeleteDialog"
      :title="t('user.deleteTitle')"
      :description="t('user.confirmDelete')"
      :confirm-text="t('common.delete')"
      variant="destructive"
      @confirm="deleteUser"
      @cancel="showDeleteDialog = false; deletingUser = null"
    />

    <!-- Invite Link Dialog -->
    <Dialog :open="showInviteLinkDialog" @update:open="(val) => { showInviteLinkDialog = val }">
      <DialogContent class="max-w-sm">
        <DialogHeader>
          <DialogTitle>{{ t('user.inviteLinkTitle') }}</DialogTitle>
          <DialogDescription>{{ t('user.inviteLinkDescription') }}</DialogDescription>
        </DialogHeader>
        <div class="flex items-center gap-2">
          <Input :model-value="inviteLink" readonly class="text-xs font-mono" />
          <Button size="icon-sm" variant="outline" @click="copyInviteLink">
            <component :is="linkCopied ? Check : Copy" class="h-3.5 w-3.5" />
          </Button>
        </div>
        <DialogFooter>
          <Button size="sm" variant="outline" @click="showInviteLinkDialog = false">{{ t('common.close') }}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
