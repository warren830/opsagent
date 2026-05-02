<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { toast } from 'vue-sonner'
import {
  Layers, Search, Upload, RefreshCw, Loader2,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Textarea } from '@/components/ui/textarea'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription,
} from '@/components/ui/dialog'
import DataTable from '@/components/shared/DataTable.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const router = useRouter()

// ─── Types ──────────────────────────────────────────────────────
interface CatalogEntity {
  id: string
  tenant_id: string
  kind: string
  name: string
  display_name: string | null
  description: string | null
  lifecycle: string
  owner_group_id: string | null
  system_id: string | null
  tags: string[]
  annotations: Record<string, unknown>
  source_url: string | null
  source_ref: string | null
  spec: Record<string, unknown>
  created_at: string
  updated_at: string
}

interface Cluster {
  id: string
  name: string
  cloud: string
  region: string | null
}

interface ImportResult {
  run_id: string
  entities_created: number
  entities_updated: number
  errors: string[]
}

// ─── State ──────────────────────────────────────────────────────
const entities = ref<CatalogEntity[]>([])
const loading = ref(true)
const query = ref('')
const activeKind = ref('')
const activeLifecycle = ref('')

// Cache of id → display name for owner/system reference resolution. The
// list endpoint already ships every row in one shot, so we build the map
// on the client instead of adding N extra round-trips.
const entityById = computed(() => {
  const m = new Map<string, CatalogEntity>()
  for (const e of entities.value) m.set(e.id, e)
  return m
})

const kindTabs = computed(() => [
  { value: '', label: t('catalog.kindAll') },
  { value: 'system', label: t('catalog.kindSystem') },
  { value: 'component', label: t('catalog.kindComponent') },
  { value: 'api', label: t('catalog.kindApi') },
  { value: 'resource', label: t('catalog.kindResource') },
  { value: 'group', label: t('catalog.kindGroup') },
])

const lifecycleOptions = computed(() => [
  { value: '', label: t('catalog.lifecycleAll') },
  { value: 'production', label: t('catalog.lifecycleProduction') },
  { value: 'experimental', label: t('catalog.lifecycleExperimental') },
  { value: 'deprecated', label: t('catalog.lifecycleDeprecated') },
  { value: 'retired', label: t('catalog.lifecycleRetired') },
])

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase()
  return entities.value.filter((e) => {
    if (activeKind.value && e.kind !== activeKind.value) return false
    if (activeLifecycle.value && e.lifecycle !== activeLifecycle.value) return false
    if (q) {
      const hay = `${e.name} ${e.display_name ?? ''}`.toLowerCase()
      if (!hay.includes(q)) return false
    }
    return true
  })
})

const columns = computed(() => [
  { key: 'name', label: t('catalog.columnName') },
  { key: 'kind', label: t('catalog.columnKind') },
  { key: 'system', label: t('catalog.columnSystem') },
  { key: 'owner', label: t('catalog.columnOwner') },
  { key: 'lifecycle', label: t('catalog.columnLifecycle') },
  { key: 'tags', label: t('catalog.columnTags') },
  { key: 'updated', label: t('catalog.columnUpdated') },
])

// ─── Helpers ────────────────────────────────────────────────────
function kindVariant(kind: string): 'default' | 'info' | 'success' | 'warning' | 'secondary' {
  switch (kind) {
    case 'system': return 'default'
    case 'component': return 'info'
    case 'api': return 'warning'
    case 'resource': return 'success'
    case 'group': return 'secondary'
    default: return 'secondary'
  }
}

function lifecycleVariant(lc: string): 'success' | 'info' | 'secondary' | 'destructive' {
  switch (lc) {
    case 'production': return 'success'
    case 'experimental': return 'info'
    case 'deprecated': return 'secondary'
    case 'retired': return 'destructive'
    default: return 'secondary'
  }
}

function formatDate(iso: string): string {
  if (!iso) return '-'
  return new Date(iso).toLocaleDateString(undefined, {
    year: 'numeric', month: 'short', day: 'numeric',
  })
}

function displayLabel(e: CatalogEntity): string {
  return e.display_name || e.name
}

