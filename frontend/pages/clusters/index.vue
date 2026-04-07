<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { Plus, Pencil, Trash2, RefreshCw, Server, Search } from 'lucide-vue-next'
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
  region: string | null
  role_name: string | null
  description: string | null
  status: string
  is_discovered: boolean
  last_seen_at: string | null
  config: Record<string, unknown>
  created_at: string
  updated_at: string
}

interface DiscoverResult {
  discovered: number
  errors: string[]
}

const clusters = ref<Cluster[]>([])
const loading = ref(true)
const saving = ref(false)
const discovering = ref(false)

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

// Split clusters into static and discovered
const staticClusters = computed(() => clusters.value.filter(c => !c.is_discovered))
const discoveredClusters = computed(() => clusters.value.filter(c => c.is_discovered))

const staticColumns = computed(() => [
  { key: 'name', label: t('tenant.name') },
  { key: 'cloud', label: t('cluster.cloud') },
  { key: 'cluster_type', label: t('cluster.type') },
  { key: 'region', label: t('cluster.region') },
  { key: 'status', label: t('cluster.status') },
])

function statusVariant(status: string): 'success' | 'warning' | 'destructive' | 'secondary' {
  switch (status?.toUpperCase()) {
    case 'ACTIVE': return 'success'
    case 'CREATING': case 'UPDATING': return 'warning'
    case 'FAILED': case 'DELETING': case 'DELETED': return 'destructive'
    default: return 'secondary'
  }
}

function statusDot(status: string): string {
  switch (status?.toUpperCase()) {
    case 'ACTIVE': return 'bg-emerald-400'
    case 'CREATING': case 'UPDATING': return 'bg-amber-400'
    case 'FAILED': case 'DELETING': return 'bg-red-400'
    default: return 'bg-zinc-400'
  }
}

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
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

async function discoverClusters() {
  discovering.value = true
  try {
    const result = await api.post<DiscoverResult>('/api/clusters/discover', {})
    if (result.discovered > 0) {
      toast.success(t('cluster.discoverSuccess', { count: result.discovered }))
    } else {
      toast.info(t('cluster.discoverEmpty'))
    }
    if (result.errors?.length) {
      console.warn('Cluster discover errors:', result.errors)
      for (const e of result.errors.slice(0, 3)) {
        toast.warning(e, { duration: 8000 })
      }
    }
    await fetchClusters()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    discovering.value = false
  }
}

// Auto-refresh every 10 minutes
let refreshTimer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  fetchClusters()
  refreshTimer = setInterval(() => {
    discoverClusters()
  }, 10 * 60 * 1000)
})

