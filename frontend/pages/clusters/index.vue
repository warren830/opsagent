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
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from '@/components/ui/dialog'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import DataTable from '@/components/shared/DataTable.vue'
import ConfirmDialog from '@/components/shared/ConfirmDialog.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

interface Cluster {
  id: string
  name: string
  cloud: string
  cluster_type: string
  account_id: string | null
  region: string
  role_name: string | null
  description: string | null
  status: string
  is_discovered: boolean
  created_at: string
}

const clusters = ref<Cluster[]>([])
const loading = ref(true)
const saving = ref(false)

const showFormDialog = ref(false)
const editingCluster = ref<Cluster | null>(null)
const showDeleteDialog = ref(false)
const deletingCluster = ref<Cluster | null>(null)

const form = ref({
  name: '',
  cloud: 'aws',
  cluster_type: 'eks',
  account_id: '',
  region: '',
  role_name: '',
  description: '',
})

const isEditing = computed(() => !!editingCluster.value)
const formTitle = computed(() => isEditing.value ? t('common.edit') : t('cluster.addCluster'))

const columns = computed(() => [
  { key: 'name', label: t('tenant.name') },
  { key: 'cloud', label: t('cluster.cloud') },
  { key: 'cluster_type', label: t('cluster.type') },
  { key: 'region', label: t('cluster.region') },
  { key: 'status', label: t('cluster.status') },
  { key: 'is_discovered', label: t('cluster.autoDiscovery') },
])

function statusVariant(status: string): 'success' | 'warning' | 'destructive' | 'secondary' {
  switch (status) {
    case 'active': case 'ACTIVE': return 'success'
    case 'creating': case 'updating': return 'warning'
    case 'failed': case 'deleting': return 'destructive'
    default: return 'secondary'
  }
}

async function fetchClusters() {
  loading.value = true
  try {
    clusters.value = await api.get<Cluster[]>('/api/clusters')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(() => { fetchClusters() })

function openCreate() {
  editingCluster.value = null
  form.value = { name: '', cloud: 'aws', cluster_type: 'eks', account_id: '', region: '', role_name: '', description: '' }
  showFormDialog.value = true
}

function openEdit(cluster: Cluster) {
  editingCluster.value = cluster
  form.value = {
    name: cluster.name,
    cloud: cluster.cloud,
    cluster_type: cluster.cluster_type,
    account_id: cluster.account_id || '',
    region: cluster.region,
    role_name: cluster.role_name || '',
    description: cluster.description || '',
  }
  showFormDialog.value = true
}

function openDelete(cluster: Cluster) {
  deletingCluster.value = cluster
  showDeleteDialog.value = true
}

async function saveCluster() {
  saving.value = true
  try {
    const payload = {
      name: form.value.name,
      cloud: form.value.cloud,
      cluster_type: form.value.cluster_type,
      account_id: form.value.account_id || null,
      region: form.value.region,
      role_name: form.value.role_name || null,
      description: form.value.description || null,
    }
    if (isEditing.value) {
      await api.put(`/api/clusters/${editingCluster.value!.id}`, payload)
      toast.success(t('common.success'))
    } else {
      await api.post('/api/clusters', payload)
      toast.success(t('common.success'))
    }
    showFormDialog.value = false
    await fetchClusters()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteCluster() {
  if (!deletingCluster.value) return
  try {
    await api.del(`/api/clusters/${deletingCluster.value.id}`)
    toast.success(t('common.success'))
    showDeleteDialog.value = false
    deletingCluster.value = null
    await fetchClusters()
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
      <h1 class="text-base font-semibold text-foreground">{{ t('cluster.title') }}</h1>
      <Button size="sm" @click="openCreate">
        <Plus class="h-3.5 w-3.5" />
        {{ t('cluster.addCluster') }}
      </Button>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="clusters" :loading="loading">
      <template #cell-name="{ row }">
        <span class="font-medium text-foreground">{{ (row as Cluster).name }}</span>
      </template>

      <template #cell-cloud="{ row }">
        <Badge variant="info">{{ (row as Cluster).cloud }}</Badge>
      </template>

      <template #cell-cluster_type="{ row }">
        <code class="rounded-sm bg-secondary px-1.5 py-0.5 text-[11px] font-mono text-muted-foreground">{{ (row as Cluster).cluster_type }}</code>
      </template>

      <template #cell-region="{ row }">
        <span class="text-muted-foreground">{{ (row as Cluster).region }}</span>
      </template>

      <template #cell-status="{ row }">
        <Badge :variant="statusVariant((row as Cluster).status)">
          {{ (row as Cluster).status }}
        </Badge>
      </template>

      <template #cell-is_discovered="{ row }">
        <Badge v-if="(row as Cluster).is_discovered" variant="success">Auto</Badge>
        <span v-else class="text-muted-foreground">Static</span>
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" @click="openEdit(row as Cluster)">
          <Pencil class="h-3 w-3" />
        </Button>
        <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(row as Cluster)">
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

        <form class="space-y-3" @submit.prevent="saveCluster">
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('tenant.name') }}</label>
            <Input v-model="form.name" placeholder="my-cluster" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('cluster.cloud') }}</label>
            <Select v-model="form.cloud">
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="aws">AWS</SelectItem>
                <SelectItem value="alicloud">Alicloud</SelectItem>
                <SelectItem value="azure">Azure</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('cluster.type') }}</label>
            <Select v-model="form.cluster_type">
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="eks">EKS</SelectItem>
                <SelectItem value="ack">ACK</SelectItem>
                <SelectItem value="aks">AKS</SelectItem>
                <SelectItem value="k3s">K3s</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('account.accountId') }}</label>
            <Input v-model="form.account_id" placeholder="123456789012" />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('cluster.region') }}</label>
            <Input v-model="form.region" placeholder="ap-northeast-1" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('account.roleName') }}</label>
            <Input v-model="form.role_name" placeholder="eks-access-role" />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('pipeline.description') }}</label>
            <Input v-model="form.description" />
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
      @confirm="deleteCluster"
      @cancel="showDeleteDialog = false; deletingCluster = null"
    />
  </div>
</template>
