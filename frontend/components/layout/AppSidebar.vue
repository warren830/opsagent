<script setup lang="ts">
import {
  LayoutDashboard, Users, Building2,
  Cloud, Server, Boxes, Radio, Brain, Network,
  BookOpen, LibraryBig, Wrench, Plug, Clock, Rocket,
  GitBranch, Activity, AlertTriangle, AlertOctagon, Settings, Target,
  ChevronDown, MessageSquare, PanelLeftClose, PanelLeftOpen,
  ChevronsUpDown, ChevronsDownUp, Layers,
} from 'lucide-vue-next'
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from '@/components/ui/collapsible'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip'
import { Separator } from '@/components/ui/separator'
import { ScrollArea } from '@/components/ui/scroll-area'

const { t } = useI18n()
const route = useRoute()
const authStore = useAuthStore()
const { issueCount, startPolling } = useIssueCount()
onMounted(() => { startPolling() })

const mobileSidebarOpen = useState('mobileSidebarOpen', () => false)

function closeMobileSidebar() {
  mobileSidebarOpen.value = false
}

const collapsed = useState('sidebarCollapsed', () => false)
function toggleSidebar() {
  collapsed.value = !collapsed.value
}

interface NavItem {
  label: string
  to: string
  icon: any
  superAdminOnly?: boolean
  badge?: number
}

interface NavGroup {
  label: string
  items: NavItem[]
  defaultOpen?: boolean
}

const navGroups = computed<NavGroup[]>(() => {
  const groups: NavGroup[] = [
    {
      label: t('nav.groups.principals'),
      defaultOpen: true,
      items: [
        { label: t('nav.users'), to: '/users', icon: Users, superAdminOnly: true },
        { label: t('nav.tenants'), to: '/tenants', icon: Building2, superAdminOnly: true },
      ],
    },
    {
      label: t('nav.groups.assets'),
      defaultOpen: true,
      items: [
        { label: t('nav.accounts'), to: '/accounts', icon: Cloud },
        { label: t('nav.clusters'), to: '/clusters', icon: Server },
        { label: t('nav.topology'), to: '/topology', icon: Network },
        { label: t('nav.catalog'), to: '/catalog', icon: Layers },
        { label: t('nav.resources'), to: '/resources', icon: Boxes },
      ],
    },
    {
      label: t('nav.groups.integrations'),
      defaultOpen: true,
      items: [
        { label: t('nav.channels'), to: '/channels', icon: Radio },
        { label: t('nav.providers'), to: '/providers', icon: Brain },
      ],
    },
    {
      label: t('nav.groups.knowledge'),
      defaultOpen: true,
      items: [
        { label: t('nav.glossary'), to: '/glossary', icon: BookOpen },
        { label: t('nav.knowledgeBase'), to: '/knowledge', icon: LibraryBig },
      ],
    },
    {
      label: t('nav.groups.tools'),
      defaultOpen: true,
      items: [
        { label: t('nav.skills'), to: '/skills', icon: Wrench },
        { label: t('nav.mcp'), to: '/mcp', icon: Plug },
        { label: t('nav.scheduledJobs'), to: '/scheduled-jobs', icon: Clock },
      ],
    },
    {
      label: t('nav.groups.telemetry'),
      defaultOpen: true,
      items: [
        { label: t('nav.telemetry'), to: '/telemetry', icon: Activity },
        { label: t('nav.pipeline'), to: '/repo', icon: GitBranch },
      ],
    },
    {
      label: t('nav.groups.ops'),
      defaultOpen: true,
      items: [
        { label: t('nav.deployments'), to: '/deployments', icon: Rocket },
        { label: t('nav.slos'), to: '/slos', icon: Target },
        { label: t('nav.issues'), to: '/issues', icon: AlertTriangle, badge: issueCount.value || undefined },
        { label: t('nav.incidents'), to: '/incidents', icon: AlertOctagon },
      ],
    },
  ]

  return groups.map(g => ({
    ...g,
    items: g.items.filter(i => !i.superAdminOnly || authStore.isSuperAdmin),
  })).filter(g => g.items.length > 0)
})

// Group open/close state — keyed by group label
const groupOpen = ref<Record<string, boolean>>({})
const groupOpenInitialized = ref(false)

