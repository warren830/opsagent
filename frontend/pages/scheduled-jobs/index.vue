<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Plus, Pencil, Trash2, Play, History, ChevronLeft, Loader2 } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { marked } from 'marked'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
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
  job_type: string
  skill_path: string | null
  skill_params: Record<string, unknown> | null
  targets: Record<string, unknown> | null
  visibility: string
  user_id: string | null
  created_by: string | null
  last_run_at: string | null
  created_at: string
}

interface JobRun {
  id: string
  job_id: string
  status: string
  trigger: string
  started_at: string | null
  finished_at: string | null
  duration_ms: number | null
  summary: string | null
  output: string | null
  result: Record<string, unknown> | null
  error: string | null
  exit_code: number | null
  created_at: string
}

const jobs = ref<ScheduledJob[]>([])
const loading = ref(true)
const saving = ref(false)
const runningJobIds = ref<Set<string>>(new Set())

const showFormDialog = ref(false)
const editingJob = ref<ScheduledJob | null>(null)
const showDeleteDialog = ref(false)
const deletingJob = ref<ScheduledJob | null>(null)

// Run history
const showRunsDialog = ref(false)
const runsJob = ref<ScheduledJob | null>(null)
const runs = ref<JobRun[]>([])
const runsLoading = ref(false)
const showRunDetail = ref(false)
const selectedRun = ref<JobRun | null>(null)

const form = ref({
  name: '',
  cron_expression: '',
  timezone: 'Asia/Tokyo',
  query: '',
  enabled: true,
  targets: '',
  visibility: 'public' as 'public' | 'private',
})

const isEditing = computed(() => !!editingJob.value)
const formTitle = computed(() => isEditing.value ? t('common.edit') : t('scheduledJob.create'))

const timezones = [
  'UTC', 'Asia/Tokyo', 'Asia/Shanghai', 'Asia/Seoul', 'Asia/Singapore',
  'America/New_York', 'America/Chicago', 'America/Los_Angeles',
  'Europe/London', 'Europe/Paris', 'Europe/Berlin',
]

const columns = computed(() => [
  { key: 'name', label: t('scheduledJob.name') },
  { key: 'cron_expression', label: t('scheduledJob.cron') },
  { key: 'timezone', label: t('scheduledJob.timezone') },
  { key: 'visibility', label: t('scheduledJob.visibility') },
  { key: 'enabled', label: t('scheduledJob.enabled') },
  { key: 'last_run_at', label: t('scheduledJob.lastRun') },
])

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

function formatDuration(ms: number | null): string {
  if (ms == null) return '-'
  if (ms < 1000) return `${ms}ms`
  const secs = Math.floor(ms / 1000)
  if (secs < 60) return `${secs}s`
  return `${Math.floor(secs / 60)}m${secs % 60}s`
}

function statusVariant(status: string): string {
  const map: Record<string, string> = { success: 'success', failed: 'destructive', running: 'warning', pending: 'secondary' }
  return map[status] || 'default'
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
  form.value = { name: '', cron_expression: '', timezone: 'Asia/Tokyo', query: '', enabled: true, targets: '', visibility: 'public' }
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
    visibility: (job.visibility === 'private' ? 'private' : 'public') as 'public' | 'private',
  }
  showFormDialog.value = true
}

function openDelete(job: ScheduledJob) {
  deletingJob.value = job
  showDeleteDialog.value = true
}

