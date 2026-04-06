<script setup lang="ts">
import { ref, onMounted, computed, nextTick, watch } from 'vue'
import { Plus, Pencil, Trash2, Eye } from 'lucide-vue-next'
import { marked } from 'marked'
import mermaid from 'mermaid'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
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

interface KnowledgeFile {
  id: string
  filename: string
  content: string | null
  size_bytes: number
  mime_type: string
  account_id: string | null
  tenant_id: string | null
  updated_at: string
}

interface AccessibleAccount {
  id: string
  provider: string
  name: string
  account_id: string | null
  writable: boolean
}

const files = ref<KnowledgeFile[]>([])
const accounts = ref<AccessibleAccount[]>([])
const loading = ref(true)
const saving = ref(false)

const showFormDialog = ref(false)
const editingFile = ref<KnowledgeFile | null>(null)
const showDeleteDialog = ref(false)
const deletingFile = ref<KnowledgeFile | null>(null)
const showPreviewDialog = ref(false)
const previewFile = ref<KnowledgeFile | null>(null)

const form = ref({
  filename: '',
  content: '',
  account_id: '',
})

const isEditing = computed(() => !!editingFile.value)
const formTitle = computed(() => isEditing.value ? t('common.edit') : t('knowledge.newFile'))

const columns = computed(() => [
  { key: 'filename', label: t('knowledge.filename') },
  { key: 'size_bytes', label: t('knowledge.size') },
  { key: 'account_id', label: t('knowledge.account') },
  { key: 'updated_at', label: t('knowledge.updatedAt') },
])

function getAccountName(accountId: string | null): string {
  if (!accountId) return '-'
  const account = accounts.value.find((a) => a.id === accountId)
  return account ? `${account.name} (${account.provider})` : accountId
}

function canWriteFile(file: KnowledgeFile): boolean {
  if (!file.account_id) return true
  const account = accounts.value.find((a) => a.id === file.account_id)
  return account?.writable ?? false
}

const hasAnyWritable = computed(() => accounts.value.some((a) => a.writable))

async function fetchAccounts() {
  try {
    accounts.value = await api.get<AccessibleAccount[]>('/api/my/accessible-accounts')
  } catch { /* ignore */ }
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function formatDate(dateStr: string): string {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })
}

// Mermaid init
mermaid.initialize({ startOnLoad: false, theme: 'dark', themeVariables: { primaryColor: '#FF6600', primaryTextColor: '#e0e0e0', lineColor: '#555' } })

/** Custom marked renderer: mermaid code blocks → <pre class="mermaid"> */
const renderer = new marked.Renderer()
const origCode = renderer.code
renderer.code = function ({ text, lang }: { text: string; lang?: string | undefined; escaped?: boolean }) {
  if (lang === 'mermaid') {
    return `<pre class="mermaid">${text}</pre>`
  }
  return origCode.call(this, { text, lang, escaped: false })
}
marked.use({ renderer })

/** Render markdown to HTML using marked (mermaid blocks preserved) */
function renderMarkdown(md: string): string {
  if (!md) return ''
  return marked.parse(md, { async: false }) as string
}

/** After preview dialog opens, render mermaid diagrams */
async function renderMermaidDiagrams() {
  await nextTick()
  try {
    await mermaid.run({ querySelector: '.prose-preview .mermaid' })
  } catch { /* ignore render errors */ }
}

// Trigger mermaid rendering when preview dialog opens
watch(showPreviewDialog, (val) => { if (val) renderMermaidDiagrams() })

async function fetchFiles() {
  loading.value = true
  try {
    files.value = await api.get<KnowledgeFile[]>('/api/knowledge')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  fetchFiles()
  fetchAccounts()
})

function openCreate() {
  editingFile.value = null
  form.value = { filename: '', content: '', account_id: accounts.value[0]?.id || '' }
  showFormDialog.value = true
}

