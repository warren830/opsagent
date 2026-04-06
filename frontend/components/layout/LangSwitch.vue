<script setup lang="ts">
const { locale, setLocale } = useI18n({ useScope: 'global' })

function switchLang() {
  const next = locale.value === 'zh' ? 'en' : 'zh'
  setLocale(next)
}
</script>

<template>
  <button
    class="inline-flex items-center justify-center rounded text-[11px] font-medium transition-colors hover:bg-accent hover:text-accent-foreground h-7 px-2 text-muted-foreground"
    @click="switchLang"
  >
    {{ locale === 'zh' ? 'EN' : '中' }}
  </button>
</template>
