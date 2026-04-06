<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Plus, Pencil, Trash2, Search, Zap, BookOpen, X, Shield } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
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

// Common AWS regions for quick-select
const AWS_REGIONS = [
  'us-east-1', 'us-east-2', 'us-west-1', 'us-west-2',
  'eu-west-1', 'eu-west-2', 'eu-central-1',
  'ap-northeast-1', 'ap-southeast-1', 'ap-southeast-2', 'ap-south-1',
  'cn-north-1', 'cn-northwest-1',
  'me-south-1', 'sa-east-1', 'af-south-1',
]

interface Account {
  id: string
  provider: 'aws' | 'alicloud' | 'azure'
  name: string
  account_id: string
  role_arn: string | null
  profile: string | null
  regions: string[]
  source: 'manual' | 'organization'
  tenant_id: string | null
  config: Record<string, unknown> | null
  is_mock: boolean
  created_at: string
}

interface TestResult {
  success: boolean
  identity: string | null
  error: string | null
}

interface Tenant {
  id: string
  name: string
}

interface User {
  id: string
  username: string
  email: string | null
  role: string
  is_active: boolean
}

interface UserAccessView {
  user_id: string
  username: string
  email: string | null
  role: string
  created_at: string
}

const accounts = ref<Account[]>([])
const tenants = ref<Tenant[]>([])
const users = ref<User[]>([])
const loading = ref(true)
const saving = ref(false)
const discovering = ref(false)
const testingId = ref<string | null>(null)

const showFormDialog = ref(false)
const editingAccount = ref<Account | null>(null)
const showDeleteDialog = ref(false)
const deletingAccount = ref<Account | null>(null)
const showGuideDialog = ref(false)
const showRegionPicker = ref(false)

// Access management
const showAccessDialog = ref(false)
const accessAccount = ref<Account | null>(null)
const accessUsers = ref<UserAccessView[]>([])
const accessLoading = ref(false)
const grantForm = ref({ user_id: '', role: 'readonly' })
const granting = ref(false)

const form = ref({
  provider: 'aws' as 'aws' | 'alicloud' | 'azure',
  name: '',
  account_id: '',
  mode: 'cloud' as 'local' | 'cloud',
  role_arn: '',
  profile: '',
  allRegions: false,
  selectedRegions: ['us-east-1'] as string[],
  tenant_id: '',
  is_mock: false,
})

const isEditing = computed(() => !!editingAccount.value)
const formTitle = computed(() => isEditing.value ? t('common.edit') : t('account.addAccount'))

const columns = computed(() => [
  { key: 'provider', label: t('account.provider') },
  { key: 'name', label: t('account.accountName') },
  { key: 'account_id', label: t('account.accountId') },
  { key: 'mode', label: t('account.mode') },
  { key: 'regions', label: t('account.regions') },
  { key: 'tenant_id', label: t('user.tenant') },
  { key: 'source', label: t('account.source') },
])

// Users not yet granted access to current account
const availableUsers = computed(() => {
  const grantedIds = new Set(accessUsers.value.map(u => u.user_id))
  return users.value.filter(u => !grantedIds.has(u.id) && u.role !== 'super_admin' && u.is_active)
})

function getTenantName(tenantId: string | null): string {
  if (!tenantId) return '-'
  const tenant = tenants.value.find((t) => t.id === tenantId)
  return tenant?.name || tenantId
}

async function fetchTenants() {
  try {
    tenants.value = await api.get<Tenant[]>('/api/tenants')
  } catch { /* ignore */ }
}

async function fetchUsers() {
  try {
    users.value = await api.get<User[]>('/api/users')
  } catch { /* ignore */ }
}

function providerLabel(p: string): string {
  const map: Record<string, string> = {
    aws: t('account.aws'),
    alicloud: t('account.alicloud'),
    azure: t('account.azure'),
  }
  return map[p] || p
}

function providerVariant(p: string): string {
  const map: Record<string, string> = { aws: 'info', alicloud: 'warning', azure: 'default' }
  return map[p] || 'default'
}

function inferMode(account: Account): 'local' | 'cloud' {
  if (account.profile) return 'local'
  return 'cloud'
}