function openEdit(file: KnowledgeFile) {
  editingFile.value = file
  form.value = { filename: file.filename, content: file.content || '', account_id: file.account_id || '' }
  showFormDialog.value = true
}

function openPreview(file: KnowledgeFile) {
  previewFile.value = file
  showPreviewDialog.value = true
}

function openDelete(file: KnowledgeFile) {
  deletingFile.value = file
  showDeleteDialog.value = true
}

async function saveFile() {
  saving.value = true
  try {
    const payload: Record<string, unknown> = {
      filename: form.value.filename,
      content: form.value.content,
      account_id: form.value.account_id === '__none__' ? null : (form.value.account_id || null),
    }
    if (isEditing.value) {
      await api.put(`/api/knowledge/${editingFile.value!.id}`, payload)
      toast.success(t('common.success'))
    } else {
      await api.post('/api/knowledge', payload)
      toast.success(t('common.success'))
    }
    showFormDialog.value = false
    await fetchFiles()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteFile() {
  if (!deletingFile.value) return
  try {
    await api.del(`/api/knowledge/${deletingFile.value.id}`)
    toast.success(t('common.success'))
    showDeleteDialog.value = false
    deletingFile.value = null
    await fetchFiles()
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
      <h1 class="text-base font-semibold text-foreground">{{ t('knowledge.title') }}</h1>
      <Button v-if="hasAnyWritable" size="sm" @click="openCreate">
        <Plus class="h-3.5 w-3.5" />
        {{ t('knowledge.newFile') }}
      </Button>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="files" :loading="loading">
      <template #cell-filename="{ row }">
        <button
          class="font-medium text-foreground hover:text-primary transition-colors cursor-pointer text-left"
          @click="openPreview(row as KnowledgeFile)"
        >
          {{ (row as KnowledgeFile).filename }}
        </button>
      </template>

      <template #cell-size_bytes="{ row }">
        <span class="text-muted-foreground">{{ formatSize((row as KnowledgeFile).size_bytes) }}</span>
      </template>

      <template #cell-account_id="{ row }">
        <span class="text-muted-foreground">{{ getAccountName((row as KnowledgeFile).account_id) }}</span>
      </template>

      <template #cell-updated_at="{ row }">
        <span class="text-muted-foreground">{{ formatDate((row as KnowledgeFile).updated_at) }}</span>
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" @click="openPreview(row as KnowledgeFile)">
          <Eye class="h-3 w-3" />
        </Button>
        <Button v-if="canWriteFile(row as KnowledgeFile)" variant="ghost" size="icon-sm" @click="openEdit(row as KnowledgeFile)">
          <Pencil class="h-3 w-3" />
        </Button>
        <Button v-if="canWriteFile(row as KnowledgeFile)" variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(row as KnowledgeFile)">
          <Trash2 class="h-3 w-3" />
        </Button>
      </template>
    </DataTable>

    <!-- Preview Dialog -->
    <Dialog :open="showPreviewDialog" @update:open="(val) => { showPreviewDialog = val }">
      <DialogContent class="max-w-4xl max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle class="flex items-center gap-2">
            <Eye class="h-4 w-4 text-muted-foreground" />
            {{ previewFile?.filename }}
          </DialogTitle>
          <DialogDescription>
            {{ formatSize(previewFile?.size_bytes || 0) }} · {{ previewFile?.mime_type }}
          </DialogDescription>
        </DialogHeader>
        <div v-if="previewFile?.content" class="prose-preview max-w-none text-xs text-foreground/90" v-html="renderMarkdown(previewFile.content)" />
        <p v-else class="text-xs text-muted-foreground italic">{{ t('common.noData') }}</p>
        <DialogFooter class="gap-1.5">
          <Button v-if="previewFile && canWriteFile(previewFile)" variant="outline" size="sm" @click="() => { showPreviewDialog = false; if (previewFile) openEdit(previewFile) }">
            <Pencil class="h-3 w-3" />
            {{ t('common.edit') }}
          </Button>
          <Button size="sm" @click="showPreviewDialog = false">{{ t('common.close') }}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <!-- Create/Edit Dialog -->
    <Dialog :open="showFormDialog" @update:open="(val) => { showFormDialog = val }">
      <DialogContent class="max-w-lg">
        <DialogHeader>
          <DialogTitle>{{ formTitle }}</DialogTitle>
          <DialogDescription>{{ formTitle }}</DialogDescription>
        </DialogHeader>

        <form class="space-y-3" @submit.prevent="saveFile">
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('knowledge.account') }}</label>
            <Select v-model="form.account_id">
              <SelectTrigger><SelectValue placeholder="-" /></SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">-</SelectItem>
                <SelectItem v-for="acc in accounts" :key="acc.id" :value="acc.id">{{ acc.name }} ({{ acc.provider }})</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('knowledge.filename') }}</label>
            <Input v-model="form.filename" placeholder="runbook.md" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">Content (Markdown)</label>
            <Textarea v-model="form.content" placeholder="# Title..." class="font-mono min-h-[200px]" />
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
      @confirm="deleteFile"
      @cancel="showDeleteDialog = false; deletingFile = null"
    />
  </div>
