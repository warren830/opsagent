<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  ArrowLeft, FileText, RefreshCw, Save, UploadCloud, CheckCircle2, Circle, ExternalLink,
} from 'lucide-vue-next'
import { toast } from 'vue-sonner'
import { marked } from 'marked'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'

definePageMeta({ middleware: 'auth' })

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const api = useApi()

interface PostmortemDoc {
  incident_id: string
  knowledge_file_id: string | null
  status: 'absent' | 'draft' | 'published' | 'missing'
  markdown: string | null
  updated_at: string | null
}

interface Incident {
  id: string
  number: number
  title: string
  severity: string
  status: string
  jira_key: string | null
}

const incidentId = computed(() => route.params.id as string)
const doc = ref<PostmortemDoc | null>(null)
const incident = ref<Incident | null>(null)
const markdown = ref('')
const loading = ref(true)
const busy = ref(false)
const dirty = ref(false)
const lastSavedAt = ref<string | null>(null)

// Action item rows are parsed out of the current markdown and kept in a
// structured form so they can be created in Jira without re-parsing.
interface ActionItemRow {
  owner: string
  description: string
  due: string
  jira_key: string
}
const actionItems = ref<ActionItemRow[]>([])

const SECTIONS = [
  'Summary', 'Impact', 'Root Cause', 'Detection', 'Resolution', 'Timeline',
  'Action Items', 'Lessons Learned',
]

const sectionCompletion = computed<Record<string, boolean>>(() => {
  const completed: Record<string, boolean> = {}
  const md = markdown.value
  for (const section of SECTIONS) {
    const heading = `## ${section}`
    const idx = md.indexOf(heading)
    if (idx === -1) {
      completed[section] = false
      continue
    }
    // Extract section body up to the next `## ` heading.
    const rest = md.slice(idx + heading.length)
    const end = rest.indexOf('\n## ')
    const body = end === -1 ? rest : rest.slice(0, end)
    // A section counts as "filled" if it has a non-placeholder line.
    const hasContent = body
      .split('\n')
      .some((line) => {
        const l = line.trim()
        if (!l) return false
        if (l.startsWith('##')) return false
        if (l.toLowerCase().startsWith('_todo')) return false
        if (l.toLowerCase() === '_tbd_') return false
        return true
      })
    completed[section] = hasContent
  }
  return completed
})

async function fetchDoc() {
  loading.value = true
  try {
    const [fetched, inc] = await Promise.all([
      api.get<PostmortemDoc>(`/api/incidents/${incidentId.value}/postmortem`),
      api.get<Incident>(`/api/incidents/${incidentId.value}`),
    ])
    doc.value = fetched
    incident.value = inc
    if (fetched.status === 'absent' || fetched.status === 'missing' || !fetched.markdown) {
      markdown.value = ''
    } else {
      markdown.value = fetched.markdown
      lastSavedAt.value = fetched.updated_at
    }
    parseActionItems()
    dirty.value = false
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    loading.value = false
  }
}

function parseActionItems() {
  const md = markdown.value
  const idx = md.indexOf('## Action Items')
  if (idx === -1) {
    actionItems.value = []
    return
  }
  const rest = md.slice(idx)
  const end = rest.indexOf('\n## ', 1)
  const body = end === -1 ? rest : rest.slice(0, end)

  const rows: ActionItemRow[] = []
  let pastHeader = false
  for (const line of body.split('\n')) {
    const trimmed = line.trim()
    if (!pastHeader) {
      if (trimmed.startsWith('|---')) pastHeader = true
      continue
    }
    if (!trimmed.startsWith('|')) continue
    const cells = trimmed.replace(/^\||\|$/g, '').split('|').map((c) => c.trim())
    if (cells.length < 3) continue
    const description = cells[1]
    if (!description || description.toLowerCase().startsWith('_todo') || description.toLowerCase() === '_tbd_') {
      continue
    }
    rows.push({
      owner: cells[0] === '_tbd_' ? '' : cells[0],
      description,
      due: cells[2] === '_tbd_' ? '' : cells[2],
      jira_key: cells[3] || '',
    })
  }
  actionItems.value = rows
}