// Initialize group states once navGroups is computed
watch(navGroups, (groups) => {
  if (groupOpenInitialized.value) return
  for (const g of groups) {
    groupOpen.value[g.label] = g.defaultOpen || g.items.some(i => route.path.startsWith(i.to))
  }
  groupOpenInitialized.value = true
}, { immediate: true })

const allGroupsExpanded = computed(() => navGroups.value.every(g => groupOpen.value[g.label]))
const _allGroupsCollapsed = computed(() => navGroups.value.every(g => !groupOpen.value[g.label]))

function expandAllGroups() {
  for (const g of navGroups.value) groupOpen.value[g.label] = true
}

function collapseAllGroups() {
  for (const g of navGroups.value) groupOpen.value[g.label] = false
}

function isActive(path: string) {
  if (path === '/') return route.path === '/'
  return route.path.startsWith(path)
}

const chatOpen = useState('chatPanelOpen', () => true)
function toggleChat() {
  chatOpen.value = !chatOpen.value
}
</script>

<template>
  <aside
    class="glass-panel flex-col overflow-hidden transition-[width,transform] duration-200 shrink-0"
    :class="[
      collapsed ? 'w-14' : 'w-52',
      mobileSidebarOpen
        ? 'fixed inset-y-2.5 left-2.5 z-40 flex h-[calc(100vh-20px)]'
        : 'hidden md:flex'
    ]"
  >
    <!-- Collapse toggle -->
    <div class="flex items-center px-2 py-1.5 shrink-0" :class="collapsed ? 'justify-center' : 'justify-end'">
      <Button
        variant="ghost" size="sm"
        class="h-6 w-6 p-0 text-muted-foreground/50 hover:text-muted-foreground"
        @click="toggleSidebar"
      >
        <PanelLeftClose v-if="!collapsed" class="h-3.5 w-3.5" />
        <PanelLeftOpen v-else class="h-3.5 w-3.5" />
      </Button>
    </div>

    <!-- Nav content -->
    <ScrollArea class="flex-1 px-2 pb-2">
      <div class="space-y-0.5">
        <!-- === Collapsed: icon-only mode === -->
        <template v-if="collapsed">
          <Tooltip>
            <TooltipTrigger as-child>
              <NuxtLink
                to="/"
                class="flex items-center justify-center rounded h-8 w-8 mx-auto transition-colors"
                :class="isActive('/') && route.path === '/' ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
                @click="closeMobileSidebar"
              >
                <LayoutDashboard class="h-4 w-4" />
              </NuxtLink>
            </TooltipTrigger>
            <TooltipContent side="right" :side-offset="8">{{ t('nav.dashboard') }}</TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger as-child>
              <Button
                variant="ghost" size="sm"
                class="h-8 w-8 p-0 mx-auto"
                :class="chatOpen ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
                @click="toggleChat(); closeMobileSidebar()"
              >
                <MessageSquare class="h-4 w-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="right" :side-offset="8">{{ t('nav.chat') }}</TooltipContent>
          </Tooltip>

          <Separator class="!my-1.5 mx-1 bg-border/50" />

          <template v-for="group in navGroups" :key="group.label">
            <Tooltip v-for="item in group.items" :key="item.to">
              <TooltipTrigger as-child>
                <NuxtLink
                  :to="item.to"
                  class="flex items-center justify-center rounded h-8 w-8 mx-auto transition-colors"
                  :class="isActive(item.to) ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
                  @click="closeMobileSidebar"
                >
                  <component :is="item.icon" class="h-4 w-4" />
                </NuxtLink>
              </TooltipTrigger>
              <TooltipContent side="right" :side-offset="8">{{ item.label }}</TooltipContent>
            </Tooltip>
          </template>

          <Separator class="!my-1.5 mx-1 bg-border/50" />

          <Tooltip>
            <TooltipTrigger as-child>
              <NuxtLink
                to="/settings"
                class="flex items-center justify-center rounded h-8 w-8 mx-auto transition-colors"
                :class="isActive('/settings') ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
                @click="closeMobileSidebar"
              >
                <Settings class="h-4 w-4" />
              </NuxtLink>
            </TooltipTrigger>
            <TooltipContent side="right" :side-offset="8">{{ t('nav.settings') }}</TooltipContent>
          </Tooltip>

          <LayoutThemeSwitcher collapsed />
        </template>

        <!-- === Expanded: full nav === -->
        <template v-else>
          <NuxtLink
            to="/"
            class="flex items-center gap-2.5 rounded px-2.5 py-1.5 text-[13px] transition-all duration-150"
            :class="isActive('/') && route.path === '/' ? 'bg-primary/10 text-primary font-medium' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
            @click="closeMobileSidebar"
          >
            <LayoutDashboard class="h-4 w-4 shrink-0" />
            <span>{{ t('nav.dashboard') }}</span>
          </NuxtLink>

          <Button
            variant="ghost" size="sm"
            class="flex w-full items-center gap-2.5 justify-start rounded px-2.5 py-1.5 h-auto text-[13px] transition-all duration-150"
            :class="chatOpen ? 'bg-primary/10 text-primary font-medium' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
            @click="toggleChat"
          >
            <MessageSquare class="h-4 w-4 shrink-0" />
            <span>{{ t('nav.chat') }}</span>
            <kbd class="ml-auto inline-flex items-center gap-0.5 text-foreground font-mono bg-secondary/50 border border-border/50 rounded px-1.5 py-0.5"><span class="text-[13px] leading-none">⌘</span><span class="text-[10px] leading-none">K</span></kbd>
          </Button>

          <Separator class="!my-2 bg-border/50" />

          <!-- Expand / Collapse all groups toggle -->
          <div class="flex justify-end px-1 -mb-0.5">
            <Button
              v-if="!allGroupsExpanded"
              variant="ghost" size="sm"
              class="h-5 w-5 p-0 text-muted-foreground/30 hover:text-muted-foreground"
              :title="t('nav.expandAll')"
              @click="expandAllGroups"
            >
              <ChevronsUpDown class="h-3 w-3" />
            </Button>
            <Button
              v-else
              variant="ghost" size="sm"
              class="h-5 w-5 p-0 text-muted-foreground/30 hover:text-muted-foreground"
              :title="t('nav.collapseAll')"
              @click="collapseAllGroups"
            >
              <ChevronsDownUp class="h-3 w-3" />
            </Button>
          </div>

          <Collapsible
            v-for="group in navGroups"
            :key="group.label"
            v-model:open="groupOpen[group.label]"
            class="space-y-0.5"
          >
            <CollapsibleTrigger class="flex w-full items-center justify-between rounded px-2 py-1 text-[10px] font-semibold uppercase tracking-widest text-muted-foreground/50 hover:text-muted-foreground transition-colors group">
              <span>{{ group.label }}</span>
              <ChevronDown class="h-3 w-3 transition-transform duration-200 group-data-[state=closed]:-rotate-90" />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <NuxtLink
                v-for="item in group.items"
                :key="item.to"
                :to="item.to"
                class="flex items-center gap-2 rounded-md ml-2 px-2.5 py-1.5 text-[12px] transition-all duration-150 border-l-2"
                :class="isActive(item.to) ? 'bg-primary/15 text-primary font-medium border-primary' : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground border-transparent'"
                @click="closeMobileSidebar"
              >
                <component :is="item.icon" class="h-4 w-4 shrink-0" />
                <span class="flex-1">{{ item.label }}</span>
                <Badge v-if="item.badge" variant="destructive" class="h-4 min-w-4 justify-center text-[9px] px-1 animate-pulse shadow-[0_0_8px_rgba(239,68,68,0.6)]">
                  {{ item.badge }}
                </Badge>
              </NuxtLink>
            </CollapsibleContent>
          </Collapsible>

          <Separator class="!my-2 bg-border/50" />

          <NuxtLink
            to="/settings"
            class="flex items-center gap-2.5 rounded px-2.5 py-1.5 text-[13px] transition-all duration-150"
            :class="isActive('/settings') ? 'bg-primary/10 text-primary font-medium' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
            @click="closeMobileSidebar"
          >
            <Settings class="h-4 w-4 shrink-0" />
            <span>{{ t('nav.settings') }}</span>
          </NuxtLink>

          <LayoutThemeSwitcher />
        </template>
      </div>
    </ScrollArea>
  </aside>
</template>
