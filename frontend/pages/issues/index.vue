<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Search } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
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

interface Issue {
  id: string
  title: string
  description: string | null
  source: string
  severity: 'critical' | 'high' | 'medium' | 'low'
  status: 'open' | 'investigating' | 'rca_done' | 'resolved'
  rca_result: string | null
  timeline: { time: string; event: string }[] | null
  created_at: string
}

const issues = ref<Issue[]>([])
const loading = ref(true)
const activeFilter = ref<string>('')
const triggeringRca = ref(false)

const showDetailDialog = ref(false)
const selectedIssue = ref<Issue | null>(null)

const filters = [
  { value: '', label: () => t('issue.all') },
  { value: 'open', label: () => t('issue.open') },
  { value: 'investigating', label: () => t('issue.investigating') },
  { value: 'rca_done', label: () => t('issue.rcaDone') },
  { value: 'resolved', label: () => t('issue.resolved') },
]

const filteredIssues = computed(() => {
  if (!activeFilter.value) return issues.value.filter((i) => i.status !== 'resolved')
  return issues.value.filter((i) => i.status === activeFilter.value)
})

const columns = computed(() => [
  { key: 'title', label: t('issue.title') },
  { key: 'source', label: t('issue.source') },
  { key: 'severity', label: t('issue.severity') },
  { key: 'status', label: t('cluster.status') },
  { key: 'created_at', label: t('tenant.createdAt') },
])

function severityVariant(severity: string): 'destructive' | 'warning' | 'info' | 'secondary' {
  switch (severity) {
    case 'critical': return 'destructive'
    case 'high': return 'warning'
    case 'medium': return 'info'
    case 'low': return 'secondary'
    default: return 'secondary'
  }
}

function statusVariant(status: string): 'warning' | 'info' | 'success' | 'secondary' {
  switch (status) {
    case 'open': return 'warning'
    case 'investigating': return 'info'
    case 'rca_done': return 'success'
    case 'resolved': return 'secondary'
    default: return 'secondary'
  }
}

function statusLabel(status: string): string {
  const map: Record<string, string> = {
    open: t('issue.open'),
    investigating: t('issue.investigating'),
    rca_done: t('issue.rcaDone'),
    resolved: t('issue.resolved'),
  }
  return map[status] || status
}

function formatDate(dateStr: string): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

async function fetchIssues() {
  loading.value = true
  try {
    const params = new URLSearchParams()
    if (activeFilter.value) params.set('status', activeFilter.value)
    const qs = params.toString()
    issues.value = await api.get<Issue[]>(`/api/issues${qs ? '?' + qs : ''}`)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

function openDetail(issue: Issue) {
  selectedIssue.value = issue
  showDetailDialog.value = true
}

async function triggerRca() {
  if (!selectedIssue.value) return
  triggeringRca.value = true
  try {
    await api.post(`/api/issues/${selectedIssue.value.id}/rca`)
    toast.success(t('common.success'))
    // Refresh the issue detail
    const updated = await api.get<Issue>(`/api/issues/${selectedIssue.value.id}`)
    selectedIssue.value = updated
    await fetchIssues()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    triggeringRca.value = false
  }
}

onMounted(() => { fetchIssues() })
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('issue.title') }}</h1>
    </div>

    <!-- Status filter tabs -->
    <div class="flex items-center gap-1.5">
      <Button
        v-for="f in filters"
        :key="f.value"
        size="sm"
        :variant="activeFilter === f.value ? 'default' : 'outline'"
        @click="activeFilter = f.value; fetchIssues()"
      >
        {{ f.label() }}
      </Button>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="filteredIssues" :loading="loading">
      <template #cell-title="{ row }">
        <button class="font-medium text-foreground hover:text-primary transition-colors text-left" @click="openDetail(row as Issue)">
          {{ (row as Issue).title }}
        </button>
      </template>

      <template #cell-source="{ row }">
        <Badge variant="secondary">{{ (row as Issue).source }}</Badge>
      </template>

      <template #cell-severity="{ row }">
        <Badge :variant="severityVariant((row as Issue).severity)">
          {{ (row as Issue).severity }}
        </Badge>
      </template>

      <template #cell-status="{ row }">
        <Badge :variant="statusVariant((row as Issue).status)">
          {{ statusLabel((row as Issue).status) }}
        </Badge>
      </template>

      <template #cell-created_at="{ row }">
        <span class="text-muted-foreground">{{ formatDate((row as Issue).created_at) }}</span>
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" @click="openDetail(row as Issue)">
          <Search class="h-3 w-3" />
        </Button>
      </template>
    </DataTable>

    <!-- Detail Dialog -->
    <Dialog :open="showDetailDialog" @update:open="(val) => { showDetailDialog = val }">
      <DialogContent class="max-w-lg">
        <DialogHeader>
          <DialogTitle>{{ t('issue.detail') }}</DialogTitle>
          <DialogDescription>{{ selectedIssue?.title }}</DialogDescription>
        </DialogHeader>

        <div v-if="selectedIssue" class="space-y-3">
          <!-- Meta -->
          <div class="flex items-center gap-1.5">
            <Badge :variant="severityVariant(selectedIssue.severity)">{{ selectedIssue.severity }}</Badge>
            <Badge :variant="statusVariant(selectedIssue.status)">{{ statusLabel(selectedIssue.status) }}</Badge>
            <Badge variant="secondary">{{ selectedIssue.source }}</Badge>
          </div>

          <!-- Description -->
          <div v-if="selectedIssue.description" class="space-y-1">
            <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('glossary.description') }}</label>
            <p class="text-xs text-foreground whitespace-pre-wrap rounded border border-border/60 bg-secondary/30 p-2">{{ selectedIssue.description }}</p>
          </div>

          <!-- RCA Result -->
          <div v-if="selectedIssue.rca_result" class="space-y-1">
            <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('issue.rcaProgress') }}</label>
            <pre class="text-xs text-foreground whitespace-pre-wrap rounded border border-border/60 bg-secondary/30 p-2 max-h-[200px] overflow-y-auto">{{ selectedIssue.rca_result }}</pre>
          </div>

          <!-- Timeline -->
          <div v-if="selectedIssue.timeline && selectedIssue.timeline.length > 0" class="space-y-1">
            <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('issue.timeline') }}</label>
            <div class="space-y-1 rounded border border-border/60 bg-secondary/30 p-2">
              <div
                v-for="(entry, idx) in selectedIssue.timeline"
                :key="idx"
                class="flex items-start gap-2 text-xs"
              >
                <span class="text-muted-foreground whitespace-nowrap shrink-0">{{ entry.time }}</span>
                <span class="text-foreground">{{ entry.event }}</span>
              </div>
            </div>
          </div>

          <!-- Trigger RCA -->
          <div v-if="selectedIssue.status !== 'resolved' && selectedIssue.status !== 'rca_done'">
            <Button size="sm" :disabled="triggeringRca" @click="triggerRca">
              {{ triggeringRca ? t('common.loading') : t('issue.rcaProgress') }}
            </Button>
          </div>
        </div>

        <DialogFooter class="gap-1.5 pt-1">
          <Button variant="outline" size="sm" @click="showDetailDialog = false">{{ t('common.close') }}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
