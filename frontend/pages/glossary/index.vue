<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue'
import { Plus, Pencil, Trash2, Eye } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
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

interface GlossaryItem {
  id: string
  term: string
  full_name: string | null
  description: string | null
  aliases: string[]
  account_id: string | null
  tenant_id: string | null
  created_at: string
}

interface AccessibleAccount {
  id: string
  provider: string
  name: string
  account_id: string | null
  writable: boolean
}

const items = ref<GlossaryItem[]>([])
const accounts = ref<AccessibleAccount[]>([])
const loading = ref(true)
const saving = ref(false)
const search = ref('')

const showFormDialog = ref(false)
const editingItem = ref<GlossaryItem | null>(null)
const showPreviewDialog = ref(false)
const previewItem = ref<GlossaryItem | null>(null)
const showDeleteDialog = ref(false)
const deletingItem = ref<GlossaryItem | null>(null)

const form = ref({
  term: '',
  full_name: '',
  description: '',
  aliases: '',
  account_id: '',
})

const isEditing = computed(() => !!editingItem.value)
const formTitle = computed(() => isEditing.value ? t('common.edit') : t('glossary.addTerm'))

const columns = computed(() => [
  { key: 'term', label: t('glossary.term') },
  { key: 'full_name', label: t('glossary.fullName') },
  { key: 'aliases', label: t('glossary.aliases') },
  { key: 'account_id', label: t('glossary.account') },
  { key: 'description', label: t('glossary.description') },
])

function getAccountName(accountId: string | null): string {
  if (!accountId) return '-'
  const account = accounts.value.find((a) => a.id === accountId)
  return account ? `${account.name} (${account.provider})` : accountId
}

function canWriteItem(item: GlossaryItem): boolean {
  if (!item.account_id) return true
  const account = accounts.value.find((a) => a.id === item.account_id)
  return account?.writable ?? false
}

const hasAnyWritable = computed(() => accounts.value.some((a) => a.writable))

async function fetchAccounts() {
  try {
    accounts.value = await api.get<AccessibleAccount[]>('/api/my/accessible-accounts')
  } catch { /* ignore */ }
}

function truncate(str: string | null, len: number): string {
  if (!str) return '-'
  return str.length > len ? str.slice(0, len) + '...' : str
}

