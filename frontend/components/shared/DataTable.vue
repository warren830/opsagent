<script setup lang="ts">
import { Skeleton } from '@/components/ui/skeleton'

export interface Column {
  key: string
  label: string
  sortable?: boolean
}

defineProps<{
  columns: Column[]
  data: Record<string, unknown>[]
  loading?: boolean
}>()

const { t } = useI18n()
</script>

<template>
  <div class="rounded border border-border/60 bg-card overflow-hidden">
    <div class="overflow-x-auto">
      <table class="w-full text-xs">
        <thead>
          <tr class="border-b border-border/60 bg-secondary/30">
            <th
              v-for="col in columns"
              :key="col.key"
              class="h-9 px-3 text-left align-middle font-medium text-muted-foreground whitespace-nowrap uppercase tracking-wider text-[11px]"
            >
              {{ col.label }}
            </th>
            <th
              v-if="$slots.actions"
              class="h-9 px-3 text-right align-middle font-medium text-muted-foreground whitespace-nowrap uppercase tracking-wider text-[11px]"
            >
              {{ t('common.actions') }}
            </th>
          </tr>
        </thead>
        <tbody>
          <!-- Loading skeleton rows -->
          <template v-if="loading">
            <tr
              v-for="i in 5"
              :key="`skeleton-${i}`"
              class="border-b border-border/40 transition-colors"
            >
              <td
                v-for="col in columns"
                :key="`skeleton-${i}-${col.key}`"
                class="px-3 py-2"
              >
                <Skeleton class="h-4 w-3/4 rounded-sm" />
              </td>
              <td v-if="$slots.actions" class="px-3 py-2 text-right">
                <div class="flex items-center justify-end gap-1">
                  <Skeleton class="h-6 w-6 rounded-sm" />
                  <Skeleton class="h-6 w-6 rounded-sm" />
                </div>
              </td>
            </tr>
          </template>

          <!-- Data rows -->
          <template v-else-if="data.length > 0">
            <tr
              v-for="(row, index) in data"
              :key="index"
              class="border-b border-border/40 transition-colors hover:bg-accent/50 group"
            >
              <td
                v-for="col in columns"
                :key="col.key"
                class="px-3 py-2 align-middle"
              >
                <slot :name="`cell-${col.key}`" :row="row" :value="row[col.key]">
                  <span class="text-foreground">{{ row[col.key] ?? '-' }}</span>
                </slot>
              </td>
              <td v-if="$slots.actions" class="px-3 py-2 text-right">
                <div class="flex items-center justify-end gap-0.5">
                  <slot name="actions" :row="row" />
                </div>
              </td>
            </tr>
          </template>

          <!-- Empty state -->
          <template v-else>
            <tr>
              <td
                :colspan="columns.length + ($slots.actions ? 1 : 0)"
                class="h-24 text-center"
              >
                <div class="flex flex-col items-center justify-center gap-1.5 text-muted-foreground">
                  <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8 opacity-30" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" />
                  </svg>
                  <span class="text-xs">{{ t('common.noData') }}</span>
                </div>
              </td>
            </tr>
          </template>
        </tbody>
      </table>
    </div>
  </div>
</template>
