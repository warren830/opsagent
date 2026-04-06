<script setup lang="ts">
import {
  LayoutDashboard, Users, Building2, ShieldCheck,
  Cloud, Server, Boxes, Radio, Brain,
  BookOpen, LibraryBig, Wrench, Plug, Clock,
  GitBranch, Activity, AlertTriangle, Settings,
  ChevronDown, MessageSquare, PanelLeftClose, PanelLeftOpen,
} from 'lucide-vue-next'
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from '@/components/ui/collapsible'
import { Badge } from '@/components/ui/badge'
import { Tooltip, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip'

const { t } = useI18n()
const route = useRoute()
const authStore = useAuthStore()

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
        { label: t('nav.approvals'), to: '/approvals', icon: ShieldCheck },
      ],
    },
    {
      label: t('nav.groups.assets'),
      defaultOpen: true,
      items: [
        { label: t('nav.accounts'), to: '/accounts', icon: Cloud },
        { label: t('nav.clusters'), to: '/clusters', icon: Server },
        { label: t('nav.resources'), to: '/resources', icon: Boxes },
      ],
    },
    {
      label: t('nav.groups.integrations'),
      items: [
        { label: t('nav.channels'), to: '/channels', icon: Radio },
        { label: t('nav.providers'), to: '/providers', icon: Brain },
      ],
    },
    {
      label: t('nav.groups.knowledge'),
      items: [
        { label: t('nav.glossary'), to: '/glossary', icon: BookOpen },
        { label: t('nav.knowledgeBase'), to: '/knowledge', icon: LibraryBig },
      ],
    },
    {
      label: t('nav.groups.tools'),
      items: [
        { label: t('nav.skills'), to: '/skills', icon: Wrench },
        { label: t('nav.mcp'), to: '/mcp', icon: Plug },
        { label: t('nav.scheduledJobs'), to: '/scheduled-jobs', icon: Clock },
      ],
    },
    {
      label: t('nav.groups.pipeline'),
      items: [
        { label: t('nav.pipeline'), to: '/pipeline', icon: GitBranch },
      ],
    },
    {
      label: t('nav.groups.telemetry'),
      items: [
        { label: t('nav.telemetry'), to: '/telemetry', icon: Activity },
      ],
    },
    {
      label: t('nav.groups.ops'),
      items: [
        { label: t('nav.issues'), to: '/issues', icon: AlertTriangle },
      ],
    },
  ]

  return groups.map(g => ({
    ...g,
    items: g.items.filter(i => !i.superAdminOnly || authStore.isSuperAdmin),
  })).filter(g => g.items.length > 0)
})

function isActive(path: string) {
  if (path === '/') return route.path === '/'
  return route.path.startsWith(path)
}

function groupHasActive(group: NavGroup) {
  return group.items.some(i => isActive(i.to))
}

const chatOpen = useState('chatPanelOpen', () => false)
function toggleChat() {
  chatOpen.value = !chatOpen.value
}
</script>