async function regenerate() {
  busy.value = true
  try {
    const fresh = await api.post<PostmortemDoc>(`/api/incidents/${incidentId.value}/postmortem/draft`)
    doc.value = fresh
    markdown.value = fresh.markdown || ''
    lastSavedAt.value = fresh.updated_at
    parseActionItems()
    dirty.value = false
    toast.success(t('postmortem.regenerated'))
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    busy.value = false
  }
}

// useApi() exposes `put` for PATCH-like semantics, but the backend uses
// PATCH. Use fetch directly for the PATCH verb.
async function patchMarkdown(): Promise<PostmortemDoc> {
  const res = await fetch(`/api/incidents/${incidentId.value}/postmortem`, {
    method: 'PATCH',
    credentials: 'include',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ markdown: markdown.value }),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }))
    throw new Error(err.error || 'save failed')
  }
  return res.json()
}

async function saveReal() {
  if (!markdown.value.trim()) {
    toast.error(t('postmortem.errors.empty'))
    return
  }
  busy.value = true
  try {
    const updated = await patchMarkdown()
    doc.value = updated
    lastSavedAt.value = updated.updated_at
    dirty.value = false
    parseActionItems()
    toast.success(t('postmortem.saved'))
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    busy.value = false
  }
}

async function publish() {
  if (dirty.value) {
    toast.error(t('postmortem.errors.dirtyBeforePublish'))
    return
  }
  if (!confirm(t('postmortem.confirmPublish'))) return
  busy.value = true
  try {
    const published = await api.post<PostmortemDoc>(`/api/incidents/${incidentId.value}/postmortem/publish`)
    doc.value = published
    toast.success(t('postmortem.published'))
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    busy.value = false
  }
}

async function requestReview() {
  // "Request review" is an intentionally lightweight action: it drops a
  // timeline note on the incident so the reviewer sees it. The backend
  // already supports `POST /api/incidents/:id/timeline` for free-form
  // notes.
  busy.value = true
  try {
    await api.post(`/api/incidents/${incidentId.value}/timeline`, {
      summary: 'Postmortem ready for review',
      kind: 'manual_note',
      payload: { kind: 'review_requested' },
    })
    toast.success(t('postmortem.reviewRequested'))
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    busy.value = false
  }
}

async function createJiraForActionItem(row: ActionItemRow, index: number) {
  if (!row.description.trim()) return
  busy.value = true
  try {
    const summary = `[INC-${incident.value?.number ?? '?'}] ${row.description}`.slice(0, 250)
    const description = [
      `From postmortem for INC-${incident.value?.number} · ${incident.value?.title}`,
      row.owner ? `Owner: ${row.owner}` : '',
      row.due ? `Due: ${row.due}` : '',
    ].filter(Boolean).join('\n')
    const created = await api.post<{ key: string; url: string }>('/api/jira/create', {
      summary,
      description,
      issue_type: 'Task',
      labels: ['postmortem', `inc-${incident.value?.number}`],
    })
    actionItems.value[index].jira_key = created.key
    // Rewrite the table in markdown so the change persists after save.
    rewriteActionItemsTable()
    dirty.value = true
    toast.success(created.key)
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t('common.error')
    toast.error(msg)
  } finally {
    busy.value = false
  }
}

function addActionItem() {
  actionItems.value.push({ owner: '', description: '', due: '', jira_key: '' })
}

function removeActionItem(idx: number) {
  actionItems.value.splice(idx, 1)
  rewriteActionItemsTable()
  dirty.value = true
}

function rewriteActionItemsTable() {
  // Replace the entire `## Action Items` block in `markdown.value` with
  // the structured rows. The section header and separator line are
  // re-emitted so the table stays parseable.
  const md = markdown.value
  const idx = md.indexOf('## Action Items')
  if (idx === -1) {
    // Append a new section at the end.
    markdown.value = `${md.trimEnd()}\n\n## Action Items\n\n${renderActionItemsTable()}\n`
    return
  }
  const rest = md.slice(idx)
  const end = rest.indexOf('\n## ', 1)
  const after = end === -1 ? '' : rest.slice(end)
  markdown.value = `${md.slice(0, idx)}## Action Items\n\n${renderActionItemsTable()}\n${after}`
}