async function fetchItems() {
  loading.value = true
  try {
    const params = search.value ? `?q=${encodeURIComponent(search.value)}` : ''
    items.value = await api.get<GlossaryItem[]>(`/api/glossary${params}`)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

let searchTimeout: ReturnType<typeof setTimeout> | null = null
watch(search, () => {
  if (searchTimeout) clearTimeout(searchTimeout)
  searchTimeout = setTimeout(() => { fetchItems() }, 300)
})

onMounted(() => {
  fetchItems()
  fetchAccounts()
})

function openCreate() {
  editingItem.value = null
  form.value = { term: '', full_name: '', description: '', aliases: '', account_id: accounts.value[0]?.id || '' }
  showFormDialog.value = true
}

function openEdit(item: GlossaryItem) {
  editingItem.value = item
  form.value = {
    term: item.term,
    full_name: item.full_name || '',
    description: item.description || '',
    aliases: (item.aliases || []).join(', '),
    account_id: item.account_id || '',
  }
  showFormDialog.value = true
}

function openPreview(item: GlossaryItem) {
  previewItem.value = item
  showPreviewDialog.value = true
}

function openDelete(item: GlossaryItem) {
  deletingItem.value = item
  showDeleteDialog.value = true
}

async function saveItem() {
  saving.value = true
  try {
    const payload: Record<string, unknown> = {
      term: form.value.term,
      full_name: form.value.full_name || null,
      description: form.value.description || null,
      aliases: form.value.aliases.split(',').map((s) => s.trim()).filter(Boolean),
      account_id: form.value.account_id || null,
    }
    if (isEditing.value) {
      await api.put(`/api/glossary/${editingItem.value!.id}`, payload)
      toast.success(t('common.success'))
    } else {
      await api.post('/api/glossary', payload)
      toast.success(t('common.success'))
    }
    showFormDialog.value = false
    await fetchItems()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function deleteItem() {
  if (!deletingItem.value) return
  try {
    await api.del(`/api/glossary/${deletingItem.value.id}`)
    toast.success(t('common.success'))
    showDeleteDialog.value = false
    deletingItem.value = null
    await fetchItems()
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
      <h1 class="text-base font-semibold text-foreground">{{ t('glossary.title') }}</h1>
      <Button v-if="hasAnyWritable" size="sm" @click="openCreate">
        <Plus class="h-3.5 w-3.5" />
        {{ t('glossary.addTerm') }}
      </Button>
    </div>

    <!-- Search -->
    <div class="flex items-center gap-1.5">
      <Input v-model="search" :placeholder="t('common.search')" class="max-w-[240px]" />
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="items" :loading="loading">
      <template #cell-term="{ row }">
        <span class="font-medium text-foreground">{{ (row as GlossaryItem).term }}</span>
      </template>

      <template #cell-full_name="{ row }">
        <span class="text-muted-foreground">{{ (row as GlossaryItem).full_name || '-' }}</span>
      </template>

      <template #cell-aliases="{ row }">
        <span class="text-muted-foreground">{{ ((row as GlossaryItem).aliases || []).join(', ') || '-' }}</span>
      </template>

      <template #cell-account_id="{ row }">
        <span class="text-muted-foreground">{{ getAccountName((row as GlossaryItem).account_id) }}</span>
      </template>

      <template #cell-description="{ row }">
        <span class="text-muted-foreground">{{ truncate((row as GlossaryItem).description, 60) }}</span>
      </template>

      <template #actions="{ row }">
        <Button variant="ghost" size="icon-sm" :title="t('glossary.preview')" @click="openPreview(row as GlossaryItem)">
          <Eye class="h-3 w-3" />
        </Button>
        <Button v-if="canWriteItem(row as GlossaryItem)" variant="ghost" size="icon-sm" @click="openEdit(row as GlossaryItem)">
          <Pencil class="h-3 w-3" />
        </Button>
        <Button v-if="canWriteItem(row as GlossaryItem)" variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(row as GlossaryItem)">
          <Trash2 class="h-3 w-3" />
        </Button>
      </template>
    </DataTable>

    <!-- Preview Dialog -->
    <Dialog :open="showPreviewDialog" @update:open="(val) => { showPreviewDialog = val }">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>{{ previewItem?.term }}</DialogTitle>
          <DialogDescription v-if="previewItem?.full_name">{{ previewItem.full_name }}</DialogDescription>
        </DialogHeader>
        <div v-if="previewItem" class="space-y-3 text-sm">
          <div v-if="previewItem.description" class="space-y-1">
            <label class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">{{ t('glossary.description') }}</label>
            <p class="text-foreground/90 leading-relaxed">{{ previewItem.description }}</p>
          </div>
          <div v-if="previewItem.aliases && previewItem.aliases.length > 0" class="space-y-1">
            <label class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">{{ t('glossary.aliases') }}</label>
            <div class="flex flex-wrap gap-1">
              <Badge v-for="alias in previewItem.aliases" :key="alias" variant="secondary" class="text-[10px]">{{ alias }}</Badge>
            </div>
          </div>
          <div v-if="previewItem.account_id" class="space-y-1">
            <label class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">{{ t('glossary.account') }}</label>
            <span class="text-xs text-foreground/80">{{ getAccountName(previewItem.account_id) }}</span>
          </div>
        </div>
        <DialogFooter class="pt-1">
          <Button variant="outline" size="sm" @click="showPreviewDialog = false">{{ t('common.close') }}</Button>
          <Button size="sm" @click="showPreviewDialog = false; openEdit(previewItem!)">
            <Pencil class="h-3 w-3" />
            {{ t('common.edit') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <!-- Create/Edit Dialog -->
    <Dialog :open="showFormDialog" @update:open="(val) => { showFormDialog = val }">
      <DialogContent class="max-w-sm">
        <DialogHeader>
          <DialogTitle>{{ formTitle }}</DialogTitle>
          <DialogDescription>{{ isEditing ? t('glossary.editDescription') : t('glossary.addDescription') }}</DialogDescription>
        </DialogHeader>

        <form class="space-y-3" @submit.prevent="saveItem">
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('glossary.account') }}</label>
            <select
              v-model="form.account_id"
              class="flex h-8 w-full rounded border border-border/60 bg-secondary/50 px-2.5 py-1.5 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 transition-colors"
            >
              <option value="">-</option>
              <option v-for="acc in accounts" :key="acc.id" :value="acc.id">{{ acc.name }} ({{ acc.provider }})</option>
            </select>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('glossary.term') }}</label>
            <Input v-model="form.term" :placeholder="t('glossary.term')" required />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('glossary.fullName') }}</label>
            <Input v-model="form.full_name" :placeholder="t('glossary.fullName')" />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('glossary.description') }}</label>
            <Textarea v-model="form.description" :placeholder="t('glossary.description')" class="min-h-[80px]" />
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('glossary.aliases') }}</label>
            <Input v-model="form.aliases" placeholder="alias1, alias2, ..." />
            <p class="text-[10px] text-muted-foreground">Comma-separated</p>
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
      @confirm="deleteItem"
      @cancel="showDeleteDialog = false; deletingItem = null"
    />
  </div>
</template>