async function toggleEnabled(job: ScheduledJob) {
  try {
    await api.put(`/api/scheduled-jobs/${job.id}`, { enabled: !job.enabled })
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
    const payload: Record<string, unknown> = {
      name: form.value.name,
      cron_expression: form.value.cron_expression,
      timezone: form.value.timezone,
      job_type: 'agent',
      query: form.value.query || null,
      enabled: form.value.enabled,
      targets: targetsObj,
      visibility: form.value.visibility,
    }
    if (isEditing.value) {
      await api.put(`/api/scheduled-jobs/${editingJob.value!.id}`, payload)
    } else {
      await api.post('/api/scheduled-jobs', payload)
    }
    toast.success(t('common.success'))
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

async function triggerRun(job: ScheduledJob) {
  if (runningJobIds.value.has(job.id)) return
  runningJobIds.value.add(job.id)
  try {
    const run = await api.post<JobRun>(`/api/scheduled-jobs/${job.id}/run`, {})
    toast.success(t('scheduledJob.runTriggered'))
    pollRunStatus(job.id, run.id)
  } catch (err: unknown) {
    runningJobIds.value.delete(job.id)
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function pollRunStatus(jobId: string, runId: string) {
  const poll = setInterval(async () => {
    try {
      const run = await api.get<JobRun>(`/api/job-runs/${runId}`)
      if (run.status === 'success' || run.status === 'failed') {
        clearInterval(poll)
        runningJobIds.value.delete(jobId)
        await fetchJobs()
        if (run.status === 'success') {
          toast.success(`${t('scheduledJob.runCompleted')}`)
        } else {
          toast.error(`${t('scheduledJob.runFailed')}`)
        }
      }
    } catch {
      clearInterval(poll)
      runningJobIds.value.delete(jobId)
    }
  }, 3000)
}

async function openRuns(job: ScheduledJob) {
  runsJob.value = job
  showRunsDialog.value = true
  showRunDetail.value = false
  selectedRun.value = null
  await fetchRuns(job.id)
}

async function fetchRuns(jobId: string) {
  runsLoading.value = true
  try {
    runs.value = await api.get<JobRun[]>(`/api/scheduled-jobs/${jobId}/runs?limit=50`)
  } catch {
    runs.value = []
  } finally {
    runsLoading.value = false
  }
}

function viewRunDetail(run: JobRun) {
  selectedRun.value = run
  showRunDetail.value = true
}

function backToRuns() {
  showRunDetail.value = false
  selectedRun.value = null
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

      <template #cell-visibility="{ row }">
        <Badge
          :variant="(row as ScheduledJob).visibility === 'public' ? 'default' : 'secondary'"
          class="text-[10px]"
        >
          {{ (row as ScheduledJob).visibility === 'public' ? t('scheduledJob.public') : t('scheduledJob.private') }}
        </Badge>
      </template>

      <template #cell-enabled="{ row }">
        <Switch :checked="(row as ScheduledJob).enabled" @update:checked="toggleEnabled(row as ScheduledJob)" />
      </template>

      <template #cell-last_run_at="{ row }">
        <span class="text-muted-foreground">{{ formatDate((row as ScheduledJob).last_run_at) }}</span>
      </template>

      <template #actions="{ row }">
        <Button
          variant="ghost"
          size="icon-sm"
          :disabled="runningJobIds.has((row as ScheduledJob).id)"
          :title="t('scheduledJob.runNow')"
          @click="triggerRun(row as ScheduledJob)"
        >
          <Loader2 v-if="runningJobIds.has((row as ScheduledJob).id)" class="h-3 w-3 animate-spin text-sky-500" />
          <Play v-else class="h-3 w-3" />
        </Button>
        <Button variant="ghost" size="icon-sm" :title="t('scheduledJob.viewRuns')" @click="openRuns(row as ScheduledJob)">
          <History class="h-3 w-3" />
        </Button>
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
            <Select v-model="form.timezone">
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem v-for="tz in timezones" :key="tz" :value="tz">{{ tz }}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('scheduledJob.query') }}</label>
            <Textarea v-model="form.query" :placeholder="t('scheduledJob.query')" class="min-h-[80px]" />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('scheduledJob.visibility') }}</label>
            <div class="flex rounded-md border border-border/60 overflow-hidden">
              <button
                type="button"
                class="flex-1 py-1.5 text-xs font-medium transition-colors"
                :class="form.visibility === 'public'
                  ? 'bg-primary/10 text-primary'
                  : 'text-muted-foreground hover:bg-secondary/50'"
                @click="form.visibility = 'public'"
              >
                {{ t('scheduledJob.public') }}
              </button>
              <button
                type="button"
                class="flex-1 py-1.5 text-xs font-medium transition-colors"
                :class="form.visibility === 'private'
                  ? 'bg-primary/10 text-primary'
                  : 'text-muted-foreground hover:bg-secondary/50'"
                @click="form.visibility = 'private'"
              >
                {{ t('scheduledJob.private') }}
              </button>
            </div>
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

    <!-- Run History Dialog -->
    <Dialog :open="showRunsDialog" @update:open="(val) => { showRunsDialog = val }">
      <DialogContent class="max-w-2xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle class="text-sm">
            <template v-if="showRunDetail && selectedRun">
              <button class="inline-flex items-center gap-1 text-muted-foreground hover:text-foreground mr-2" @click="backToRuns">
                <ChevronLeft class="h-3.5 w-3.5" />
              </button>
              {{ t('scheduledJob.output') }}
            </template>
            <template v-else>
              {{ t('scheduledJob.runs') }} — {{ runsJob?.name }}
            </template>
          </DialogTitle>
          <DialogDescription>{{ runsJob?.name }}</DialogDescription>
        </DialogHeader>

        <!-- Run Detail View -->
        <template v-if="showRunDetail && selectedRun">
          <div class="space-y-3">
            <div class="flex items-center gap-2 text-xs text-muted-foreground">
              <Badge :variant="statusVariant(selectedRun.status) as any">{{ selectedRun.status }}</Badge>
              <span>{{ formatDate(selectedRun.started_at) }}</span>
              <span v-if="selectedRun.duration_ms != null">{{ formatDuration(selectedRun.duration_ms) }}</span>
              <Badge v-if="selectedRun.trigger === 'manual'" variant="secondary">manual</Badge>
            </div>

            <div v-if="selectedRun.summary" class="rounded bg-secondary/50 px-3 py-2 text-xs text-foreground/80">
              {{ selectedRun.summary }}
            </div>

            <!-- Structured result (builtin) -->
            <div v-if="selectedRun.result" class="rounded bg-secondary/50 px-3 py-2">
              <pre class="text-[11px] font-mono text-muted-foreground whitespace-pre-wrap">{{ JSON.stringify(selectedRun.result, null, 2) }}</pre>
            </div>

            <!-- Agent/Skill output (markdown) -->
            <div v-if="selectedRun.output" class="prose-preview rounded border border-border/40 bg-background p-3 max-h-[400px] overflow-y-auto text-xs text-foreground/90" v-html="marked.parse(selectedRun.output)" />

            <!-- Error -->
            <div v-if="selectedRun.error" class="rounded bg-destructive/10 border border-destructive/30 px-3 py-2">
              <pre class="text-[11px] font-mono text-destructive whitespace-pre-wrap">{{ selectedRun.error }}</pre>
            </div>
          </div>
        </template>

        <!-- Runs List View -->
        <template v-else>
          <div v-if="runsLoading" class="text-xs text-muted-foreground py-8 text-center">{{ t('common.loading') }}</div>
          <div v-else-if="runs.length === 0" class="text-xs text-muted-foreground/50 py-8 text-center">{{ t('scheduledJob.noRuns') }}</div>
          <div v-else class="space-y-0.5 max-h-[400px] overflow-y-auto">
            <div
              v-for="run in runs"
              :key="run.id"
              class="flex items-center justify-between rounded px-2.5 py-2 cursor-pointer group hover:bg-secondary/40 transition-colors"
              @click="viewRunDetail(run)"
            >
              <div class="flex items-center gap-2 min-w-0">
                <Badge :variant="statusVariant(run.status) as any" class="text-[10px] shrink-0">{{ run.status }}</Badge>
                <span class="text-[11px] text-muted-foreground shrink-0">{{ formatDate(run.started_at) }}</span>
                <span v-if="run.duration_ms != null" class="text-[10px] text-muted-foreground/60 shrink-0">{{ formatDuration(run.duration_ms) }}</span>
                <Badge v-if="run.trigger === 'manual'" variant="secondary" class="text-[10px] shrink-0">manual</Badge>
              </div>
              <span class="text-[11px] text-foreground/70 truncate max-w-[200px]">{{ run.summary || run.error || '-' }}</span>
            </div>
          </div>
        </template>
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