function renderActionItemsTable(): string {
  let out = '| Owner | Description | Due | Jira |\n|---|---|---|---|\n'
  if (actionItems.value.length === 0) {
    out += '| _tbd_ | _TODO — concrete engineering follow-up_ | _tbd_ | |\n'
    return out
  }
  for (const row of actionItems.value) {
    out += `| ${row.owner || '_tbd_'} | ${row.description || '_TODO_'} | ${row.due || '_tbd_'} | ${row.jira_key} |\n`
  }
  return out
}

function onMarkdownInput(e: Event) {
  const el = e.target as HTMLTextAreaElement
  markdown.value = el.value
  dirty.value = true
  parseActionItems()
}

function onActionItemChange() {
  rewriteActionItemsTable()
  dirty.value = true
}

const previewHtml = computed(() => {
  return marked.parse(markdown.value || '', { async: false }) as string
})

function scrollToSection(name: string) {
  const heading = `## ${name}`
  const idx = markdown.value.indexOf(heading)
  if (idx === -1) return
  // Focus the textarea and move caret to the heading.
  const ta = document.getElementById('pm-editor') as HTMLTextAreaElement | null
  if (!ta) return
  ta.focus()
  ta.setSelectionRange(idx, idx + heading.length)
  // Scroll the textarea so the line is visible — approximate.
  const linesBefore = markdown.value.slice(0, idx).split('\n').length
  const lineHeight = 20
  ta.scrollTop = Math.max(0, (linesBefore - 3) * lineHeight)
}

function formatTs(iso: string | null): string {
  if (!iso) return '—'
  return new Date(iso).toLocaleString(undefined, {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  })
}

onMounted(() => {
  fetchDoc()
})
</script>

