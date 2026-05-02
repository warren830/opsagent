<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import { toast } from 'vue-sonner'
import {
  ArrowLeft, Pencil, Trash2, ExternalLink, Layers, Loader2,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Textarea } from '@/components/ui/textarea'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription,
} from '@/components/ui/dialog'
import ConfirmDialog from '@/components/shared/ConfirmDialog.vue'
import TopologyGraph from '@/components/catalog/TopologyGraph.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()
const route = useRoute()
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

interface CatalogRelation {
  id: string
  from_id: string
  to_id: string
  relation_type: string
  created_at: string
}

// ─── State ──────────────────────────────────────────────────────
const entityId = computed(() => route.params.id as string)
const entity = ref<CatalogEntity | null>(null)
const loading = ref(true)
const relations = ref<CatalogRelation[]>([])
const relatedIndex = ref<Map<string, CatalogEntity>>(new Map())
const activeTab = ref<'overview' | 'relations' | 'graph'>('overview')

// Edit dialog state
const showEdit = ref(false)
const saving = ref(false)
const editForm = ref({
  display_name: '',
  description: '',
  lifecycle: 'experimental',
  tags: '',
  source_url: '',
})

// Delete confirm state
const showDelete = ref(false)

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

function relationLabel(type: string): string {
  switch (type) {
    case 'owns': return t('catalog.relationOwns')
    case 'provides': return t('catalog.relationProvides')
    case 'consumes': return t('catalog.relationConsumes')
    case 'depends_on': return t('catalog.relationDependsOn')
    case 'part_of': return t('catalog.relationPartOf')
    case 'deployed_on': return t('catalog.relationDeployedOn')
    default: return type
  }
}

// Group relations into outgoing (this → other) and incoming (other → this),
// then bucket by relation_type for the Relations tab grid.
interface RelationBucket {
  direction: 'out' | 'in'
  relation_type: string
  label: string
  entries: { relation: CatalogRelation; other: CatalogEntity | null }[]
}

const relationBuckets = computed<RelationBucket[]>(() => {
  if (!entity.value) return []
  const me = entity.value.id
  const buckets = new Map<string, RelationBucket>()
  for (const r of relations.value) {
    const direction: 'out' | 'in' = r.from_id === me ? 'out' : 'in'
    const otherId = direction === 'out' ? r.to_id : r.from_id
    const key = `${direction}:${r.relation_type}`
    if (!buckets.has(key)) {
      buckets.set(key, {
        direction,
        relation_type: r.relation_type,
        label: relationLabel(r.relation_type),
        entries: [],
      })
    }
    buckets.get(key)!.entries.push({
      relation: r,
      other: relatedIndex.value.get(otherId) ?? null,
    })
  }
  return Array.from(buckets.values())
})

const prettySpec = computed(() => {
  if (!entity.value?.spec) return ''
  try {
    return JSON.stringify(entity.value.spec, null, 2)
  } catch {
    return ''
  }
})

const annotationEntries = computed(() => {
  if (!entity.value?.annotations) return []
  return Object.entries(entity.value.annotations).map(([key, value]) => ({
    key,
    value: typeof value === 'string' ? value : JSON.stringify(value),
  }))
})

// ─── API ────────────────────────────────────────────────────────
async function fetchEntity() {
  loading.value = true
  try {
    entity.value = await api.get<CatalogEntity>(`/api/catalog/entities/${entityId.value}`)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
    entity.value = null
  } finally {
    loading.value = false
  }
}

async function fetchRelations() {
  try {
    relations.value = await api.get<CatalogRelation[]>(
      `/api/catalog/entities/${entityId.value}/relations`,
    )

    // Resolve related entity names. One catalog listing fetch covers everything
    // on the page — the list endpoint is cheap and it keeps the UI copy cached.
    if (relations.value.length > 0 && relatedIndex.value.size === 0) {
      const all = await api.get<CatalogEntity[]>('/api/catalog/entities')
      const m = new Map<string, CatalogEntity>()
      for (const e of all) m.set(e.id, e)
      relatedIndex.value = m
    }
  } catch {
    // silent — empty relations are a valid state
  }
}

