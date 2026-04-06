<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Plus, Pencil, Trash2 } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
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

interface PipelineRepo {
  id: string
  repo_id: string
  name: string
  repository: string
  description: string | null
  enabled: boolean
  token: string | null
  created_at: string
}

const repos = ref<PipelineRepo[]>([])
const loading = ref(true)
const saving = ref(false)

const showFormDialog = ref(false)
const editingRepo = ref<PipelineRepo | null>(null)
const showDeleteDialog = ref(false)
const deletingRepo = ref<PipelineRepo | null>(null)

const form = ref({
  repo_id: '',
  name: '',
  repository: '',
  description: '',
  enabled: true,
  token: '',
})

const isEditing = computed(() => !!editingRepo.value)
const formTitle = computed(() => isEditing.value ? t('common.edit') : t('pipeline.addRepo'))

const columns = computed(() => [
  { key: 'name', label: t('pipeline.repoName') },
  { key: 'repository', label: t('pipeline.repository') },
  { key: 'repo_id', label: t('pipeline.repoId') },
  { key: 'enabled', label: t('pipeline.enabled') },
])

async function fetchRepos() {
  loading.value = true
  try {
    repos.value = await api.get<PipelineRepo[]>('/api/pipeline/repos')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(() => { fetchRepos() })

function openCreate() {
  editingRepo.value = null
  form.value = { repo_id: '', name: '', repository: '', description: '', enabled: true, token: '' }
  showFormDialog.value = true
}

function openEdit(repo: PipelineRepo) {
  editingRepo.value = repo
  form.value = {
    repo_id: repo.repo_id,
    name: repo.name,
    repository: repo.repository,
    description: repo.description || '',
    enabled: repo.enabled,
    token: '',
  }
  showFormDialog.value = true
}

function openDelete(repo: PipelineRepo) {
  deletingRepo.value = repo
  showDeleteDialog.value = true
}

async function toggleEnabled(repo: PipelineRepo) {
  try {
    await api.put(`/api/pipeline/repos/${repo.id}`, { ...repo, enabled: !repo.enabled })
    await fetchRepos()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

async function saveRepo() {
  saving.value = true
  try {
    const payload: Record<string, unknown> = {
      repo_id: form.value.repo_id,
      name: form.value.name,
      repository: form.value.repository,
      description: form.value.description || null,
      enabled: form.value.enabled,
    }
    if (form.value.token) payload.token = form.value.token
    if (isEditing.value) {
      await api.put(`/api/pipeline/repos/${editingRepo.value!.id}`, payload)
      toast.success(t('common.success'))
    } else {
      await api.post('/api/pipeline/repos', payload)
      toast.success(t('common.success'))
    }
    showFormDialog.value = false
    await fetchRepos()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteRepo() {
  if (!deletingRepo.value) return
  try {
    await api.del(`/api/pipeline/repos/${deletingRepo.value.id}`)
    toast.success(t('common.success'))
    showDeleteDialog.value = false
    deletingRepo.value = null
    await fetchRepos()
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
      <h1 class="text-base font-semibold text-foreground">{{ t('pipeline.title') }}</h1>
      <Button size="sm" @click="openCreate">
        <Plus class="h-3.5 w-3.5" />
        {{ t('pipeline.addRepo') }}
      </Button>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="repos" :loading="loading">
      <template #cell-name="{ row }">
        <span class="font-medium text-foreground">{{ (row as PipelineRepo).name }}</span>
      </template>

      <template #cell-repository="{ row }">
        <span class="text-muted-foreground text-[11px] font-mono">{{ (row as PipelineRepo).repository }}</span>
      </template>

      <template #cell-repo_id="{ row }">
        <code class="rounded-sm bg-secondary px-1.5 py-0.5 text-[11px] font-mono text-muted-foreground">{{ (row as PipelineRepo).repo_id }}</code>
      </template>

      <template #cell-enabled="{ row }">
        <Switch :checked="(row as PipelineRepo).enabled" @update:checked="toggleEnabled(row as PipelineRepo)" />
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" @click="openEdit(row as PipelineRepo)">
          <Pencil class="h-3 w-3" />
        </Button>
        <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(row as PipelineRepo)">
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

        <form class="space-y-3" @submit.prevent="saveRepo">
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('pipeline.repoId') }}</label>
            <Input v-model="form.repo_id" placeholder="org/repo" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('pipeline.repoName') }}</label>
            <Input v-model="form.name" :placeholder="t('pipeline.repoName')" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('pipeline.repository') }}</label>
            <Input v-model="form.repository" placeholder="https://github.com/org/repo" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('pipeline.description') }}</label>
            <Input v-model="form.description" :placeholder="t('pipeline.description')" />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('pipeline.token') }}</label>
            <Input v-model="form.token" type="password" placeholder="ghp_..." />
          </div>

          <div class="flex items-center justify-between rounded border border-border/60 px-3 py-2">
            <label class="text-xs font-medium">{{ t('pipeline.enabled') }}</label>
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
      @confirm="deleteRepo"
      @cancel="showDeleteDialog = false; deletingRepo = null"
    />
  </div>
</template>
