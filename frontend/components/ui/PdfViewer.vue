<script setup lang="ts">
import { ref, watch, onMounted, nextTick } from 'vue'
import { X, ZoomIn, ZoomOut, ChevronLeft, ChevronRight, Maximize2 } from 'lucide-vue-next'

interface BBox {
  x0: number
  y0: number
  x1: number
  y1: number
}

const props = defineProps<{
  open: boolean
  pdfUrl: string
  page?: number
  bbox?: BBox
  fileName?: string
}>()

const emit = defineEmits<{
  (e: 'update:open', val: boolean): void
}>()

const canvasRef = ref<HTMLCanvasElement>()
const overlayRef = ref<HTMLCanvasElement>()
const containerRef = ref<HTMLDivElement>()
const currentPage = ref(1)
const totalPages = ref(0)
const scale = ref(1.5)
const loading = ref(false)
const loadProgress = ref(0) // 0-100
const error = ref('')

let pdfDoc: any = null
let pdfjsLib: any = null

async function loadPdfJs() {
  if (pdfjsLib) return
  const pdfjs = await import('pdfjs-dist')
  // Use bundled worker
  pdfjs.GlobalWorkerOptions.workerSrc = new URL(
    'pdfjs-dist/build/pdf.worker.min.mjs',
    import.meta.url
  ).toString()
  pdfjsLib = pdfjs
}

async function loadPdf() {
  if (!props.pdfUrl) return
  loading.value = true
  error.value = ''

  try {
    await loadPdfJs()
    loadProgress.value = 0
    const loadingTask = pdfjsLib.getDocument({
      url: props.pdfUrl,
      cMapUrl: 'https://cdn.jsdelivr.net/npm/pdfjs-dist@4.0.379/cmaps/',
      cMapPacked: true,
    })
    loadingTask.onProgress = (data: { loaded: number; total: number }) => {
      if (data.total > 0) {
        loadProgress.value = Math.min(Math.round((data.loaded / data.total) * 100), 100)
      }
    }
    pdfDoc = await loadingTask.promise
    totalPages.value = pdfDoc.numPages
    currentPage.value = props.page || 1
    await renderPage()
  } catch (e: any) {
    error.value = `Failed to load PDF: ${e.message || e}`
  } finally {
    loading.value = false
  }
}

async function renderPage() {
  if (!pdfDoc || !canvasRef.value) return

  const page = await pdfDoc.getPage(currentPage.value)
  const viewport = page.getViewport({ scale: scale.value })

  const canvas = canvasRef.value
  canvas.width = viewport.width
  canvas.height = viewport.height

  const ctx = canvas.getContext('2d')!
  await page.render({ canvasContext: ctx, viewport }).promise

  // Draw bbox overlay
  drawBboxOverlay(viewport)
}

function drawBboxOverlay(viewport: any) {
  if (!overlayRef.value) return
  const overlay = overlayRef.value
  overlay.width = viewport.width
  overlay.height = viewport.height

  const ctx = overlay.getContext('2d')!
  ctx.clearRect(0, 0, overlay.width, overlay.height)

  // Only draw if we're on the target page and have bbox
  if (!props.bbox || currentPage.value !== (props.page || 1)) return

  // Docling/GraphRAG bbox uses top-left origin (same as canvas)
  // x0,y0 = top-left corner; x1,y1 = bottom-right corner (y1 > y0)
  const b = props.bbox

  const x = b.x0 * scale.value
  const y = b.y0 * scale.value
  const w = (b.x1 - b.x0) * scale.value
  const h = (b.y1 - b.y0) * scale.value

  // Highlight rectangle
  ctx.fillStyle = 'rgba(255, 102, 0, 0.15)'
  ctx.fillRect(x, y, w, h)
  ctx.strokeStyle = 'rgba(255, 102, 0, 0.7)'
  ctx.lineWidth = 2
  ctx.setLineDash([6, 3])
  ctx.strokeRect(x, y, w, h)

  // Scroll bbox into view
  nextTick(() => {
    if (containerRef.value) {
      containerRef.value.scrollTo({
        top: Math.max(0, y - 80),
        behavior: 'smooth',
      })
    }
  })
}