onUnmounted(() => {
  if (refreshTimer) clearInterval(refreshTimer)
})

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
    region: cluster.region || '',
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
  <div class="space-y-6">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('cluster.title') }}</h1>
    </div>

    <!-- ═══════ AUTO DISCOVERY ═══════ -->
    <div class="rounded-lg border border-border/60 bg-card/30">
      <div class="flex items-center justify-between px-4 py-3 border-b border-border/40">
        <div class="flex items-center gap-2">
          <Search class="h-3.5 w-3.5 text-primary" />
          <span class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{{ t('cluster.autoDiscovery') }}</span>
        </div>
        <Button
          variant="outline"
          size="sm"
          :disabled="discovering"
          @click="discoverClusters"
        >
          <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': discovering }" />
          {{ discovering ? t('cluster.refreshing') : t('cluster.refreshNow') }}
        </Button>
      </div>

      <div class="px-4 py-3">
        <p class="text-[11px] text-muted-foreground/70">
          Scans all accessible AWS accounts for EKS clusters. Discovered clusters appear below.
        </p>
      </div>
    </div>

    <!-- ═══════ STATIC CLUSTERS ═══════ -->
    <div class="space-y-3">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <Server class="h-3.5 w-3.5 text-muted-foreground" />
          <span class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{{ t('cluster.staticClusters') }}</span>
          <Badge variant="secondary" class="h-4 min-w-4 justify-center text-[9px] px-1">{{ staticClusters.length }}</Badge>
        </div>
        <Button size="sm" @click="openCreate">
          <Plus class="h-3.5 w-3.5" />
          {{ t('cluster.addCluster') }}
        </Button>
      </div>

      <DataTable v-if="staticClusters.length > 0 || loading" :columns="staticColumns" :data="staticClusters" :loading="loading">
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
          <span class="text-muted-foreground">{{ (row as Cluster).region || '-' }}</span>
        </template>

        <template #cell-status="{ row }">
          <Badge :variant="statusVariant((row as Cluster).status)">
            {{ (row as Cluster).status }}
          </Badge>
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

      <div v-if="!loading && staticClusters.length === 0" class="rounded-lg border border-dashed border-border/50 py-8 text-center">
        <p class="text-xs text-muted-foreground/60">{{ t('cluster.noStaticClusters') }}</p>
      </div>
    </div>

    <!-- ═══════ DISCOVERED CLUSTERS ═══════ -->
    <div class="space-y-3">
      <div class="flex items-center gap-2">
        <RefreshCw class="h-3.5 w-3.5 text-muted-foreground" />
        <span class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{{ t('cluster.discoveredClusters') }}</span>
        <Badge variant="secondary" class="h-4 min-w-4 justify-center text-[9px] px-1">{{ discoveredClusters.length }}</Badge>
      </div>

      <!-- Cluster cards grid -->
      <div v-if="discoveredClusters.length > 0" class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
        <div
          v-for="cluster in discoveredClusters"
          :key="cluster.id"
          class="group relative rounded-lg border border-border/50 bg-card/40 p-4 transition-all duration-200 hover:border-border hover:bg-card/60"
        >
          <!-- Status dot + name -->
          <div class="flex items-start justify-between mb-3">
            <div class="flex items-center gap-2 min-w-0">
              <span class="relative flex h-2 w-2 shrink-0">
                <span
                  class="absolute inline-flex h-full w-full rounded-full opacity-75"
                  :class="[statusDot(cluster.status), cluster.status?.toUpperCase() === 'ACTIVE' ? 'animate-ping' : '']"
                />
                <span class="relative inline-flex rounded-full h-2 w-2" :class="statusDot(cluster.status)" />
              </span>
              <span class="font-medium text-sm text-foreground truncate">{{ cluster.name }}</span>
            </div>
            <Badge :variant="statusVariant(cluster.status)" class="text-[9px] shrink-0">{{ cluster.status }}</Badge>
          </div>

          <!-- Badges row -->
          <div class="flex flex-wrap gap-1.5 mb-3">
            <Badge variant="info" class="text-[9px]">{{ cluster.cluster_type.toUpperCase() }}</Badge>
            <Badge v-if="cluster.account_id" variant="secondary" class="text-[9px]">{{ cluster.account_id }}</Badge>
            <Badge v-if="cluster.region" variant="secondary" class="text-[9px]">{{ cluster.region }}</Badge>
            <Badge v-if="cluster.config?.version" variant="secondary" class="text-[9px]">v{{ cluster.config.version }}</Badge>
          </div>

          <!-- Meta info -->
          <div class="space-y-1 text-[10px] text-muted-foreground/70">
            <div v-if="cluster.config?.endpoint" class="flex items-center gap-1.5 truncate">
              <span class="text-muted-foreground/50 w-12 shrink-0">{{ t('cluster.endpoint') }}</span>
              <span class="truncate">{{ cluster.config.endpoint }}</span>
            </div>
            <div v-if="cluster.last_seen_at" class="flex items-center gap-1.5">
              <span class="text-muted-foreground/50 w-12 shrink-0">{{ t('cluster.lastSeen') }}</span>
              <span>{{ formatDate(cluster.last_seen_at) }}</span>
            </div>
          </div>

          <!-- Hover actions -->
          <div class="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity flex gap-0.5">
            <Button variant="ghost" size="icon-sm" class="h-6 w-6" @click="openEdit(cluster)">
              <Pencil class="h-3 w-3" />
            </Button>
            <Button variant="ghost" size="icon-sm" class="h-6 w-6 text-destructive hover:text-destructive" @click="openDelete(cluster)">
              <Trash2 class="h-3 w-3" />
            </Button>
          </div>
        </div>
      </div>

      <div v-else-if="!loading" class="rounded-lg border border-dashed border-border/50 py-8 text-center">
        <p class="text-xs text-muted-foreground/60">{{ t('cluster.noDiscoveredClusters') }}</p>
      </div>
    </div>

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
