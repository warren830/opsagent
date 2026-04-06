<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue'
import { ScanSearch } from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import DataTable from '@/components/shared/DataTable.vue'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const api = useApi()

interface Resource {
  id: string
  resource_type: string
  name: string
  account_id: string
  region: string
  status: string
  arn: string | null
}

const resources = ref<Resource[]>([])
const loading = ref(true)
const scanning = ref(false)
const search = ref('')
const filterType = ref('')
const filterRegion = ref('')

const columns = computed(() => [
  { key: 'resource_type', label: t('resource.type') },
  { key: 'name', label: t('tenant.name') },
  { key: 'account_id', label: t('resource.account') },
  { key: 'region', label: t('resource.region') },
  { key: 'status', label: t('cluster.status') },
  { key: 'arn', label: t('resource.arn') },
])

const resourceTypes = computed(() => {
  const types = new Set(resources.value.map((r) => r.resource_type))
  return Array.from(types).sort()
})

const regions = computed(() => {
  const regs = new Set(resources.value.map((r) => r.region))
  return Array.from(regs).sort()
})

function statusVariant(status: string): 'success' | 'warning' | 'destructive' | 'secondary' {
  switch (status) {
    case 'running': case 'available': case 'active': return 'success'
    case 'pending': case 'creating': return 'warning'
    case 'stopped': case 'terminated': case 'failed': return 'destructive'
    default: return 'secondary'
  }
}

function truncate(str: string | null, len: number): string {
  if (!str) return '-'
  return str.length > len ? str.slice(0, len) + '...' : str
}

async function fetchResources() {
  loading.value = true
  try {
    const params = new URLSearchParams()
    if (search.value) params.set('q', search.value)
    if (filterType.value) params.set('type', filterType.value)
    if (filterRegion.value) params.set('region', filterRegion.value)
    const qs = params.toString()
    resources.value = await api.get<Resource[]>(`/api/resources${qs ? '?' + qs : ''}`)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

async function scanResources() {
  scanning.value = true
  try {
    await api.post('/api/resources/scan')
    toast.success(t('common.success'))
    await fetchResources()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    scanning.value = false
  }
}

let searchTimeout: ReturnType<typeof setTimeout> | null = null
watch(search, () => {
  if (searchTimeout) clearTimeout(searchTimeout)
  searchTimeout = setTimeout(() => { fetchResources() }, 300)
})

watch([filterType, filterRegion], () => { fetchResources() })

onMounted(() => { fetchResources() })
</script>

<template>
  <div class="space-y-4">
    <!-- Page Header -->
    <div class="flex items-center justify-between">
      <h1 class="text-base font-semibold text-foreground">{{ t('resource.title') }}</h1>
      <Button size="sm" :disabled="scanning" @click="scanResources">
        <ScanSearch class="h-3.5 w-3.5" />
        {{ scanning ? t('resource.scanning') : t('resource.scan') }}
      </Button>
    </div>

    <!-- Search & Filters -->
    <div class="flex items-center gap-1.5">
      <Input v-model="search" :placeholder="t('common.search')" class="max-w-[200px]" />
      <select
        v-model="filterType"
        class="flex h-8 rounded border border-border/60 bg-secondary/50 px-2.5 py-1.5 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 transition-colors"
      >
        <option value="">{{ t('resource.type') }}</option>
        <option v-for="rt in resourceTypes" :key="rt" :value="rt">{{ rt }}</option>
      </select>
      <select
        v-model="filterRegion"
        class="flex h-8 rounded border border-border/60 bg-secondary/50 px-2.5 py-1.5 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-primary/50 transition-colors"
      >
        <option value="">{{ t('resource.region') }}</option>
        <option v-for="r in regions" :key="r" :value="r">{{ r }}</option>
      </select>
    </div>

    <!-- Data Table -->
    <DataTable :columns="columns" :data="resources" :loading="loading">
      <template #cell-resource_type="{ row }">
        <Badge variant="secondary">{{ (row as Resource).resource_type }}</Badge>
      </template>

      <template #cell-name="{ row }">
        <span class="font-medium text-foreground">{{ (row as Resource).name }}</span>
      </template>

      <template #cell-account_id="{ row }">
        <code class="rounded-sm bg-secondary px-1.5 py-0.5 text-[11px] font-mono text-muted-foreground">{{ (row as Resource).account_id }}</code>
      </template>

      <template #cell-region="{ row }">
        <span class="text-muted-foreground">{{ (row as Resource).region }}</span>
      </template>

      <template #cell-status="{ row }">
        <Badge :variant="statusVariant((row as Resource).status)">
          {{ (row as Resource).status }}
        </Badge>
      </template>

      <template #cell-arn="{ row }">
        <span class="text-muted-foreground text-[11px] font-mono" :title="(row as Resource).arn || ''">{{ truncate((row as Resource).arn, 40) }}</span>
      </template>
    </DataTable>
  </div>
</template>
