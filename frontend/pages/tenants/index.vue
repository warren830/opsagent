<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Plus, Pencil, Trash2 } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import DataTable from '@/components/shared/DataTable.vue'
import ConfirmDialog from '@/components/shared/ConfirmDialog.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

interface Tenant {
  id: string
  name: string
  slug: string
  created_at: string
}

const tenants = ref<Tenant[]>([])
const loading = ref(true)
const saving = ref(false)

const showFormDialog = ref(false)
const editingTenant = ref<Tenant | null>(null)
const showDeleteDialog = ref(false)
const deletingTenant = ref<Tenant | null>(null)

const form = ref({
  name: '',
})

const isEditing = computed(() => !!editingTenant.value)
const formTitle = computed(() => isEditing.value ? t('tenant.editTenant') : t('tenant.create'))

const columns = computed(() => [
  { key: 'name', label: t('tenant.name') },
  { key: 'slug', label: t('tenant.slug') },
  { key: 'created_at', label: t('tenant.createdAt') },
])

async function fetchTenants() {
  loading.value = true
  try {
    tenants.value = await api.get<Tenant[]>('/api/tenants')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(() => { fetchTenants() })

function formatDate(dateStr: string): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })
}

function autoSlug(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '')
}

function openCreate() {
  editingTenant.value = null
  form.value = { name: '' }
  showFormDialog.value = true
}

function openEdit(tenant: Tenant) {
  editingTenant.value = tenant
  form.value = { name: tenant.name }
  showFormDialog.value = true
}

function openDelete(tenant: Tenant) {
  deletingTenant.value = tenant
  showDeleteDialog.value = true
}

async function saveTenant() {
  saving.value = true
  try {
    const slug = isEditing.value ? editingTenant.value!.slug : autoSlug(form.value.name)
    const payload = { name: form.value.name, slug }
    if (isEditing.value) {
      await api.put(`/api/tenants/${editingTenant.value!.id}`, payload)
      toast.success(t('tenant.updateSuccess'))
    } else {
      await api.post('/api/tenants', payload)
      toast.success(t('tenant.createSuccess'))
    }
    showFormDialog.value = false
    await fetchTenants()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteTenant() {
  if (!deletingTenant.value) return
  try {
    await api.del(`/api/tenants/${deletingTenant.value.id}`)
    toast.success(t('tenant.deleteSuccess'))
    showDeleteDialog.value = false
    deletingTenant.value = null
    await fetchTenants()
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
      <h1 class="text-base font-semibold text-foreground">{{ t('tenant.title') }}</h1>
      <Button size="sm" @click="openCreate">
        <Plus class="h-3.5 w-3.5" />
        {{ t('tenant.create') }}
      </Button>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="tenants" :loading="loading">
      <template #cell-name="{ row }">
        <span class="font-medium text-foreground">{{ (row as Tenant).name }}</span>
      </template>

      <template #cell-slug="{ row }">
        <code class="rounded-sm bg-secondary px-1.5 py-0.5 text-[11px] font-mono text-muted-foreground">{{ (row as Tenant).slug }}</code>
      </template>

      <template #cell-created_at="{ row }">
        <span class="text-muted-foreground">{{ formatDate((row as Tenant).created_at) }}</span>
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" @click="openEdit(row as Tenant)">
          <Pencil class="h-3 w-3" />
        </Button>
        <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(row as Tenant)">
          <Trash2 class="h-3 w-3" />
        </Button>
      </template>
    </DataTable>

    <!-- Create/Edit Dialog -->
    <Dialog :open="showFormDialog" @update:open="(val) => { showFormDialog = val }">
      <DialogContent class="max-w-sm">
        <DialogHeader>
          <DialogTitle>{{ formTitle }}</DialogTitle>
        </DialogHeader>

        <form class="space-y-3" @submit.prevent="saveTenant">
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('tenant.name') }}</label>
            <Input v-model="form.name" :placeholder="t('tenant.name')" required />
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
      :title="t('tenant.deleteTitle')"
      :description="t('tenant.confirmDelete')"
      :confirm-text="t('common.delete')"
      variant="destructive"
      @confirm="deleteTenant"
      @cancel="showDeleteDialog = false; deletingTenant = null"
    />
  </div>
</template>