function toggleRegion(region: string) {
  const idx = form.value.selectedRegions.indexOf(region)
  if (idx >= 0) {
    form.value.selectedRegions.splice(idx, 1)
  } else {
    form.value.selectedRegions.push(region)
  }
}

function removeRegion(region: string) {
  form.value.selectedRegions = form.value.selectedRegions.filter(r => r !== region)
}

async function fetchAccounts() {
  loading.value = true
  try {
    accounts.value = await api.get<Account[]>('/api/accounts')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  fetchAccounts()
  fetchTenants()
  fetchUsers()
})

function openCreate() {
  editingAccount.value = null
  form.value = { provider: 'aws', name: '', account_id: '', mode: 'cloud', role_arn: '', profile: '', allRegions: false, selectedRegions: ['us-east-1'], tenant_id: tenants.value[0]?.id || '', is_mock: false }
  showRegionPicker.value = false
  showFormDialog.value = true
}

function openEdit(account: Account) {
  editingAccount.value = account
  const regions = account.regions || []
  const isAll = regions.length === 0
  form.value = {
    provider: account.provider,
    name: account.name,
    account_id: account.account_id || '',
    mode: inferMode(account),
    role_arn: account.role_arn || '',
    profile: account.profile || '',
    allRegions: isAll,
    selectedRegions: isAll ? [] : [...regions],
    tenant_id: account.tenant_id || '',
    is_mock: account.is_mock,
  }
  showRegionPicker.value = false
  showFormDialog.value = true
}

function openDelete(account: Account) {
  deletingAccount.value = account
  showDeleteDialog.value = true
}

