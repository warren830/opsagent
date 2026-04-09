<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { Plus, Trash2, RefreshCw, GitBranch, Download, Search, Loader2, Check } from 'lucide-vue-next'
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
import DataTable from '@/components/shared/DataTable.vue'
import ConfirmDialog from '@/components/shared/ConfirmDialog.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const skillsVersion = useState('skillsVersion', () => 0)

interface Skill {
  id: string
  name: string
  description: string | null
  git_url: string | null
  repo_path: string | null
  visibility: string
  enabled: boolean
  tenant_id: string | null
  user_id: string | null
  created_by: string | null
  created_at: string
  updated_at: string
}

interface DiscoveredSkill {
  name: string
  description: string | null
  path: string
}

const skills = ref<Skill[]>([])
const loading = ref(true)
const installing = ref(false)
const updatingSkillIds = ref<Set<string>>(new Set())

const showAddDialog = ref(false)
const showDeleteDialog = ref(false)
const deletingSkill = ref<Skill | null>(null)
const gitUrl = ref('')
const visibility = ref('private')
const searchQuery = ref('')

// Discover state
const discovering = ref(false)
const discoveredSkills = ref<DiscoveredSkill[]>([])
const selectedSkills = ref<Set<string>>(new Set())
const discoverStep = ref<'url' | 'select'>('url')

const filteredSkills = computed(() => {
  if (!searchQuery.value.trim()) return skills.value
  const q = searchQuery.value.toLowerCase()
  return skills.value.filter(s =>
    s.name.toLowerCase().includes(q) ||
    (s.description || '').toLowerCase().includes(q) ||
    (s.git_url || '').toLowerCase().includes(q),
  )
})

const columns = computed(() => [
  { key: 'name', label: t('skill.name') },
  { key: 'description', label: t('skill.description') },
  { key: 'visibility', label: t('skill.visibility') },
  { key: 'source', label: t('skill.source') },
  { key: 'updated_at', label: t('skill.updatedAt') },
])

function formatTime(iso: string): string {
  const d = new Date(iso)
  const now = new Date()
  const diffMs = now.getTime() - d.getTime()
  const diffMin = Math.floor(diffMs / 60000)
  const diffHr = Math.floor(diffMs / 3600000)
  const diffDay = Math.floor(diffMs / 86400000)
  if (diffMin < 1) return t('skill.justNow')
  if (diffMin < 60) return `${diffMin}m ago`
  if (diffHr < 24) return `${diffHr}h ago`
  if (diffDay < 7) return `${diffDay}d ago`
  return d.toLocaleDateString()
}

