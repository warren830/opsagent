<script setup lang="ts">
const canvasRef = ref<HTMLCanvasElement>()
let animId = 0

onMounted(() => {
  const canvas = canvasRef.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')
  if (!ctx) return

  // ── Config ──
  const dotSpacing = 6        // px between dot centers
  const dotRadius = 1.8       // base dot radius
  const scrollSpeed = 0.3     // px per frame
  const text = 'loops'
  const rowGap = 30           // extra vertical gap between repeated rows

  // ── Resize ──
  function resize() {
    canvas!.width = window.innerWidth
    canvas!.height = window.innerHeight
  }
  resize()
  window.addEventListener('resize', resize)

  // ── Rasterize text into dot-matrix bitmap ──
  // Use an offscreen canvas to render large text, then sample pixels
  function rasterizeText(): boolean[][] {
    const off = document.createElement('canvas')
    const octx = off.getContext('2d')!
    const fontSize = 140 // big enough for clear dot matrix
    octx.font = `900 ${fontSize}px "Inter", "Helvetica Neue", Arial, sans-serif`
    const metrics = octx.measureText(text)
    const textW = Math.ceil(metrics.width)
    const textH = fontSize * 1.3
    off.width = textW + 20
    off.height = Math.ceil(textH) + 20

    octx.fillStyle = '#000'
    octx.fillRect(0, 0, off.width, off.height)
    octx.fillStyle = '#fff'
    octx.font = `900 ${fontSize}px "Inter", "Helvetica Neue", Arial, sans-serif`
    octx.textBaseline = 'top'
    octx.fillText(text, 10, 10)

    const imageData = octx.getImageData(0, 0, off.width, off.height)
    const grid: boolean[][] = []
    for (let y = 0; y < off.height; y += dotSpacing) {
      const row: boolean[] = []
      for (let x = 0; x < off.width; x += dotSpacing) {
        const idx = (y * off.width + x) * 4
        row.push(imageData.data[idx] > 128)
      }
      grid.push(row)
    }
    return grid
  }

  const grid = rasterizeText()
  const gridRows = grid.length
  const gridCols = grid[0]?.length || 0
  const blockW = gridCols * dotSpacing  // pixel width of one "Ops" block
  const blockH = gridRows * dotSpacing  // pixel height of one block

  // vertical repeat: how many rows needed to tile the screen
  const rowH = blockH + rowGap

  let offsetX = 0

  // Per-dot shimmer phase (random, for subtle animation)
  const phases: number[][] = grid.map(row => row.map(() => Math.random() * Math.PI * 2))

  function draw(time: number) {
    const W = canvas!.width
    const H = canvas!.height

    // Clear
    ctx!.fillStyle = '#111217'
    ctx!.fillRect(0, 0, W, H)

    // How many vertical tiles
    const numRows = Math.ceil(H / rowH) + 1
    // Center vertically
    const totalH = numRows * rowH
    const startY = (H - totalH) / 2

    // Horizontal: seamlessly tile
    const numCols = Math.ceil(W / (blockW + 60)) + 2

    for (let ry = 0; ry < numRows; ry++) {
      // Alternate rows scroll in opposite direction for depth
      const dir = ry % 2 === 0 ? 1 : -1
      const rowOffsetX = (offsetX * dir) % (blockW + 60)

      const baseY = startY + ry * rowH

      for (let cx = -1; cx < numCols; cx++) {
        const baseX = cx * (blockW + 60) + rowOffsetX

        for (let r = 0; r < gridRows; r++) {
          for (let c = 0; c < gridCols; c++) {
            if (!grid[r][c]) continue

            const px = baseX + c * dotSpacing
            const py = baseY + r * dotSpacing

            // Skip if off screen
            if (px < -dotSpacing || px > W + dotSpacing || py < -dotSpacing || py > H + dotSpacing) continue

            // Shimmer: subtle brightness variation
            const phase = phases[r][c]
            const shimmer = 0.5 + 0.5 * Math.sin(time * 0.001 + phase)
            const alpha = 0.18 + shimmer * 0.22

            ctx!.beginPath()
            ctx!.arc(px, py, dotRadius + shimmer * 0.6, 0, Math.PI * 2)
            ctx!.fillStyle = `rgba(255, 102, 0, ${alpha})`
            ctx!.fill()
          }
        }
      }
    }

    offsetX += scrollSpeed

    animId = requestAnimationFrame(draw)
  }

  animId = requestAnimationFrame(draw)

  onUnmounted(() => {
    cancelAnimationFrame(animId)
    window.removeEventListener('resize', resize)
  })
})
</script>

<template>
  <div class="min-h-screen flex items-center justify-center bg-[#111217] relative overflow-hidden">
    <!-- Dot-matrix scrolling wall -->
    <canvas ref="canvasRef" class="absolute inset-0 pointer-events-none" />

    <!-- Subtle center glow -->
    <div class="absolute inset-0 pointer-events-none" style="background: radial-gradient(ellipse 60% 50% at 50% 50%, rgba(255,102,0,0.05) 0%, transparent 70%)" />

    <!-- Vignette — darken edges -->
    <div class="absolute inset-0 pointer-events-none" style="background: radial-gradient(ellipse 70% 70% at 50% 50%, transparent 20%, rgba(17,18,23,0.92) 100%)" />

    <!-- Content -->
    <div class="relative z-10">
      <slot />
    </div>
  </div>
</template>