<template>
  <div v-if="loading" class="p-6">
    <div class="h-8 w-64 bg-panel/60 rounded animate-pulse mb-4" />
    <div class="h-40 bg-panel/40 rounded animate-pulse" />
  </div>

  <div v-else class="flex flex-col h-[calc(100vh-4rem)] gap-3">
    <!-- Top bar -->
    <header class="flex items-center gap-3 flex-wrap">
      <Button variant="ghost" size="sm" class="h-7 px-2" @click="router.push(`/incidents/${incidentId}`)">
        <ArrowLeft class="h-3 w-3" />
        <span class="text-[11px]">{{ t('common.back') }}</span>
      </Button>
      <FileText class="h-4 w-4 text-muted-foreground" />
      <h1 class="text-sm font-semibold text-foreground">
        {{ t('postmortem.title') }} — INC-{{ String(incident?.number || 0).padStart(4, '0') }}
      </h1>
      <Badge v-if="doc?.status === 'published'" variant="success" class="text-[10px]">
        {{ t('postmortem.statusPublished') }}
      </Badge>
      <Badge v-else-if="doc?.status === 'draft'" variant="warning" class="text-[10px]">
        {{ t('postmortem.statusDraft') }}
      </Badge>
      <Badge v-else variant="secondary" class="text-[10px]">
        {{ t('postmortem.statusAbsent') }}
      </Badge>

      <div class="ml-auto flex items-center gap-2">
        <span v-if="lastSavedAt" class="text-[10px] text-muted-foreground/60">
          {{ t('postmortem.lastSaved') }}: {{ formatTs(lastSavedAt) }}
        </span>
        <Button variant="outline" size="sm" class="h-7 px-2 text-[11px]" :disabled="busy" @click="regenerate">
          <RefreshCw class="h-3 w-3" />
          {{ t('postmortem.regenerate') }}
        </Button>
        <Button variant="outline" size="sm" class="h-7 px-2 text-[11px]" :disabled="busy || !dirty" @click="saveReal">
          <Save class="h-3 w-3" />
          {{ t('common.save') }}
        </Button>
        <Button variant="outline" size="sm" class="h-7 px-2 text-[11px]" :disabled="busy" @click="requestReview">
          {{ t('postmortem.requestReview') }}
        </Button>
        <Button size="sm" class="h-7 px-2 text-[11px]"
                :disabled="busy || doc?.status !== 'draft' || dirty"
                @click="publish">
          <UploadCloud class="h-3 w-3" />
          {{ t('postmortem.publish') }}
        </Button>
      </div>
    </header>

    <!-- Empty state when no draft exists yet -->
    <div v-if="doc?.status === 'absent' || doc?.status === 'missing'"
         class="flex-1 flex items-center justify-center">
      <div class="bg-panel/60 rounded-md p-6 max-w-md text-center space-y-3">
        <FileText class="h-10 w-10 text-muted-foreground/50 mx-auto" />
        <h2 class="text-sm font-semibold text-foreground">{{ t('postmortem.emptyTitle') }}</h2>
        <p class="text-xs text-muted-foreground">{{ t('postmortem.emptyDesc') }}</p>
        <Button size="sm" :disabled="busy" @click="regenerate">
          <RefreshCw class="h-3 w-3" />
          {{ t('postmortem.generateFirstDraft') }}
        </Button>
      </div>
    </div>

    <!-- Split editor + preview -->
    <section v-else class="grid grid-cols-1 xl:grid-cols-[200px_1fr_1fr] gap-3 flex-1 min-h-0">
      <!-- Sections nav -->
      <nav class="bg-panel/60 rounded-md p-3 overflow-y-auto">
        <h3 class="text-[10px] uppercase tracking-[0.2em] text-muted-foreground font-semibold mb-2">
          {{ t('postmortem.sections') }}
        </h3>
        <ul class="space-y-1">
          <li v-for="s in SECTIONS" :key="s">
            <button
              class="w-full flex items-center gap-2 px-2 py-1 rounded text-[11px] hover:bg-background/60 transition-colors text-left"
              @click="scrollToSection(s)"
            >
              <CheckCircle2 v-if="sectionCompletion[s]" class="h-3 w-3 text-success shrink-0" />
              <Circle v-else class="h-3 w-3 text-muted-foreground/50 shrink-0" />
              <span class="truncate">{{ s }}</span>
            </button>
          </li>
        </ul>
      </nav>

      <!-- Editor -->
      <div class="bg-panel/60 rounded-md flex flex-col overflow-hidden">
        <div class="px-3 py-2 border-b border-border/30 flex items-center gap-2">
          <span class="text-[11px] uppercase tracking-[0.2em] text-muted-foreground font-semibold">
            {{ t('postmortem.editor') }}
          </span>
          <span v-if="dirty" class="text-[10px] text-warning">• {{ t('postmortem.unsaved') }}</span>
        </div>
        <textarea
          id="pm-editor"
          :value="markdown"
          class="flex-1 w-full px-3 py-2 bg-background/40 text-xs font-mono leading-relaxed outline-none resize-none"
          spellcheck="false"
          @input="onMarkdownInput"
        />

        <!-- Action items sub-editor -->
        <div class="border-t border-border/30 p-3 space-y-2 max-h-[40vh] overflow-y-auto">
          <div class="flex items-center gap-2">
            <h4 class="text-[10px] uppercase tracking-[0.2em] text-muted-foreground font-semibold">
              {{ t('postmortem.actionItems') }}
            </h4>
            <Button variant="ghost" size="sm" class="h-6 px-2 ml-auto text-[10px]" @click="addActionItem">
              + {{ t('common.create') }}
            </Button>
          </div>
          <table v-if="actionItems.length > 0" class="w-full text-[11px]">
            <thead>
              <tr class="text-muted-foreground/70 border-b border-border/30">
                <th class="text-left py-1 px-1 font-normal">{{ t('postmortem.ai.owner') }}</th>
                <th class="text-left py-1 px-1 font-normal">{{ t('postmortem.ai.description') }}</th>
                <th class="text-left py-1 px-1 font-normal">{{ t('postmortem.ai.due') }}</th>
                <th class="text-left py-1 px-1 font-normal">Jira</th>
                <th />
              </tr>
            </thead>
            <tbody>
              <tr v-for="(row, idx) in actionItems" :key="idx" class="border-b border-border/20">
                <td class="py-1 px-1">
                  <input v-model="row.owner" class="w-full h-6 px-1 rounded bg-background/60 text-[11px] outline-none"
                         @change="onActionItemChange" />
                </td>
                <td class="py-1 px-1">
                  <input v-model="row.description" class="w-full h-6 px-1 rounded bg-background/60 text-[11px] outline-none"
                         @change="onActionItemChange" />
                </td>
                <td class="py-1 px-1 w-24">
                  <input v-model="row.due" type="date"
                         class="w-full h-6 px-1 rounded bg-background/60 text-[11px] outline-none"
                         @change="onActionItemChange" />
                </td>
                <td class="py-1 px-1 w-24">
                  <a v-if="row.jira_key" class="text-primary hover:underline inline-flex items-center gap-1"
                     :href="`#`"><ExternalLink class="h-3 w-3" />{{ row.jira_key }}</a>
                  <Button v-else variant="outline" size="sm" class="h-6 px-2 text-[10px]"
                          :disabled="busy || !row.description.trim()"
                          @click="createJiraForActionItem(row, idx)">
                    Jira
                  </Button>
                </td>
                <td class="py-1 px-1 w-8">
                  <Button variant="ghost" size="sm" class="h-6 w-6 p-0 text-destructive"
                          @click="removeActionItem(idx)">×</Button>
                </td>
              </tr>
            </tbody>
          </table>
          <p v-else class="text-[11px] text-muted-foreground/70">
            {{ t('postmortem.ai.empty') }}
          </p>
        </div>
      </div>

      <!-- Preview -->
      <div class="bg-panel/60 rounded-md overflow-hidden flex flex-col">
        <div class="px-3 py-2 border-b border-border/30">
          <span class="text-[11px] uppercase tracking-[0.2em] text-muted-foreground font-semibold">
            {{ t('postmortem.preview') }}
          </span>
        </div>
        <div class="flex-1 overflow-y-auto p-4 pm-preview text-xs text-foreground/90" v-html="previewHtml" />
      </div>
    </section>
  </div>