function initEditForm() {
  if (!entity.value) return
  editForm.value = {
    display_name: entity.value.display_name ?? '',
    description: entity.value.description ?? '',
    lifecycle: entity.value.lifecycle,
    tags: entity.value.tags.join(', '),
    source_url: entity.value.source_url ?? '',
  }
}

async function submitEdit() {
  if (!entity.value) return
  saving.value = true
  try {
    const payload: Record<string, unknown> = {
      display_name: editForm.value.display_name || null,
      description: editForm.value.description || null,
      lifecycle: editForm.value.lifecycle,
      tags: editForm.value.tags.split(',').map(s => s.trim()).filter(Boolean),
      source_url: editForm.value.source_url || null,
    }
    entity.value = await api.put<CatalogEntity>(
      `/api/catalog/entities/${entityId.value}`,
      payload,
    )
    toast.success(t('common.success'))
    showEdit.value = false
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    saving.value = false
  }
}

async function handleDelete() {
  try {
    await api.del(`/api/catalog/entities/${entityId.value}`)
    toast.success(t('common.success'))
    showDelete.value = false
    router.push('/catalog')
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  }
}

onMounted(async () => {
  await fetchEntity()
  await fetchRelations()
})

// Refetch if the route id changes while we stay on this page (rare but
// possible when the topology graph navigates to a neighbour).
watch(entityId, async (newId, oldId) => {
  if (newId && newId !== oldId) {
    relatedIndex.value = new Map()
    activeTab.value = 'overview'
    await fetchEntity()
    await fetchRelations()
  }
})

function openEdit() {
  initEditForm()
  showEdit.value = true
}
</script>