function prevPage() {
  if (currentPage.value > 1) {
    currentPage.value--
    renderPage()
  }
}

function nextPage() {
  if (currentPage.value < totalPages.value) {
    currentPage.value++
    renderPage()
  }
}

function zoomIn() {
  scale.value = Math.min(scale.value + 0.25, 4)
  renderPage()
}

function zoomOut() {
  scale.value = Math.max(scale.value - 0.25, 0.5)
  renderPage()
}

function close() {
  emit('update:open', false)
}

watch(() => props.open, (val) => {
  if (val) {
    nextTick(loadPdf)
  } else {
    pdfDoc = null
  }
})

watch(() => props.pdfUrl, () => {
  if (props.open) loadPdf()
})
</script>

<template>
  <Teleport to="body">
    <Transition
      enter-active-class="transition-all duration-200"
      enter-from-class="opacity-0"
      leave-active-class="transition-all duration-150"
      leave-to-class="opacity-0"
    >
      <div v-if="open" class="fixed inset-0 z-[100] flex items-center justify-center">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/70 backdrop-blur-sm" @click="close" />

        <!-- Viewer -->
        <div class="relative z-10 flex flex-col bg-card border border-border/60 rounded-xl shadow-2xl max-w-[90vw] max-h-[90vh] w-[800px]">
          <!-- Header -->
          <div class="flex items-center justify-between px-4 py-2.5 border-b border-border/40">
            <div class="flex items-center gap-2 min-w-0">
              <span class="text-xs font-medium text-foreground truncate">{{ fileName || 'PDF' }}</span>
              <span class="text-[10px] text-muted-foreground/60">
                Page {{ currentPage }} / {{ totalPages }}
              </span>
            </div>
            <div class="flex items-center gap-0.5">
              <button
                class="h-7 w-7 rounded flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
                title="Zoom out"
                @click="zoomOut"
              >
                <ZoomOut class="h-3.5 w-3.5" />
              </button>
              <span class="text-[10px] text-muted-foreground/60 w-10 text-center">{{ Math.round(scale * 100) }}%</span>
              <button
                class="h-7 w-7 rounded flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
                title="Zoom in"
                @click="zoomIn"
              >
                <ZoomIn class="h-3.5 w-3.5" />
              </button>
              <div class="w-px h-4 bg-border/40 mx-1" />
              <button
                class="h-7 w-7 rounded flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
                :disabled="currentPage <= 1"
                @click="prevPage"
              >
                <ChevronLeft class="h-3.5 w-3.5" />
              </button>
              <button
                class="h-7 w-7 rounded flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
                :disabled="currentPage >= totalPages"
                @click="nextPage"
              >
                <ChevronRight class="h-3.5 w-3.5" />
              </button>
              <div class="w-px h-4 bg-border/40 mx-1" />
              <button
                class="h-7 w-7 rounded flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
                @click="close"
              >
                <X class="h-3.5 w-3.5" />
              </button>
            </div>
          </div>

          <!-- Content -->
          <div ref="containerRef" class="flex-1 overflow-auto bg-[#1a1a1a] flex justify-center p-4">
            <!-- Loading -->
            <div v-if="loading" class="flex flex-col items-center justify-center py-20 gap-3">
              <div class="w-48 h-1.5 rounded-full bg-white/[0.06] overflow-hidden">
                <div
                  class="h-full rounded-full bg-gradient-to-r from-primary to-orange-400 transition-all duration-300"
                  :style="{ width: `${loadProgress || 5}%` }"
                />
              </div>
              <div class="text-[11px] text-muted-foreground/50">
                {{ loadProgress > 0 ? `Loading PDF... ${loadProgress}%` : 'Loading PDF...' }}
              </div>
            </div>
            <!-- Error -->
            <div v-else-if="error" class="flex items-center justify-center py-20">
              <div class="text-xs text-destructive">{{ error }}</div>
            </div>
            <!-- Canvas -->
            <div v-else class="relative inline-block shadow-lg">
              <canvas ref="canvasRef" class="block" />
              <canvas ref="overlayRef" class="absolute top-0 left-0 pointer-events-none" />
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