async function saveAccount() {
  saving.value = true
  try {
    const regions = form.value.allRegions ? [] : form.value.selectedRegions
    const payload: Record<string, unknown> = {
      provider: form.value.provider,
      name: form.value.name,
      account_id: form.value.account_id,
      role_arn: form.value.mode === 'cloud' ? (form.value.role_arn || null) : null,
      profile: form.value.mode === 'local' ? (form.value.profile || null) : null,
      regions,
      tenant_id: form.value.tenant_id || null,
      is_mock: form.value.is_mock,
      config: {},
    }
    if (isEditing.value) {
      await api.put(`/api/accounts/${editingAccount.value!.id}`, payload)
    } else {
      await api.post('/api/accounts', payload)
    }
    toast.success(t('common.success'))
    showFormDialog.value = false
    await fetchAccounts()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteAccount() {
  if (!deletingAccount.value) return
  try {
    await api.del(`/api/accounts/${deletingAccount.value.id}`)
    toast.success(t('common.success'))
    showDeleteDialog.value = false
    deletingAccount.value = null
    await fetchAccounts()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function discoverAccounts() {
  discovering.value = true
  try {
    const result = await api.post<Account[]>('/api/accounts/discover', {})
    if (result.length > 0) {
      toast.success(t('account.discoverSuccess', { count: result.length }))
      await fetchAccounts()
    } else {
      toast.info(t('account.discoverEmpty'))
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    discovering.value = false
  }
}

async function testConnection(account: Account) {
  testingId.value = account.id
  try {
    const result = await api.post<TestResult>(`/api/accounts/${account.id}/test`, {})
    if (result.success) {
      toast.success(`${t('account.testSuccess')}: ${result.identity || ''}`)
    } else {
      toast.error(`${t('account.testFailed')}: ${result.error || 'Unknown error'}`)
    }
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    testingId.value = null
  }
}

// ── Access Management ──────────────────────────────────────────────────

async function openAccessDialog(account: Account) {
  accessAccount.value = account
  showAccessDialog.value = true
  grantForm.value = { user_id: '', role: 'readonly' }
  await fetchAccessUsers(account.id)
}

async function fetchAccessUsers(accountId: string) {
  accessLoading.value = true
  try {
    accessUsers.value = await api.get<UserAccessView[]>(`/api/accounts/${accountId}/users`)
  } catch {
    accessUsers.value = []
  } finally {
    accessLoading.value = false
  }
}

async function grantAccess() {
  if (!accessAccount.value || !grantForm.value.user_id) return
  granting.value = true
  try {
    await api.post('/api/account-access/grant', {
      user_id: grantForm.value.user_id,
      account_id: accessAccount.value.id,
      role: grantForm.value.role,
    })
    toast.success(t('account.grantSuccess'))
    grantForm.value = { user_id: '', role: 'readonly' }
    await fetchAccessUsers(accessAccount.value.id)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    granting.value = false
  }
}

async function revokeAccess(userId: string) {
  if (!accessAccount.value) return
  try {
    await api.del(`/api/account-access/${userId}/${accessAccount.value.id}`)
    toast.success(t('account.revokeSuccess'))
    await fetchAccessUsers(accessAccount.value.id)
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
      <h1 class="text-base font-semibold text-foreground">{{ t('account.title') }}</h1>
      <div class="flex items-center gap-2">
        <Button variant="outline" size="sm" :disabled="discovering" @click="discoverAccounts">
          <Search class="h-3.5 w-3.5" :class="{ 'animate-spin': discovering }" />
          {{ discovering ? t('account.discovering') : t('account.discover') }}
        </Button>
        <Button size="sm" @click="openCreate">
          <Plus class="h-3.5 w-3.5" />
          {{ t('account.addAccount') }}
        </Button>
      </div>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="accounts" :loading="loading">
      <template #cell-provider="{ row }">
        <Badge :variant="providerVariant((row as Account).provider) as any">{{ providerLabel((row as Account).provider) }}</Badge>
      </template>

      <template #cell-name="{ row }">
        <span class="font-medium text-foreground">{{ (row as Account).name }}</span>
        <Badge v-if="(row as Account).is_mock" variant="warning" class="ml-1.5">Mock</Badge>
      </template>

      <template #cell-account_id="{ row }">
        <code class="rounded-sm bg-secondary px-1.5 py-0.5 text-[11px] font-mono text-muted-foreground">{{ (row as Account).account_id || '-' }}</code>
      </template>

      <template #cell-mode="{ row }">
        <Badge v-if="inferMode(row as Account) === 'local'" variant="secondary">
          {{ t('account.modeLocal') }}
        </Badge>
        <Badge v-else variant="info">
          {{ t('account.modeCloud') }}
        </Badge>
        <code v-if="(row as Account).role_arn" class="ml-1.5 rounded-sm bg-secondary px-1 py-0.5 text-[9px] font-mono text-muted-foreground/70 truncate max-w-[140px] inline-block align-middle">
          {{ (row as Account).role_arn }}
        </code>
        <span v-else-if="(row as Account).profile" class="ml-1.5 text-[10px] text-muted-foreground">
          {{ (row as Account).profile }}
        </span>
      </template>

      <template #cell-regions="{ row }">
        <Badge v-if="((row as Account).regions || []).length === 0" variant="default" class="text-[10px]">{{ t('account.allRegions') }}</Badge>
        <div v-else class="flex flex-wrap gap-1">
          <Badge v-for="r in ((row as Account).regions || []).slice(0, 3)" :key="r" variant="secondary" class="text-[10px]">{{ r }}</Badge>
          <Badge v-if="((row as Account).regions || []).length > 3" variant="secondary" class="text-[10px]">+{{ (row as Account).regions.length - 3 }}</Badge>
        </div>
      </template>

      <template #cell-tenant_id="{ row }">
        <span class="text-muted-foreground">{{ getTenantName((row as Account).tenant_id) }}</span>
      </template>

      <template #cell-source="{ row }">
        <Badge v-if="(row as Account).source === 'organization'" variant="info">{{ t('account.sourceOrg') }}</Badge>
        <Badge v-else variant="secondary">{{ t('account.sourceManual') }}</Badge>
      </template>

      <template #actions="{ row }">
        <Button
          v-if="(row as Account).provider === 'aws' && !(row as Account).is_mock"
          variant="ghost"
          size="icon-sm"
          :disabled="testingId === (row as Account).id"
          @click="testConnection(row as Account)"
        >
          <Zap class="h-3 w-3" :class="{ 'animate-pulse text-yellow-500': testingId === (row as Account).id }" />
        </Button>
        <Button variant="ghost" size="icon-sm" @click="openAccessDialog(row as Account)" :title="t('account.manageAccess')">
          <Shield class="h-3 w-3" />
        </Button>
        <Button variant="ghost" size="icon-sm" @click="openEdit(row as Account)">
          <Pencil class="h-3 w-3" />
        </Button>
        <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(row as Account)">
          <Trash2 class="h-3 w-3" />
        </Button>
      </template>
    </DataTable>

    <!-- Create/Edit Dialog -->
    <Dialog :open="showFormDialog" @update:open="(val) => { showFormDialog = val }">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>{{ formTitle }}</DialogTitle>
          <DialogDescription>{{ formTitle }}</DialogDescription>
        </DialogHeader>

        <form class="space-y-3" @submit.prevent="saveAccount">
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('user.tenant') }}</label>
            <select
              v-model="form.tenant_id"
              required
              class="flex h-8 w-full rounded border border-border/60 bg-secondary/50 px-2.5 py-1.5 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 transition-colors"
            >
              <option v-for="tenant in tenants" :key="tenant.id" :value="tenant.id">{{ tenant.name }}</option>
            </select>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('account.provider') }}</label>
            <select
              v-model="form.provider"
              class="flex h-8 w-full rounded border border-border/60 bg-secondary/50 px-2.5 py-1.5 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 transition-colors"
            >
              <option value="aws">{{ t('account.aws') }}</option>
              <option value="alicloud">{{ t('account.alicloud') }}</option>
              <option value="azure">{{ t('account.azure') }}</option>
            </select>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('account.accountName') }}</label>
            <Input v-model="form.name" :placeholder="t('account.accountName')" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('account.accountId') }}</label>
            <Input v-model="form.account_id" :placeholder="t('account.accountId')" required />
          </div>

          <!-- Mode selector (AWS only) -->
          <template v-if="form.provider === 'aws'">
            <div class="space-y-1.5">
              <label class="text-xs font-medium">{{ t('account.mode') }}</label>
              <div class="flex gap-2">
                <button
                  type="button"
                  class="flex-1 h-8 rounded border text-xs font-medium transition-all"
                  :class="form.mode === 'local'
                    ? 'border-primary bg-primary/10 text-primary'
                    : 'border-border/60 bg-secondary/50 text-muted-foreground hover:border-border'"
                  @click="form.mode = 'local'"
                >
                  {{ t('account.modeLocal') }}
                </button>
                <button
                  type="button"
                  class="flex-1 h-8 rounded border text-xs font-medium transition-all"
                  :class="form.mode === 'cloud'
                    ? 'border-primary bg-primary/10 text-primary'
                    : 'border-border/60 bg-secondary/50 text-muted-foreground hover:border-border'"
                  @click="form.mode = 'cloud'"
                >
                  {{ t('account.modeCloud') }}
                </button>
              </div>
            </div>

            <!-- Local mode: Profile -->
            <div v-if="form.mode === 'local'" class="space-y-1.5">
              <label class="text-xs font-medium">{{ t('account.profile') }}</label>
              <Input v-model="form.profile" :placeholder="t('account.profileHint')" />
              <p class="text-[10px] text-muted-foreground">{{ t('account.profileDesc') }}</p>
            </div>

            <!-- Cloud mode: Role ARN + guide -->
            <div v-if="form.mode === 'cloud'" class="space-y-1.5">
              <div class="flex items-center justify-between">
                <label class="text-xs font-medium">{{ t('account.roleArn') }}</label>
                <button type="button" class="flex items-center gap-1 text-[10px] text-primary hover:underline" @click="showGuideDialog = true">
                  <BookOpen class="h-3 w-3" />
                  {{ t('account.setupGuide') }}
                </button>
              </div>
              <Input v-model="form.role_arn" :placeholder="t('account.roleArnHint')" />
              <p class="text-[10px] text-muted-foreground">{{ t('account.roleArnDesc') }}</p>
            </div>
          </template>

          <!-- Regions -->
          <div class="space-y-1.5">
            <div class="flex items-center justify-between">
              <label class="text-xs font-medium">{{ t('account.regions') }}</label>
              <label class="flex items-center gap-1.5 cursor-pointer">
                <span class="text-[10px] text-muted-foreground">{{ t('account.allRegions') }}</span>
                <Switch
                  :checked="form.allRegions"
                  class="scale-75 origin-right"
                  @update:checked="(val: boolean) => { form.allRegions = val; if (val) form.selectedRegions = [] }"
                />
              </label>
            </div>

            <template v-if="!form.allRegions">
              <div class="flex flex-wrap gap-1 min-h-[32px] rounded border border-border/60 bg-secondary/30 p-1.5 cursor-pointer" @click="showRegionPicker = !showRegionPicker">
                <Badge
                  v-for="r in form.selectedRegions"
                  :key="r"
                  variant="secondary"
                  class="text-[10px] gap-0.5 pr-0.5"
                >
                  {{ r }}
                  <button type="button" class="ml-0.5 rounded-full hover:bg-foreground/10 p-0.5" @click.stop="removeRegion(r)">
                    <X class="h-2.5 w-2.5" />
                  </button>
                </Badge>
                <span v-if="form.selectedRegions.length === 0" class="text-[10px] text-muted-foreground/50 self-center px-1">{{ t('account.selectRegions') }}</span>
              </div>

              <div v-if="showRegionPicker" class="rounded border border-border/60 bg-secondary/50 p-1.5 max-h-[160px] overflow-y-auto grid grid-cols-2 gap-0.5">
                <button
                  v-for="r in AWS_REGIONS"
                  :key="r"
                  type="button"
                  class="text-left text-[10px] px-2 py-1 rounded transition-colors"
                  :class="form.selectedRegions.includes(r)
                    ? 'bg-primary/15 text-primary font-medium'
                    : 'text-muted-foreground hover:bg-secondary hover:text-foreground'"
                  @click="toggleRegion(r)"
                >
                  {{ r }}
                </button>
              </div>
            </template>
            <p class="text-[10px] text-muted-foreground">{{ t('account.regionsDesc') }}</p>
          </div>

          <div class="flex items-center justify-between rounded border border-border/60 px-3 py-2">
            <label class="text-xs font-medium">Mock</label>
            <Switch v-model:checked="form.is_mock" />
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

    <!-- Access Management Dialog -->
    <Dialog :open="showAccessDialog" @update:open="(val) => { showAccessDialog = val }">
      <DialogContent class="max-w-sm">
        <DialogHeader>
          <DialogTitle class="text-sm">{{ t('account.manageAccess') }}</DialogTitle>
          <DialogDescription v-if="accessAccount">{{ accessAccount.name }} ({{ accessAccount.account_id }})</DialogDescription>
        </DialogHeader>

        <div class="space-y-2.5">
          <!-- Add user — compact inline -->
          <div class="flex items-center gap-1.5">
            <select
              v-model="grantForm.user_id"
              class="flex h-7 flex-1 rounded bg-secondary/50 px-2 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 transition-colors"
            >
              <option value="" disabled>{{ t('account.selectUser') }}...</option>
              <option v-for="u in availableUsers" :key="u.id" :value="u.id">{{ u.username }}</option>
            </select>
            <select
              v-model="grantForm.role"
              class="flex h-7 w-20 shrink-0 rounded bg-secondary/50 px-1.5 text-[11px] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 transition-colors"
            >
              <option value="readonly">{{ t('account.accessReadonly') }}</option>
              <option value="admin">{{ t('account.accessAdmin') }}</option>
            </select>
            <Button
              size="icon-sm"
              variant="ghost"
              class="shrink-0 text-primary hover:text-primary"
              :disabled="!grantForm.user_id || granting"
              @click="grantAccess"
            >
              <Plus class="h-3.5 w-3.5" />
            </Button>
          </div>

          <!-- Authorized users list -->
          <div v-if="accessLoading" class="text-xs text-muted-foreground py-6 text-center">{{ t('common.loading') }}</div>
          <div v-else-if="accessUsers.length === 0" class="text-xs text-muted-foreground/50 py-6 text-center">{{ t('account.noAccessUsers') }}</div>
          <div v-else class="space-y-0.5 max-h-[240px] overflow-y-auto">
            <div
              v-for="u in accessUsers"
              :key="u.user_id"
              class="flex items-center justify-between rounded px-2 py-1.5 group hover:bg-secondary/40 transition-colors"
            >
              <div class="flex items-center gap-1.5 min-w-0">
                <span class="text-xs text-foreground/90 truncate">{{ u.username }}</span>
                <Badge :variant="u.role === 'admin' ? 'warning' : 'secondary'" class="text-[10px] shrink-0">
                  {{ u.role === 'admin' ? t('account.accessAdmin') : t('account.accessReadonly') }}
                </Badge>
              </div>
              <Button
                variant="ghost"
                size="icon-sm"
                class="text-muted-foreground hover:text-destructive opacity-0 group-hover:opacity-100 transition-opacity shrink-0 h-5 w-5"
                @click="revokeAccess(u.user_id)"
              >
                <X class="h-3 w-3" />
              </Button>
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>

    <!-- Setup Guide Dialog -->
    <Dialog :open="showGuideDialog" @update:open="(val) => { showGuideDialog = val }">
      <DialogContent class="max-w-2xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{{ t('account.setupGuideTitle') }}</DialogTitle>
          <DialogDescription>{{ t('account.setupGuideTitle') }}</DialogDescription>
        </DialogHeader>
        <div class="space-y-4 text-xs text-foreground/90">
          <div>
            <h3 class="text-sm font-semibold mb-2">1. Create IAM Role in target account</h3>
            <pre class="rounded bg-secondary/80 p-3 text-[11px] font-mono overflow-x-auto">aws iam create-role \
  --role-name OpenOpsRole \
  --assume-role-policy-document file://trust-policy.json</pre>
          </div>
          <div>
            <h3 class="text-sm font-semibold mb-2">2. Trust Policy (trust-policy.json)</h3>
            <p class="mb-2 text-muted-foreground">Allow OpenOps backend to assume this role:</p>
            <pre class="rounded bg-secondary/80 p-3 text-[11px] font-mono overflow-x-auto">{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Principal": {
      "AWS": "arn:aws:iam::&lt;OPENOPS_ACCOUNT&gt;:root"
    },
    "Action": "sts:AssumeRole"
  }]
}</pre>
            <p class="mt-2 text-muted-foreground">
              Replace <code class="text-foreground/80 bg-secondary px-1 rounded text-[10px]">&lt;OPENOPS_ACCOUNT&gt;</code>
              with the AWS account ID where OpenOps is deployed.
            </p>
          </div>
          <div>
            <h3 class="text-sm font-semibold mb-2">3. Attach Permissions</h3>
            <p class="mb-2 text-muted-foreground">Recommended: ReadOnlyAccess for resource scanning. Add more policies as needed.</p>
            <pre class="rounded bg-secondary/80 p-3 text-[11px] font-mono overflow-x-auto">aws iam attach-role-policy \
  --role-name OpenOpsRole \
  --policy-arn arn:aws:iam::aws:policy/ReadOnlyAccess</pre>
          </div>
          <div>
            <h3 class="text-sm font-semibold mb-2">4. Copy Role ARN</h3>
            <pre class="rounded bg-secondary/80 p-3 text-[11px] font-mono overflow-x-auto">aws iam get-role --role-name OpenOpsRole \
  --query 'Role.Arn' --output text</pre>
            <p class="mt-2 text-muted-foreground">
              Paste the output (e.g. <code class="text-foreground/80 bg-secondary px-1 rounded text-[10px]">arn:aws:iam::123456789012:role/OpenOpsRole</code>)
              into the Role ARN field.
            </p>
          </div>
        </div>
        <DialogFooter>
          <Button size="sm" @click="showGuideDialog = false">{{ t('common.close') }}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <ConfirmDialog
      :open="showDeleteDialog"
      :title="t('common.delete')"
      :description="t('common.confirmDelete')"
      :confirm-text="t('common.delete')"
      variant="destructive"
      @confirm="deleteAccount"
      @cancel="showDeleteDialog = false; deletingAccount = null"
    />
  </div>
</template>