<template>
  <div class="space-y-4">
    <!-- Back + header actions -->
    <div class="flex items-center justify-between">
      <Button variant="ghost" size="sm" @click="router.push('/catalog')">
        <ArrowLeft class="h-3.5 w-3.5" />
        {{ t('common.back') }}
      </Button>

      <div v-if="entity" class="flex items-center gap-1.5">
        <Button variant="outline" size="sm" @click="openEdit">
          <Pencil class="h-3.5 w-3.5" />
          {{ t('catalog.editEntity') }}
        </Button>
        <Button
          variant="outline"
          size="sm"
          class="text-destructive hover:text-destructive"
          @click="showDelete = true"
        >
          <Trash2 class="h-3.5 w-3.5" />
          {{ t('common.delete') }}
        </Button>
      </div>
    </div>

    <!-- Loading / Not found -->
    <div v-if="loading" class="flex items-center justify-center py-16">
      <Loader2 class="h-6 w-6 animate-spin text-primary" />
    </div>

    <div v-else-if="!entity" class="rounded-lg border border-dashed border-border/50 py-12 text-center">
      <Layers class="h-10 w-10 mx-auto text-muted-foreground/40" />
      <p class="mt-2 text-sm text-muted-foreground">{{ t('catalog.notFound') }}</p>
    </div>

    <!-- Main grid -->
    <div v-else class="grid grid-cols-1 lg:grid-cols-[320px_1fr] gap-4">
      <!-- ═══ Left: metadata panel ═══ -->
      <aside class="space-y-4">
        <div class="rounded-lg bg-panel/60 p-4 space-y-3">
          <div class="flex items-start justify-between gap-2">
            <div class="min-w-0">
              <h1 class="text-base font-semibold text-foreground truncate">
                {{ entity.display_name || entity.name }}
              </h1>
              <p v-if="entity.display_name" class="text-[11px] text-muted-foreground/70 truncate">
                {{ entity.name }}
              </p>
            </div>
            <Badge :variant="kindVariant(entity.kind)" class="uppercase text-[10px] shrink-0">
              {{ entity.kind }}
            </Badge>
          </div>

          <p v-if="entity.description" class="text-xs text-muted-foreground leading-relaxed">
            {{ entity.description }}
          </p>

          <div class="flex items-center gap-1.5">
            <Badge :variant="lifecycleVariant(entity.lifecycle)" class="text-[10px]">
              {{ entity.lifecycle }}
            </Badge>
          </div>

          <div v-if="entity.tags.length > 0" class="flex flex-wrap gap-1">
            <Badge
              v-for="tag in entity.tags"
              :key="tag"
              variant="secondary"
              class="text-[9px]"
            >
              {{ tag }}
            </Badge>
          </div>
        </div>

        <!-- Ownership / system -->
        <div class="rounded-lg bg-panel/60 p-4 space-y-2 text-xs">
          <div v-if="entity.system_id" class="flex items-start justify-between gap-2">
            <span class="text-muted-foreground">{{ t('catalog.metaSystem') }}</span>
            <NuxtLink
              :to="`/catalog/${entity.system_id}`"
              class="text-primary hover:underline truncate max-w-[60%] text-right"
            >
              {{ relatedIndex.get(entity.system_id)?.name ?? entity.system_id.slice(0, 8) }}
            </NuxtLink>
          </div>

          <div v-if="entity.owner_group_id" class="flex items-start justify-between gap-2">
            <span class="text-muted-foreground">{{ t('catalog.metaOwner') }}</span>
            <NuxtLink
              :to="`/catalog/${entity.owner_group_id}`"
              class="text-primary hover:underline truncate max-w-[60%] text-right"
            >
              {{ relatedIndex.get(entity.owner_group_id)?.name ?? entity.owner_group_id.slice(0, 8) }}
            </NuxtLink>
          </div>

          <div v-if="entity.source_url" class="flex items-start justify-between gap-2">
            <span class="text-muted-foreground">{{ t('catalog.metaSource') }}</span>
            <a
              :href="entity.source_url"
              target="_blank"
              rel="noopener noreferrer"
              class="text-primary hover:underline truncate max-w-[60%] text-right inline-flex items-center gap-1"
            >
              {{ entity.source_ref || entity.source_url }}
              <ExternalLink class="h-3 w-3 shrink-0" />
            </a>
          </div>
        </div>
      </aside>

      <!-- ═══ Right: tabs ═══ -->
      <section class="space-y-3">
        <!-- Tab bar -->
        <div class="flex items-center gap-1 border-b border-border/40">
          <button
            v-for="tab in [
              { id: 'overview' as const, label: t('catalog.tabOverview') },
              { id: 'relations' as const, label: t('catalog.tabRelations') },
              { id: 'graph' as const, label: t('catalog.tabGraph') },
            ]"
            :key="tab.id"
            class="relative px-3 h-8 text-xs font-medium transition-colors"
            :class="activeTab === tab.id
              ? 'text-primary'
              : 'text-muted-foreground hover:text-foreground'"
            @click="activeTab = tab.id"
          >
            {{ tab.label }}
            <span
              v-if="activeTab === tab.id"
              class="absolute bottom-0 left-0 right-0 h-0.5 bg-primary rounded-t"
            />
          </button>
        </div>

        <!-- Overview tab -->
        <div v-if="activeTab === 'overview'" class="space-y-3">
          <div v-if="annotationEntries.length > 0" class="rounded-lg bg-panel/60 p-4 space-y-2">
            <div class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              Annotations
            </div>
            <dl class="grid grid-cols-1 md:grid-cols-2 gap-x-4 gap-y-1.5 text-xs">
              <template v-for="ann in annotationEntries" :key="ann.key">
                <dt class="text-muted-foreground truncate">{{ ann.key }}</dt>
                <dd class="font-mono text-[11px] text-foreground truncate">{{ ann.value }}</dd>
              </template>
            </dl>
          </div>

          <div v-if="prettySpec" class="rounded-lg bg-panel/60 p-4 space-y-2">
            <div class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              {{ t('catalog.metaSpec') }}
            </div>
            <pre class="text-[11px] font-mono whitespace-pre-wrap break-words text-foreground/90">{{ prettySpec }}</pre>
          </div>
        </div>

        <!-- Relations tab -->
        <div v-else-if="activeTab === 'relations'" class="space-y-3">
          <div
            v-if="relationBuckets.length === 0"
            class="rounded-lg border border-dashed border-border/50 py-10 text-center text-xs text-muted-foreground/70"
          >
            {{ t('catalog.relationsEmpty') }}
          </div>
          <div
            v-for="bucket in relationBuckets"
            :key="`${bucket.direction}:${bucket.relation_type}`"
            class="rounded-lg bg-panel/60 p-4 space-y-2"
          >
            <div class="flex items-center gap-2">
              <span class="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
                {{ bucket.label }}
              </span>
              <Badge variant="secondary" class="text-[9px]">{{ bucket.direction === 'out' ? '→' : '←' }}</Badge>
              <Badge variant="secondary" class="text-[9px]">{{ bucket.entries.length }}</Badge>
            </div>
            <ul class="space-y-1.5">
              <li
                v-for="entry in bucket.entries"
                :key="entry.relation.id"
                class="flex items-center justify-between gap-2 text-xs"
              >
                <NuxtLink
                  v-if="entry.other"
                  :to="`/catalog/${entry.other.id}`"
                  class="flex items-center gap-2 min-w-0 hover:text-primary transition-colors"
                >
                  <Badge :variant="kindVariant(entry.other.kind)" class="uppercase text-[9px] shrink-0">
                    {{ entry.other.kind }}
                  </Badge>
                  <span class="truncate">{{ entry.other.display_name || entry.other.name }}</span>
                </NuxtLink>
                <span v-else class="text-muted-foreground/60 font-mono text-[10px]">
                  {{ bucket.direction === 'out' ? entry.relation.to_id : entry.relation.from_id }}
                </span>
              </li>
            </ul>
          </div>
        </div>

        <!-- Graph tab -->
        <div v-else-if="activeTab === 'graph'" class="rounded-lg bg-panel/60 overflow-hidden">
          <TopologyGraph :entity-id="entityId" :depth="2" />
        </div>
      </section>
    </div>

    <!-- Edit dialog -->
    <Dialog :open="showEdit" @update:open="(v) => { showEdit = v }">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>{{ t('catalog.editEntity') }}</DialogTitle>
          <DialogDescription>{{ entity?.name }}</DialogDescription>
        </DialogHeader>

        <form class="space-y-3" @submit.prevent="submitEdit">
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('catalog.columnName') }}</label>
            <Input v-model="editForm.display_name" :placeholder="entity?.name || ''" />
          </div>
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('glossary.description') }}</label>
            <Textarea v-model="editForm.description" rows="3" />
          </div>
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('catalog.columnLifecycle') }}</label>
            <Select v-model="editForm.lifecycle">
              <SelectTrigger class="h-8 text-xs"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="production">production</SelectItem>
                <SelectItem value="experimental">experimental</SelectItem>
                <SelectItem value="deprecated">deprecated</SelectItem>
                <SelectItem value="retired">retired</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('catalog.columnTags') }}</label>
            <Input v-model="editForm.tags" placeholder="rust, critical, team-platform" />
          </div>
          <div class="space-y-1.5">
            <label class="text-xs font-medium">{{ t('catalog.metaSource') }} URL</label>
            <Input v-model="editForm.source_url" placeholder="https://github.com/..." />
          </div>

          <DialogFooter>
            <Button variant="outline" size="sm" type="button" @click="showEdit = false">
              {{ t('common.cancel') }}
            </Button>
            <Button size="sm" type="submit" :disabled="saving">
              <Loader2 v-if="saving" class="h-3.5 w-3.5 animate-spin" />
              {{ t('common.save') }}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>

    <ConfirmDialog
      :open="showDelete"
      :title="t('common.delete')"
      :description="t('catalog.deleteConfirm')"
      :confirm-text="t('common.delete')"
      variant="destructive"
      @confirm="handleDelete"
      @cancel="showDelete = false"
    />
  </div>
</template>