async function fetchSkills() {
  loading.value = true
  try {
    skills.value = await api.get<Skill[]>('/api/skills')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

onMounted(() => { fetchSkills() })

function openAdd() {
  gitUrl.value = ''
  visibility.value = 'private'
  discoveredSkills.value = []
  selectedSkills.value = new Set()
  discoverStep.value = 'url'
  showAddDialog.value = true
}

function closeAdd() {
  showAddDialog.value = false
  discovering.value = false
  installing.value = false
}

function openDelete(skill: Skill) {
  deletingSkill.value = skill
  showDeleteDialog.value = true
}

async function discoverSkills() {
  if (!gitUrl.value.trim()) return
  discovering.value = true
  try {
    const resp = await api.post<{ git_url: string; skills: DiscoveredSkill[] }>(
      '/api/skills/discover',
      { git_url: gitUrl.value.trim() },
    )
    discoveredSkills.value = resp.skills
    // Select all by default
    selectedSkills.value = new Set(resp.skills.map(s => s.path))
    discoverStep.value = 'select'
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    discovering.value = false
  }
}

function toggleSkill(path: string) {
  const s = new Set(selectedSkills.value)
  if (s.has(path)) {
    s.delete(path)
  } else {
    s.add(path)
  }
  selectedSkills.value = s
}

function toggleAll() {
  if (selectedSkills.value.size === discoveredSkills.value.length) {
    selectedSkills.value = new Set()
  } else {
    selectedSkills.value = new Set(discoveredSkills.value.map(s => s.path))
  }
}

async function installSelected() {
  if (selectedSkills.value.size === 0) return
  installing.value = true
  try {
    await api.post('/api/skills', {
      git_url: gitUrl.value.trim(),
      selected: Array.from(selectedSkills.value),
      visibility: visibility.value,
    })
    toast.success(t('skill.addSuccess'))
    closeAdd()
    await fetchSkills()
    skillsVersion.value++
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    installing.value = false
  }
}

async function updateSkill(skill: Skill) {
  updatingSkillIds.value.add(skill.id)
  try {
    await api.put(`/api/skills/${skill.id}`, {})
    toast.success(t('skill.updateSuccess'))
    await fetchSkills()
    skillsVersion.value++
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    updatingSkillIds.value.delete(skill.id)
  }
}

async function deleteSkill() {
  if (!deletingSkill.value) return
  try {
    await api.del(`/api/skills/${deletingSkill.value.id}`)
    toast.success(t('common.success'))
    showDeleteDialog.value = false
    deletingSkill.value = null
    await fetchSkills()
    skillsVersion.value++
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.isComposing) {
    e.preventDefault()
    if (discoverStep.value === 'url') {
      discoverSkills()
    }
  }
}

function truncateDesc(desc: string | null, len: number): string {
  if (!desc) return ''
  return desc.length > len ? desc.slice(0, len) + '...' : desc
}
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <h1 class="text-base font-semibold text-foreground">{{ t('skill.title') }}</h1>
        <Badge variant="secondary" class="text-[10px]">{{ skills.length }}</Badge>
      </div>
      <div class="flex items-center gap-1.5">
        <Button size="sm" @click="openAdd">
          <Plus class="h-3.5 w-3.5" />
          {{ t('skill.addFromGit') }}
        </Button>
      </div>
    </div>

    <!-- Search + Info -->
    <div class="flex items-center gap-3">
      <div class="relative flex-1 max-w-xs">
        <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground/40" />
        <Input
          v-model="searchQuery"
          :placeholder="t('common.search')"
          class="pl-8 h-8"
        />
      </div>
      <div class="flex items-center gap-1.5 text-[11px] text-muted-foreground/50">
        <GitBranch class="h-3 w-3 shrink-0" />
        <span class="hidden sm:inline">{{ t('skill.cliInfo') }}</span>
      </div>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="filteredSkills" :loading="loading">
      <template #cell-name="{ row }">
        <span class="font-medium text-foreground">{{ (row as Skill).name }}</span>
      </template>

      <template #cell-description="{ row }">
        <span class="text-muted-foreground text-[11px]">
          {{ truncateDesc((row as Skill).description, 80) || '-' }}
        </span>
      </template>

      <template #cell-visibility="{ row }">
        <Badge
          :variant="(row as Skill).visibility === 'public' ? 'default' : 'secondary'"
          class="text-[10px]"
        >
          {{ (row as Skill).visibility === 'public' ? t('skill.public') : t('skill.private') }}
        </Badge>
      </template>

      <template #cell-source="{ row }">
        <span class="text-[11px] font-mono text-muted-foreground/70 truncate max-w-[200px] inline-block">
          {{ (row as Skill).git_url || '-' }}
        </span>
      </template>

      <template #cell-updated_at="{ row }">
        <span class="text-[11px] text-muted-foreground" :title="new Date((row as Skill).updated_at).toLocaleString()">
          {{ formatTime((row as Skill).updated_at) }}
        </span>
      </template>

      <template #actions="{ row }">
        <Button
          variant="ghost"
          size="icon-sm"
          :title="t('skill.update')"
          :disabled="updatingSkillIds.has((row as Skill).id) || !(row as Skill).git_url"
          @click="updateSkill(row as Skill)"
        >
          <Loader2 v-if="updatingSkillIds.has((row as Skill).id)" class="h-3 w-3 animate-spin" />
          <RefreshCw v-else class="h-3 w-3" />
        </Button>
        <a
          v-if="(row as Skill).git_url"
          :href="(row as Skill).git_url!.split('#')[0]"
          target="_blank"
          rel="noopener"
        >
          <Button variant="ghost" size="icon-sm" :title="t('skill.source')">
            <Download class="h-3 w-3" />
          </Button>
        </a>
        <Button variant="ghost" size="icon-sm" class="text-destructive hover:text-destructive" @click="openDelete(row as Skill)">
          <Trash2 class="h-3 w-3" />
        </Button>
      </template>
    </DataTable>

    <!-- Add Skill Dialog (2-step: URL → Select) -->
    <Dialog :open="showAddDialog" @update:open="(val) => { if (!val) closeAdd() }">
      <DialogContent class="max-w-lg">
        <DialogHeader>
          <DialogTitle>{{ t('skill.addFromGit') }}</DialogTitle>
          <DialogDescription>{{ discoverStep === 'url' ? t('skill.addDescription') : t('skill.selectSkills') }}</DialogDescription>
        </DialogHeader>

        <!-- Step 1: Enter URL -->
        <div v-if="discoverStep === 'url'" class="space-y-3">
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('skill.gitUrl') }}</label>
            <Input
              v-model="gitUrl"
              placeholder="https://github.com/user/repo.git"
              required
              class="font-mono text-[11px]"
              @keydown="handleKeydown"
            />
            <p class="text-[10px] text-muted-foreground/60">
              {{ t('skill.gitUrlHint') }}
            </p>
          </div>

          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('skill.visibility') }}</label>
            <div class="flex gap-1.5">
              <Button
                type="button"
                size="sm"
                :variant="visibility === 'private' ? 'default' : 'outline'"
                @click="visibility = 'private'"
              >
                {{ t('skill.private') }}
              </Button>
              <Button
                type="button"
                size="sm"
                :variant="visibility === 'public' ? 'default' : 'outline'"
                @click="visibility = 'public'"
              >
                {{ t('skill.public') }}
              </Button>
            </div>
          </div>

          <DialogFooter class="gap-1.5 pt-1">
            <Button type="button" variant="outline" size="sm" @click="closeAdd">
              {{ t('common.cancel') }}
            </Button>
            <Button size="sm" :disabled="discovering || !gitUrl.trim()" @click="discoverSkills">
              <Loader2 v-if="discovering" class="h-3 w-3 animate-spin" />
              <Search v-else class="h-3 w-3" />
              {{ discovering ? t('skill.discovering') : t('skill.discover') }}
            </Button>
          </DialogFooter>
        </div>

        <!-- Step 2: Select skills -->
        <div v-else class="space-y-3">
          <div class="flex items-center justify-between">
            <p class="text-xs text-muted-foreground">
              {{ t('skill.foundSkills', { count: discoveredSkills.length }) }}
            </p>
            <Button variant="ghost" size="sm" class="text-[11px] h-6" @click="toggleAll">
              {{ selectedSkills.size === discoveredSkills.length ? t('skill.deselectAll') : t('skill.selectAll') }}
            </Button>
          </div>

          <div class="space-y-1 max-h-[300px] overflow-y-auto">
            <div
              v-for="ds in discoveredSkills"
              :key="ds.path"
              class="flex items-start gap-2.5 rounded border border-border/60 px-3 py-2 cursor-pointer transition-colors"
              :class="selectedSkills.has(ds.path) ? 'bg-primary/5 border-primary/30' : 'hover:bg-secondary/50'"
              @click="toggleSkill(ds.path)"
            >
              <div