</template>

<style scoped>
.pm-preview :deep(h1) { font-size: 0.95rem; font-weight: 700; margin-top: 0.75rem; margin-bottom: 0.5rem; border-bottom: 1px solid hsl(var(--border) / 0.3); padding-bottom: 0.25rem; }
.pm-preview :deep(h2) { font-size: 0.8125rem; font-weight: 600; margin-top: 0.75rem; margin-bottom: 0.25rem; }
.pm-preview :deep(h3) { font-size: 0.75rem; font-weight: 600; margin-top: 0.5rem; margin-bottom: 0.25rem; }
.pm-preview :deep(p) { margin: 0.25rem 0; line-height: 1.5; }
.pm-preview :deep(ul), .pm-preview :deep(ol) { padding-left: 1.25rem; margin: 0.25rem 0; }
.pm-preview :deep(table) { width: 100%; border-collapse: collapse; margin: 0.375rem 0; font-size: 0.6875rem; }
.pm-preview :deep(th) { text-align: left; padding: 0.25rem 0.5rem; border-bottom: 1px solid hsl(var(--border) / 0.6); font-weight: 600; }
.pm-preview :deep(td) { padding: 0.25rem 0.5rem; border-bottom: 1px solid hsl(var(--border) / 0.3); }
.pm-preview :deep(code) { font-size: 0.6875rem; padding: 0.125rem 0.25rem; border-radius: 0.25rem; background: hsl(var(--secondary)); font-family: ui-monospace, monospace; }
.pm-preview :deep(em) { font-style: italic; color: hsl(var(--muted-foreground)); }
</style>