// ─── API ────────────────────────────────────────────────────────
async function fetchEntities() {
  loading.value = true
  try {
    entities.value = await api.get<CatalogEntity[]>('/api/catalog/entities')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

// ─── Import YAML ────────────────────────────────────────────────
const showImport = ref(false)
const yamlText = ref('')
const importing = ref(false)

async function submitImport() {
  if (!yamlText.value.trim()) return
  importing.value = true
  try {
    // IMPORTANT: backend body is raw text, not JSON — use fetch() directly
    // so $fetch doesn't stringify the payload or set Content-Type: application/json.
    const config = useRuntimeConfig()
    const baseURL = (config.public.apiBase as string) || ''
    const response = await fetch(`${baseURL}/api/catalog/import/yaml`, {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'text/yaml' },
      body: yamlText.value,
    })
    if (!response.ok) {
      const errBody = await response.json().catch(() => ({ error: response.statusText }))
      throw new Error(errBody.error || response.statusText)
    }
    const result = (await response.json()) as ImportResult
    toast.success(t('catalog.importYamlSuccess', {
      created: result.entities_created,
      updated: result.entities_updated,
    }))
    for (const e of result.errors.slice(0, 3)) toast.warning(e, { duration: 8000 })
    showImport.value = false
    yamlText.value = ''
    await fetchEntities()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    importing.value = false
  }
}

// ─── K8s Discovery ──────────────────────────────────────────────
const showDiscover = ref(false)
const clusters = ref<Cluster[]>([])
const selectedClusterId = ref('')
const discovering = ref(false)

async function openDiscover() {
  showDiscover.value = true
  if (clusters.value.length === 0) {
    try {
      clusters.value = await api.get<Cluster[]>('/api/clusters')
    } catch {
      // silent — the select will just show empty
    }
  }
}

async function submitDiscover() {
  if (!selectedClusterId.value) return
  discovering.value = true
  try {
    const result = await api.post<ImportResult>('/api/catalog/discover/k8s', {
      cluster_id: selectedClusterId.value,
    })
    toast.success(t('catalog.discoverK8sSuccess', {
      created: result.entities_created,
      updated: result.entities_updated,
    }))
    for (const e of result.errors.slice(0, 3)) toast.warning(e, { duration: 8000 })
    showDiscover.value = false
    selectedClusterId.value = ''
    await fetchEntities()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    discovering.value = false
  }
}

// ─── Mount ──────────────────────────────────────────────────────
function openDetail(row: CatalogEntity) {
  router.push(`/catalog/${row.id}`)
}

onMounted(() => { fetchEntities() })
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between flex-wrap gap-3">
      <div class="flex items-center gap-3">
        <div class="p-2 rounded-lg bg-primary/10 border border-primary/20">
          <Layers class="h-5 w-5 text-primary" />
        </div>
        <div>
          <h1 class="text-base font-semibold text-foreground">{{ t('catalog.title') }}</h1>
          <p class="text-[11px] text-muted-foreground">{{ t('catalog.subtitle') }}</p>
        </div>
      </div>

      <div class="flex items-center gap-1.5">
        <Button variant="outline" size="sm" @click="openDiscover">
          <RefreshCw class="h-3.5 w-3.5" />
          {{ t('catalog.discoverK8s') }}
        </Button>
        <Button size="sm" @click="showImport = true">
          <Upload class="h-3.5 w-3.5" />
          {{ t('catalog.importYaml') }}
        </Button>
      </div>
    </div>

    <!-- Filters row -->
    <div class="flex items-center gap-3 flex-wrap">
      <div class="relative flex-1 min-w-[220px] max-w-md">
        <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground/60" />
        <Input v-model="query" :placeholder="t('catalog.searchPlaceholder')" class="pl-8 h-8" />
      </div>

      <div class="flex items-center gap-1">
        <Button
          v-for="tab in kindTabs"
          :key="tab.value"
          size="sm"
          :variant="activeKind === tab.value ? 'default' : 'outline'"
          @click="activeKind = tab.value"
        >
          {{ tab.label }}
        </Button>
      </div>

      <div class="w-px h-5 bg-border/60 hidden md:block" />

      <Select v-model="activeLifecycle">
        <SelectTrigger class="h-8 w-[160px] text-xs">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="opt in lifecycleOptions" :key="opt.value" :value="opt.value">
            {{ opt.label }}
          </SelectItem>
        </SelectContent>
      </Select>
    </div>

    <!-- Table / Empty state -->
    <DataTable
      v-if="entities.length > 0 || loading"
      :columns="columns"
      :data="filtered as unknown as Record<string, unknown>[]"
      :loading="loading"
    >
      <template #cell-name="{ row }">
        <button
          class="text-left font-medium text-foreground hover:text-primary transition-colors"
          @click="openDetail(row as CatalogEntity)"
        >
          {{ displayLabel(row as CatalogEntity) }}
          <span v-if="(row as CatalogEntity).display_name" class="block text-[10px] text-muted-foreground/60 font-normal">
            {{ (row as CatalogEntity).name }}
          </span>
        </button>
      </template>

      <template #cell-kind="{ row }">
        <Badge :variant="kindVariant((row as CatalogEntity).kind)" class="uppercase text-[10px]">
          {{ (row as CatalogEntity).kind }}
        </Badge>
      </template>

      <template #cell-system="{ row }">
        <span v-if="(row as CatalogEntity).system_id" class="text-muted-foreground">
          {{ entityById.get((row as CatalogEntity).system_id!)?.name ?? '—' }}
        </span>
        <span v-else class="text-muted-foreground/40">—</span>
      </template>

      <template #cell-owner="{ row }">
        <span v-if="(row as CatalogEntity).owner_group_id" class="text-muted-foreground">
          {{ entityById.get((row as CatalogEntity).owner_group_id!)?.name ?? '—' }}
        </span>
        <span v-else class="text-muted-foreground/40">—</span>
      </template>

      <template #cell-lifecycle="{ row }">
        <Badge :variant="lifecycleVariant((row as CatalogEntity).lifecycle)" class="text-[10px]">
          {{ (row as CatalogEntity).lifecycle }}
        </Badge>
      </template>

      <template #cell-tags="{ row }">
        <div class="flex flex-wrap gap-1">
          <Badge
            v-for="tag in (row as CatalogEntity).tags.slice(0, 3)"
            :key="tag"
            variant="secondary"
            class="text-[9px]"
          >
            {{ tag }}
          </Badge>
          <span
            v-if="(row as CatalogEntity).tags.length > 3"
            class="text-[10px] text-muted-foreground/60"
          >
            +{{ (row as CatalogEntity).tags.length - 3 }}
          </span>
        </div>
      </template>

      <template #cell-updated="{ row }">
        <span class="text-muted-foreground">{{ formatDate((row as CatalogEntity).updated_at) }}</span>
      </template>
    </DataTable>

    <div
      v-else
      class="rounded-lg border border-dashed border-border/50 py-12 text-center space-y-2"
    >
      <Layers class="h-10 w-10 mx-auto text-muted-foreground/40" />
      <p class="text-sm font-medium text-foreground">{{ t('catalog.emptyTitle') }}</p>
      <p class="text-xs text-muted-foreground">{{ t('catalog.emptyHint') }}</p>
      <div class="flex items-center justify-center gap-1.5 pt-2">
        <Button variant="outline" size="sm" @click="openDiscover">
          <RefreshCw class="h-3.5 w-3.5" />
          {{ t('catalog.discoverK8s') }}
        </Button>
        <Button size="sm" @click="showImport = true">
          <Upload class="h-3.5 w-3.5" />
          {{ t('catalog.importYaml') }}
        </Button>
      </div>
    </div>

    <!-- Import YAML dialog -->
    <Dialog :open="showImport" @update:open="(v) => { showImport = v }">
      <DialogContent class="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{{ t('catalog.importYamlTitle') }}</DialogTitle>
          <DialogDescription>{{ t('catalog.importYamlHint') }}</DialogDescription>
        </DialogHeader>
        <Textarea
          v-model="yamlText"
          rows="14"
          class="font-mono text-xs"
          placeholder="apiVersion: backstage.io/v1alpha1&#10;kind: Component&#10;metadata:&#10;  name: order-api&#10;spec:&#10;  type: service&#10;  lifecycle: production&#10;  owner: group:platform"
        />
        <DialogFooter>
          <Button variant="outline" size="sm" @click="showImport = false">{{ t('common.cancel') }}</Button>
          <Button size="sm" :disabled="importing || !yamlText.trim()" @click="submitImport">
            <Loader2 v-if="importing" class="h-3.5 w-3.5 animate-spin" />
            {{ t('catalog.importYamlSubmit') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <!-- K8s Discover dialog -->
    <Dialog :open="showDiscover" @update:open="(v) => { showDiscover = v }">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>{{ t('catalog.discoverK8sTitle') }}</DialogTitle>
          <DialogDescription>{{ t('catalog.discoverK8sHint') }}</DialogDescription>
        </DialogHeader>

        <div class="space-y-2">
          <label class="text-xs font-medium">{{ t('catalog.discoverK8sSelect') }}</label>
          <Select v-model="selectedClusterId">
            <SelectTrigger class="h-8 text-xs"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem v-for="c in clusters" :key="c.id" :value="c.id">
                {{ c.name }} · {{ c.region || c.cloud }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <DialogFooter>
          <Button variant="outline" size="sm" @click="showDiscover = false">{{ t('common.cancel') }}</Button>
          <Button
            size="sm"
            :disabled="discovering || !selectedClusterId"
            @click="submitDiscover"
          >
            <Loader2 v-if="discovering" class="h-3.5 w-3.5 animate-spin" />
            {{ t('catalog.discoverK8sSubmit') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
