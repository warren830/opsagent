<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Plus, Pencil, Trash2 } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
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
import DataTable from '@/components/shared/DataTable.vue'
import ConfirmDialog from '@/components/shared/ConfirmDialog.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

interface ScheduledJob {
  id: string
  name: string
  cron_expression: string
  timezone: string
  query: string | null
  enabled: boolean
  targets: Record<string, unknown> | null
  last_run_at: string | null
  created_at: string
}

const jobs = ref<ScheduledJob[]>([])
const loading = ref(true)
const saving = ref(false)

const showFormDialog = ref(false)
const editingJob = ref<ScheduledJob | null>(null)
const showDeleteDialog = ref(false)
const deletingJob = ref<ScheduledJob | null>(null)

const form = ref({
  name: '',
  cron_expression: '',
  timezone: 'Asia/Tokyo',
  query: '',
  enabled: true,
  targets: '',
})

const isEditing = computed(() => !!editingJob.value)
const formTitle = computed(() => isEditing.value ? t('common.edit') : t('scheduledJob.create'))

const timezones = [
  'UTC',
  'Asia/Tokyo',
  'Asia/Shanghai',
  'Asia/Seoul',
  'Asia/Singapore',
  'America/New_York',
  'America/Chicago',
  'America/Los_Angeles',
  'Europe/London',
  'Europe/Paris',
  'Europe/Berlin',
]

const columns = computed(() => [
  { key: 'name', label: t('scheduledJob.name') },
  { key: 'cron_expression', label: t('scheduledJob.cron') },
  { key: 'timezone', label: t('scheduledJob.timezone') },
  { key: 'enabled', label: t('scheduledJob.enabled') },
  { key: 'last_run_at', label: t('scheduledJob.lastRun') },
])

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

async function fetchJobs() {
  loading.value = true
  try {
    jobs.value = await api.get<ScheduledJob[]>('/api/scheduled-jobs')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(() => { fetchJobs() })

function openCreate() {
  editingJob.value = null
  form.value = { name: '', cron_expression: '', timezone: 'Asia/Tokyo', query: '', enabled: true, targets: '' }
  showFormDialog.value = true
}

function openEdit(job: ScheduledJob) {
  editingJob.value = job
  form.value = {
    name: job.name,
    cron_expression: job.cron_expression,
    timezone: job.timezone,
    query: job.query || '',
    enabled: job.enabled,
    targets: job.targets ? JSON.stringify(job.targets, null, 2) : '',
  }
  showFormDialog.value = true
}

function openDelete(job: ScheduledJob) {
  deletingJob.value = job
  showDeleteDialog.value = true
}

async function toggleEnabled(job: ScheduledJob) {
  try {
    await api.put(`/api/scheduled-jobs/${job.id}`, { ...job, enabled: !job.enabled })
    await fetchJobs()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function saveJob() {
  saving.value = true
  try {
    let targetsObj = null
    if (form.value.targets.trim()) {
      targetsObj = JSON.parse(form.value.targets)
    }
    const payload = {
      name: form.value.name,
      cron_expression: form.value.cron_expression,
      timezone: form.value.timezone,
      query: form.value.query || null,
      enabled: form.value.enabled,
      targets: targetsObj,
    }
    if (isEditing.value) {
      await api.put(`/api/scheduled-jobs/${editingJob.value!.id}`, payload)
      toast.success(t('common.success'))
    } else {
      await api.post('/api/scheduled-jobs', payload)
      toast.success(t('common.success'))
    }
    showFormDialog.value = false
    await fetchJobs()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteJob() {
  if (!deletingJob.value) return
  try {
    await api.del(`/api/scheduled-jobs/${deletingJob.value.id}`)
    toast.success(t('common.success'))
    showDeleteDialog.value = false
    deletingJob.value = null
    await fetchJobs()
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
      <h1 class="text-base font-semibold text-foreground">{{ t('scheduledJob.title') }}</h1>
      <Button size="sm" @click="openCreate">
        <Plus class="h-3.5 w-3.5" />
        {{ t('scheduledJob.create') }}
      </Button>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="jobs" :loading="loading">
      <template #cell-name="{ row }">
        <span class="font-medium text-foreground">{{ (row as ScheduledJob).name }}</span>
      </template>

      <template #cell-cron_expression="{ row }">
        <code class="rounded-sm bg-secondary px-1.5 py-0.5 text-[11px] font-mono text-muted-foreground">{{ (row as ScheduledJob).cron_expression }}</code>
      </template>

      <template #cell-timezone="{ row }">
        <span class="text-muted-foreground">{{ (row as ScheduledJob).timezone }}</span>
      </template>

      <template #cell-enabled="{ row }">
        <Switch :checked="(row as ScheduledJob).enabled" @update:checked="toggleEnabled(row as ScheduledJob)" />
      </template>

      <template #cell-last_run_at="{ row }">
        <span class="text-muted-foreground">{{ formatDate((row as ScheduledJob).last_run_at) }}</span>
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" @click="openEdit(row as ScheduledJob)">
          <Pencil class="h-3 w-3" />
        </Button>
        <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(row as ScheduledJob)">
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

        <form class="space-y-3" @submit.prevent="saveJob">
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('scheduledJob.name') }}</label>
            <Input v-model="form.name" :placeholder="t('scheduledJob.name')" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('scheduledJob.cron') }}</label>
            <Input v-model="form.cron_expression" placeholder="0 9 * * *" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('scheduledJob.timezone') }}</label>
            <select
              v-model="form.timezone"
              class="flex h-8 w-full rounded border border-border/60 bg-secondary/50 px-2.5 py-1.5 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 transition-colors"
            >
              <option v-for="tz in timezones" :key="tz" :value="tz">{{ tz }}</option>
            </select>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('scheduledJob.query') }}</label>
            <Textarea v-model="form.query" :placeholder="t('scheduledJob.query')" class="min-h-[80px]" />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('scheduledJob.targets') }} (JSON)</label>
            <Textarea v-model="form.targets" placeholder='{"channel_ids": [...]}' class="font-mono min-h-[60px]" />
          </div>

          <div class="flex items-center justify-between rounded border border-border/60 px-3 py-2">
            <label class="text-xs font-medium">{{ t('scheduledJob.enabled') }}</label>
            <Switch v-model:checked="form.enabled" />
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
      @confirm="deleteJob"
      @cancel="showDeleteDialog = false; deletingJob = null"
    />
  </div>
</template>