<template>
  <aside
    class="hidden md:flex flex-col border-r border-border/60 bg-card/50 transition-all duration-200 shrink-0"
    :class="collapsed ? 'w-12' : 'w-52'"
  >
    <!-- Collapse toggle -->
    <div class="flex items-center px-2 py-1.5 shrink-0" :class="collapsed ? 'justify-center' : 'justify-end'">
      <button
        class="h-6 w-6 rounded flex items-center justify-center text-muted-foreground/50 hover:text-muted-foreground hover:bg-accent transition-colors"
        @click="toggleSidebar"
      >
        <PanelLeftClose v-if="!collapsed" class="h-3.5 w-3.5" />
        <PanelLeftOpen v-else class="h-3.5 w-3.5" />
      </button>
    </div>

    <!-- Nav content -->
    <div class="flex-1 overflow-y-auto overflow-x-hidden px-2 pb-2 space-y-0.5">
      <!-- === Collapsed: icon-only mode === -->
      <template v-if="collapsed">
        <Tooltip>
          <TooltipTrigger as-child>
            <NuxtLink
              to="/"
              class="flex items-center justify-center rounded h-8 w-8 mx-auto transition-colors"
              :class="isActive('/') && route.path === '/' ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
            >
              <LayoutDashboard class="h-4 w-4" />
            </NuxtLink>
          </TooltipTrigger>
          <TooltipContent side="right" :side-offset="8">{{ t('nav.dashboard') }}</TooltipContent>
        </Tooltip>

        <Tooltip>
          <TooltipTrigger as-child>
            <button
              class="flex items-center justify-center rounded h-8 w-8 mx-auto transition-colors"
              :class="chatOpen ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
              @click="toggleChat"
            >
              <MessageSquare class="h-4 w-4" />
            </button>
          </TooltipTrigger>
          <TooltipContent side="right" :side-offset="8">{{ t('nav.chat') }}</TooltipContent>
        </Tooltip>

        <div class="!my-1.5 h-px bg-border/50 mx-1" />

        <template v-for="group in navGroups" :key="group.label">
          <Tooltip v-for="item in group.items" :key="item.to">
            <TooltipTrigger as-child>
              <NuxtLink
                :to="item.to"
                class="flex items-center justify-center rounded h-8 w-8 mx-auto transition-colors"
                :class="isActive(item.to) ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
              >
                <component :is="item.icon" class="h-4 w-4" />
              </NuxtLink>
            </TooltipTrigger>
            <TooltipContent side="right" :side-offset="8">{{ item.label }}</TooltipContent>
          </Tooltip>
        </template>

        <div class="!my-1.5 h-px bg-border/50 mx-1" />

        <Tooltip>
          <TooltipTrigger as-child>
            <NuxtLink
              to="/settings"
              class="flex items-center justify-center rounded h-8 w-8 mx-auto transition-colors"
              :class="isActive('/settings') ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
            >
              <Settings class="h-4 w-4" />
            </NuxtLink>
          </TooltipTrigger>
          <TooltipContent side="right" :side-offset="8">{{ t('nav.settings') }}</TooltipContent>
        </Tooltip>
      </template>

      <!-- === Expanded: full nav === -->
      <template v-else>
        <NuxtLink
          to="/"
          class="flex items-center gap-2.5 rounded px-2.5 py-1.5 text-[13px] transition-all duration-150"
          :class="isActive('/') && route.path === '/' ? 'bg-primary/10 text-primary font-medium' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
        >
          <LayoutDashboard class="h-4 w-4 shrink-0" />
          <span>{{ t('nav.dashboard') }}</span>
        </NuxtLink>

        <button
          class="flex w-full items-center gap-2.5 rounded px-2.5 py-1.5 text-[13px] transition-all duration-150"
          :class="chatOpen ? 'bg-primary/10 text-primary font-medium' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
          @click="toggleChat"
        >
          <MessageSquare class="h-4 w-4 shrink-0" />
          <span>{{ t('nav.chat') }}</span>
        </button>

        <div class="!my-2 h-px bg-border/50" />

        <Collapsible
          v-for="group in navGroups"
          :key="group.label"
          :default-open="group.defaultOpen || groupHasActive(group)"
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
            >
              <component :is="item.icon" class="h-4 w-4 shrink-0" />
              <span class="flex-1">{{ item.label }}</span>
              <Badge v-if="item.badge" variant="secondary" class="h-4 min-w-4 justify-center text-[9px] px-1">
                {{ item.badge }}
              </Badge>
            </NuxtLink>
          </CollapsibleContent>
        </Collapsible>

        <div class="!my-2 h-px bg-border/50" />

        <NuxtLink
          to="/settings"
          class="flex items-center gap-2.5 rounded px-2.5 py-1.5 text-[13px] transition-all duration-150"
          :class="isActive('/settings') ? 'bg-primary/10 text-primary font-medium' : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
        >
          <Settings class="h-4 w-4 shrink-0" />
          <span>{{ t('nav.settings') }}</span>
        </NuxtLink>
      </template>
    </div>
  </aside>
</template>
