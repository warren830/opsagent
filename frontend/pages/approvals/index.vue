<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Check, X } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import DataTable from '@/components/shared/DataTable.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

interface Approval {
  id: string
  command: string
  requested_by: string
  status: 'pending' | 'approved' | 'rejected' | 'executed' | 'failed'
  created_at: string
}

const approvals = ref<Approval[]>([])
const loading = ref(true)
const activeFilter = ref<string>('pending')

const filters = ['pending', 'approved', 'rejected', 'executed', 'failed'] as const

const filteredApprovals = computed(() => {
  if (!activeFilter.value) return approvals.value
  return approvals.value.filter((a) => a.status === activeFilter.value)
})

const filterCounts = computed(() => {
  const counts: Record<string, number> = {}
  for (const f of filters) {
    counts[f] = approvals.value.filter((a) => a.status === f).length
  }
  return counts
})

const columns = computed(() => [
  { key: 'command', label: t('approval.command') },
  { key: 'requested_by', label: t('approval.requestedBy') },
  { key: 'status', label: t('user.status') },
  { key: 'created_at', label: t('approval.requestedAt') },
])

function statusVariant(status: string): 'warning' | 'success' | 'destructive' | 'info' | 'secondary' {
  switch (status) {
    case 'pending': return 'warning'
    case 'approved': return 'success'
    case 'rejected': return 'destructive'
    case 'executed': return 'info'
    case 'failed': return 'destructive'
    default: return 'secondary'
  }
}

function statusLabel(status: string): string {
  const map: Record<string, string> = {
    pending: t('approval.pending'),
    approved: t('approval.approved'),
    rejected: t('approval.rejected'),
    executed: t('approval.executed'),
    failed: t('approval.failed'),
  }
  return map[status] || status
}

function formatDate(dateStr: string): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

async function fetchApprovals() {
  loading.value = true
  try {
    approvals.value = await api.get<Approval[]>('/api/approvals')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

async function approve(id: string) {
  try {
    await api.post(`/api/approvals/${id}/approve`)
    toast.success(t('common.success'))
    await fetchApprovals()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function reject(id: string) {
  try {
    await api.post(`/api/approvals/${id}/reject`)
    toast.success(t('common.success'))
    await fetchApprovals()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

onMounted(() => { fetchApprovals() })
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('approval.title') }}</h1>
    </div>

    <!-- Status filter tabs -->
    <div class="flex items-center gap-1.5">
      <Button
        v-for="f in filters"
        :key="f"
        size="sm"
        :variant="activeFilter === f ? 'default' : 'outline'"
        @click="activeFilter = f"
      >
        {{ statusLabel(f) }}
        <Badge variant="secondary" class="ml-1.5">{{ filterCounts[f] }}</Badge>
      </Button>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="filteredApprovals" :loading="loading">
      <template #cell-command="{ row }">
        <code class="rounded-sm bg-secondary px-1.5 py-0.5 text-[11px] font-mono text-foreground">{{ (row as Approval).command }}</code>
      </template>

      <template #cell-requested_by="{ row }">
        <span class="text-muted-foreground">{{ (row as Approval).requested_by }}</span>
      </template>

      <template #cell-status="{ row }">
        <Badge :variant="statusVariant((row as Approval).status)">
          {{ statusLabel((row as Approval).status) }}
        </Badge>
      </template>

      <template #cell-created_at="{ row }">
        <span class="text-muted-foreground">{{ formatDate((row as Approval).created_at) }}</span>
      </template>

      <template #actions="{ row }">
        <template v-if="(row as Approval).status === 'pending'">
          <Button variant="ghost" size="icon-sm" class="text-success hover:text-success" @click="approve((row as Approval).id)">
            <Check class="h-3 w-3" />
          </Button>
          <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="reject((row as Approval).id)">
            <X class="h-3 w-3" />
          </Button>
        </template>
      </template>
    </DataTable>
  </div>
</template>