class="mt-0.5 h-4 w-4 rounded border flex items-center justify-center shrink-0 transition-colors"
                :class="selectedSkills.has(ds.path) ? 'bg-primary border-primary text-primary-foreground' : 'border-border'"
              >
                <Check v-if="selectedSkills.has(ds.path)" class="h-3 w-3" />
              </div>
              <div class="min-w-0 flex-1">
                <div class="font-medium text-xs text-foreground">{{ ds.name }}</div>
                <div v-if="ds.description" class="text-[10px] text-muted-foreground/70 mt-0.5 line-clamp-2">
                  {{ truncateDesc(ds.description, 120) }}
                </div>
              </div>
            </div>
          </div>

          <DialogFooter class="gap-1.5 pt-1">
            <Button type="button" variant="outline" size="sm" @click="discoverStep = 'url'">
              {{ t('common.back') }}
            </Button>
            <Button size="sm" :disabled="installing || selectedSkills.size === 0" @click="installSelected">
              <Loader2 v-if="installing" class="h-3 w-3 animate-spin" />
              <Plus v-else class="h-3 w-3" />
              {{ installing ? t('skill.installing') : t('skill.installSelected', { count: selectedSkills.size }) }}
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>

    <ConfirmDialog
      :open="showDeleteDialog"
      :title="t('common.delete')"
      :description="t('skill.confirmRemove')"
      :confirm-text="t('common.delete')"
      variant="destructive"
      @confirm="deleteSkill"
      @cancel="showDeleteDialog = false; deletingSkill = null"
    />
  </div>
</template>