</template>

<style scoped>
.prose-preview :deep(h1) { font-size: 1.125rem; font-weight: 700; margin-top: 1.25rem; margin-bottom: 0.5rem; color: var(--foreground); }
.prose-preview :deep(h2) { font-size: 0.875rem; font-weight: 600; margin-top: 1rem; margin-bottom: 0.375rem; color: var(--foreground); }
.prose-preview :deep(h3) { font-size: 0.8125rem; font-weight: 600; margin-top: 0.75rem; margin-bottom: 0.25rem; color: var(--foreground); }
.prose-preview :deep(h4) { font-size: 0.75rem; font-weight: 600; margin-top: 0.5rem; margin-bottom: 0.25rem; color: var(--foreground); }
.prose-preview :deep(p) { margin: 0.375rem 0; line-height: 1.6; }
.prose-preview :deep(ul),
.prose-preview :deep(ol) { padding-left: 1.25rem; margin: 0.375rem 0; }
.prose-preview :deep(li) { margin: 0.125rem 0; }
.prose-preview :deep(code) { font-size: 0.6875rem; padding: 0.125rem 0.25rem; border-radius: 0.25rem; background: hsl(var(--secondary)); font-family: ui-monospace, monospace; }
.prose-preview :deep(pre) { margin: 0.5rem 0; padding: 0.75rem; border-radius: 0.375rem; background: hsl(var(--secondary) / 0.8); overflow-x: auto; }
.prose-preview :deep(pre code) { padding: 0; background: none; font-size: 0.6875rem; line-height: 1.5; }
.prose-preview :deep(table) { width: 100%; border-collapse: collapse; margin: 0.5rem 0; font-size: 0.6875rem; }
.prose-preview :deep(th) { text-align: left; padding: 0.375rem 0.5rem; border-bottom: 1px solid hsl(var(--border) / 0.6); font-weight: 600; color: hsl(var(--muted-foreground)); }
.prose-preview :deep(td) { padding: 0.375rem 0.5rem; border-bottom: 1px solid hsl(var(--border) / 0.3); }
.prose-preview :deep(hr) { border: none; border-top: 1px solid hsl(var(--border) / 0.4); margin: 0.75rem 0; }
.prose-preview :deep(a) { color: hsl(var(--primary)); text-decoration: underline; }
.prose-preview :deep(strong) { font-weight: 600; }
.prose-preview :deep(blockquote) { border-left: 3px solid hsl(var(--border)); padding-left: 0.75rem; margin: 0.5rem 0; color: hsl(var(--muted-foreground)); }
.prose-preview :deep(pre.mermaid) { background: transparent; padding: 0.5rem 0; text-align: center; overflow-x: auto; }
.prose-preview :deep(pre.mermaid svg) { max-width: 100%; height: auto; }
</style>
