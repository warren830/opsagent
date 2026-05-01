<!--
  ThemeSwitcher — sidebar-bottom UI for picking between Sky & Lavender (light)
  and Aurora (dark). Uses Popover so the choices appear beside the sidebar
  rather than pushing layout around.

  Collapsed mode: icon button with tooltip.
  Expanded mode: icon + label + chevron row, matching sibling nav links.
-->
<script setup lang="ts">
import { Sun, Moon, Palette, Check } from 'lucide-vue-next'
import { Popover, PopoverTrigger, PopoverContent } from '@/components/ui/popover'
import { Tooltip, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip'
import type { ThemeId } from '@/composables/useTheme'

defineProps<{ collapsed?: boolean }>()

const { t } = useI18n()
const { theme, setTheme } = useTheme()

type Option = {
  id: ThemeId
  label: string
  desc: string
  icon: typeof Sun
}

const options = computed<Option[]>(() => [
  {
    id: 'light',
    label: t('theme.light'),
    desc: 'Sky & Lavender',
    icon: Sun,
  },
  {
    id: 'aurora',
    label: t('theme.aurora'),
    desc: t('theme.auroraDesc'),
    icon: Moon,
  },
])

const currentIcon = computed(() => (theme.value === 'aurora' ? Moon : Sun))
</script>

<template>
  <Popover>
    <!-- Collapsed: icon-only with tooltip -->
    <template v-if="collapsed">
      <Tooltip>
        <TooltipTrigger as-child>
          <PopoverTrigger
            class="flex h-8 w-8 mx-auto items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground aurora-focus-ring"
            :aria-label="t('nav.selectTheme')"
          >
            <component :is="currentIcon" class="h-4 w-4" />
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="right" :side-offset="8">
          {{ t('nav.selectTheme') }}
        </TooltipContent>
      </Tooltip>
    </template>

    <!-- Expanded: matches nav-link row styling -->
    <template v-else>
      <PopoverTrigger
        class="flex w-full items-center gap-2.5 rounded px-2.5 py-1.5 text-[13px] text-muted-foreground transition-all duration-150 hover:bg-accent hover:text-foreground aurora-focus-ring"
      >
        <Palette class="h-4 w-4 shrink-0" />
        <span class="flex-1 text-left">{{ t('nav.selectTheme') }}</span>
        <span class="text-[11px] text-muted-foreground/60">
          {{ theme === 'aurora' ? t('theme.aurora') : t('theme.light') }}
        </span>
      </PopoverTrigger>
    </template>

    <PopoverContent
      side="right"
      align="end"
      :side-offset="12"
      class="w-56 p-1.5"
    >
      <div class="px-2 pt-1 pb-1.5 text-[10px] font-semibold uppercase tracking-widest text-muted-foreground/60">
        {{ t('theme.title') }}
      </div>
      <button
        v-for="opt in options"
        :key="opt.id"
        type="button"
        class="flex w-full items-center gap-2.5 rounded px-2 py-1.5 text-[12px] text-left transition-colors aurora-focus-ring"
        :class="theme === opt.id
          ? 'bg-primary/10 text-primary'
          : 'text-foreground hover:bg-accent hover:text-accent-foreground'"
        @click="setTheme(opt.id)"
      >
        <component :is="opt.icon" class="h-4 w-4 shrink-0" />
        <span class="flex-1 min-w-0">
          <span class="block font-medium leading-tight">{{ opt.label }}</span>
          <span class="block text-[10px] text-muted-foreground truncate">{{ opt.desc }}</span>
        </span>
        <Check v-if="theme === opt.id" class="h-3.5 w-3.5 shrink-0 text-primary" />
      </button>
    </PopoverContent>
  </Popover>
</template>
