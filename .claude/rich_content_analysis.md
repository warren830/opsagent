# Ops Frontend - Rich Content Rendering Analysis

## Executive Summary

The Ops frontend (Nuxt 3 + Vue 3) uses a **marked-based markdown rendering system with integrated Mermaid diagram support** across multiple pages. Rich content is rendered via `v-html` with custom `marked` renderers that handle markdown, code blocks, tables, and diagrams. The styling is **Grafana-inspired with dark theme defaults**.

---

## 1. Core Rendering Stack

### Dependencies
- **marked** (`^17.0.6`) - Markdown parser and HTML renderer
- **mermaid** (`^11.14.0`) - Diagram rendering (flowcharts, sequence diagrams, etc.)
- **pdfjs-dist** (`^5.6.205`) - PDF viewing with canvas-based rendering
- **tailwindcss** + **shadcn-vue** - UI components and styling

### Color Scheme (Dark Mode - Primary)
```css
--background: 228 22% 7%;           /* #111217 */
--foreground: 210 6% 87%;           /* #d8d9da */
--card: 225 16% 11%;                /* #181b1f */
--primary: 24 100% 50%;             /* #FF6600 (Grafana Orange) */
--secondary: 220 12% 16%;           /* #23262b */
--muted-foreground: 220 6% 55%;    /* #858688 */
--border: 220 12% 18%;              /* #2c3235 */
--success: 110 44% 58%;             /* #73BF69 */
--warning: 30 90% 58%;              /* #FF9830 */
--destructive: 354 75% 55%;         /* #F2495C */
--info: 217 85% 64%;                /* #5794F2 */
```

---

## 2. Knowledge Page (`frontend/pages/knowledge/index.vue`)

### Purpose
Management and preview of knowledge base files (markdown with optional Mermaid diagrams).

### Rendering Pattern

#### Markdown Rendering
```typescript
// Custom marked renderer: mermaid code blocks → <pre class="mermaid">
const renderer = new marked.Renderer()
const origCode = renderer.code
renderer.code = function ({ text, lang }: { text: string; lang?: string | undefined; escaped?: boolean }) {
  if (lang === 'mermaid') {
    return `<pre class="mermaid">${text}</pre>`
  }
  return origCode.call(this, { text, lang, escaped: false })
}
marked.use({ renderer })

function renderMarkdown(md: string): string {
  if (!md) return ''
  return marked.parse(md, { async: false }) as string
}
```

#### Preview Dialog Rendering
```vue
<!-- Line 274 in knowledge/index.vue -->
<div v-if="previewFile?.content"
     class="prose-preview max-w-none text-xs text-foreground/90"
     v-html="renderMarkdown(previewFile.content)" />
```

#### Mermaid Diagram Rendering
```typescript
// After preview dialog opens, render mermaid diagrams
async function renderMermaidDiagrams() {
  await nextTick()
  try {
    await mermaid.run({ querySelector: '.prose-preview .mermaid' })
  } catch { /* ignore render errors */ }
}

// Initialization
mermaid.initialize({
  startOnLoad: false,
  theme: 'dark',
  themeVariables: {
    primaryColor: '#FF6600',
    primaryTextColor: '#e0e0e0',
    lineColor: '#555'
  }
})
```

### CSS Styling for Rich Content
```css
.prose-preview :deep(h1) { font-size: 1.125rem; font-weight: 700; margin-top: 1.25rem; margin-bottom: 0.5rem; color: var(--foreground); }
.prose-preview :deep(h2) { font-size: 0.875rem; font-weight: 600; ... }
.prose-preview :deep(h3) { font-size: 0.8125rem; font-weight: 600; ... }
.prose-preview :deep(h4) { font-size: 0.75rem; font-weight: 600; ... }
.prose-preview :deep(p) { margin: 0.375rem 0; line-height: 1.6; }
.prose-preview :deep(ul), .prose-preview :deep(ol) { padding-left: 1.25rem; margin: 0.375rem 0; }
.prose-preview :deep(li) { margin: 0.125rem 0; }

/* Code blocks */
.prose-preview :deep(code) {
  font-size: 0.6875rem;
  padding: 0.125rem 0.25rem;
  border-radius: 0.25rem;
  background: hsl(var(--secondary));
  font-family: ui-monospace, monospace;
}
.prose-preview :deep(pre) {
  margin: 0.5rem 0;
  padding: 0.75rem;
  border-radius: 0.375rem;
  background: hsl(var(--secondary) / 0.8);
  overflow-x: auto;
}
.prose-preview :deep(pre code) {
  padding: 0;
  background: none;
  font-size: 0.6875rem;
  line-height: 1.5;
}

/* Tables */
.prose-preview :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin: 0.5rem 0;
  font-size: 0.6875rem;
}
.prose-preview :deep(th) {
  text-align: left;
  padding: 0.375rem 0.5rem;
  border-bottom: 1px solid hsl(var(--border) / 0.6);
  font-weight: 600;
  color: hsl(var(--muted-foreground));
}
.prose-preview :deep(td) {
  padding: 0.375rem 0.5rem;
  border-bottom: 1px solid hsl(var(--border) / 0.3);
}

/* Blockquotes */
.prose-preview :deep(blockquote) {
  border-left: 3px solid hsl(var(--border));
  padding-left: 0.75rem;
  margin: 0.5rem 0;
  color: hsl(var(--muted-foreground));
}

/* Mermaid diagrams */
.prose-preview :deep(pre.mermaid) {
  background: transparent;
  padding: 0.5rem 0;
  text-align: center;
  overflow-x: auto;
}
.prose-preview :deep(pre.mermaid svg) {
  max-width: 100%;
  height: auto;
}
```

### Supported Content
- **Markdown**: Headings, paragraphs, lists, emphasis, links
- **Code blocks**: Syntax highlighted (via marked)
- **Mermaid diagrams**: Flowcharts, sequence, state diagrams
- **Tables**: Standard GitHub-flavored markdown tables
- **Blockquotes**: With border styling

---

## 3. Issues/RCA Page (`frontend/pages/issues/index.vue`)

### Purpose
Display issues with RCA (Root Cause Analysis) results.

### Content Rendering Pattern

#### RCA Result Display
```vue
<!-- Line 268-270 in issues/index.vue -->
<div v-if="selectedIssue.rca_result" class="space-y-1">
  <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('issue.rcaProgress') }}</label>
  <pre class="text-xs text-foreground whitespace-pre-wrap rounded border border-border/60 bg-secondary/30 p-2 max-h-[200px] overflow-y-auto">{{ selectedIssue.rca_result }}</pre>
</div>
```

**Note**: RCA results are displayed as **preformatted text** (`<pre>`), NOT rendered as markdown. This preserves formatting without HTML interpretation.

#### Description Display
```vue
<!-- Line 262-264 -->
<div v-if="selectedIssue.description" class="space-y-1">
  <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('glossary.description') }}</label>
  <p class="text-xs text-foreground whitespace-pre-wrap rounded border border-border/60 bg-secondary/30 p-2">{{ selectedIssue.description }}</p>
</div>
```

#### Timeline Display
```vue
<!-- Line 274-286 -->
<div v-if="selectedIssue.timeline && selectedIssue.timeline.length > 0" class="space-y-1">
  <label class="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{{ t('issue.timeline') }}</label>
  <div class="space-y-1 rounded border border-border/60 bg-secondary/30 p-2">
    <div v-for="(entry, idx) in selectedIssue.timeline" :key="idx" class="flex items-start gap-2 text-xs">
      <span class="text-muted-foreground whitespace-nowrap shrink-0">{{ entry.time }}</span>
      <span class="text-foreground">{{ entry.event }}</span>
    </div>
  </div>
</div>
```

### Key Characteristics
- **NO markdown rendering** for RCA/description
- Uses `whitespace-pre-wrap` to preserve formatting
- Semi-transparent secondary background with border
- Fixed height overflow with scrolling

---

## 4. Chat Panel (`frontend/components/layout/ChatPanel.vue`)

### Purpose
Main chat interface with markdown, code, tables, Mermaid diagrams, and GraphRAG image integration.

### Markdown Rendering Pipeline

#### Custom Marked Renderers
```typescript
const renderer = new marked.Renderer()

// Custom code block renderer: mermaid support
const originalCode = renderer.code.bind(renderer)
renderer.code = function (token: { text: string; lang?: string | null }) {
  if (token.lang === 'mermaid') {
    return `<pre class="mermaid">${token.text}</pre>`
  }
  return originalCode(token)
}

// Custom image renderer: GraphRAG S3 image detection & clickable wrapping
const originalImage = renderer.image.bind(renderer)
renderer.image = function (token: { href: string; title: string | null; text: string }) {
  const { href, text } = token
  if (href && href.includes('graphrag-databucket')) {
    const imgMatch = href.match(/(image_\d+\.png)/)
    const imgFile = imgMatch ? imgMatch[1] : ''
    return `<span class="graphrag-img-wrap" data-img-file="${imgFile}" data-img-url="${encodeURIComponent(href)}">
      <img src="${href}" alt="${text}" class="graphrag-img" loading="lazy" />
      <span class="graphrag-img-badge">📄 View in PDF</span>
    </span>`
  }
  return originalImage(token)
}

marked.setOptions({ breaks: true, gfm: true, renderer })
```

#### Message Rendering
```vue
<!-- Line ~1042 in ChatPanel.vue -->
<div v-if="msg.type === 'text' || msg.type === 'tool_result'"
     class="chat-markdown text-[13px] leading-relaxed text-foreground/90"
     v-html="renderMd(msg.content)" />

<!-- Function definition -->
function renderMd(text: string): string {
  if (!text) return ''
  return marked.parse(text) as string
}
```

#### Mermaid Initialization & Rendering
```typescript
let mermaidLoaded = false

async function renderMermaid() {
  await nextTick()
  if (!mermaidLoaded) {
    try {
      const m = await import('mermaid')
      m.default.initialize({
        startOnLoad: false,
        theme: 'dark',
        themeVariables: {
          darkMode: true,
          background: '#181b1f',
          primaryColor: '#FF6600',
          primaryTextColor: '#e0e0e0',
          lineColor: '#555',
          fontSize: '12px'
        },
      })
      mermaidLoaded = true
    } catch { return }
  }
  try {
    const m = await import('mermaid')
    await m.default.run({ querySelector: '.chat-markdown .mermaid' })
  } catch { /* ignore incomplete mermaid during streaming */ }
}

// Watch for content changes and render mermaid
watch(() => messages.value.length, () => { scrollToBottom(); renderMermaid() })
watch(isStreaming, (val) => { if (!val) renderMermaid() })
```

#### GraphRAG Image Integration
When a markdown image URL contains `graphrag-databucket`:
1. Wrapped in clickable `<span class="graphrag-img-wrap">`
2. Displays "📄 View in PDF" badge on hover
3. Click handler opens PDF viewer with bounding box (see PdfViewer section)

### Chat Markdown CSS Styling
```css
.chat-markdown p { margin: 0 0 0.4em; }
.chat-markdown p:last-child { margin-bottom: 0; }
.chat-markdown strong { font-weight: 600; color: hsl(var(--foreground)); }

/* Inline code */
.chat-markdown code {
  font-size: 0.75em;
  padding: 0.2em 0.4em;
  border-radius: 3px;
  background: hsl(var(--secondary) / 0.6);
  font-family: ui-monospace, monospace;
}

/* Code blocks */
.chat-markdown pre {
  margin: 0.4em 0;
  padding: 0.75em;
  border-radius: 4px;
  background: hsl(var(--secondary) / 0.8);
  overflow-x: auto;
  border: 1px solid hsl(var(--border) / 0.3);
}
.chat-markdown pre code { background: none; padding: 0; font-size: inherit; }

/* Lists */
.chat-markdown ul, .chat-markdown ol { margin: 0.3em 0; padding-left: 1.4em; }
.chat-markdown li { margin: 0.15em 0; }

/* Headings */
.chat-markdown h1, .chat-markdown h2, .chat-markdown h3 { font-weight: 600; margin: 0.5em 0 0.2em; color: hsl(var(--foreground)); }
.chat-markdown h1 { font-size: 1.1em; }
.chat-markdown h2 { font-size: 1em; }
.chat-markdown h3 { font-size: 0.95em; }

/* Links */
.chat-markdown a {
  color: hsl(var(--primary));
  text-decoration: underline;
  text-underline-offset: 2px;
}

/* Blockquotes */
.chat-markdown blockquote {
  border-left: 2px solid hsl(var(--border));
  padding-left: 0.6em;
  margin: 0.3em 0;
  color: hsl(var(--muted-foreground));
}

/* Tables */
.chat-markdown table {
  border-collapse: collapse;
  margin: 0.4em 0;
  font-size: 0.9em;
  width: 100%;
}
.chat-markdown th, .chat-markdown td {
  border: 1px solid hsl(var(--border) / 0.4);
  padding: 0.3em 0.6em;
  text-align: left;
}
.chat-markdown th {
  background: hsl(var(--secondary));
  font-weight: 600;
}

/* Mermaid diagrams */
.chat-markdown pre.mermaid {
  background: transparent;
  border: none;
  padding: 0.5em 0;
  text-align: left;
  overflow-x: auto;
}
.chat-markdown pre.mermaid svg {
  height: auto;
  min-width: min-content;
}

/* Images */
.chat-markdown img {
  max-height: 600px;
  border-radius: 6px;
}

/* GraphRAG Image Styling */
.graphrag-img-wrap {
  position: relative;
  display: inline-block;
  cursor: pointer;
  border-radius: 6px;
  overflow: hidden;
}

.graphrag-img-wrap:hover {
  box-shadow: 0 0 8px hsl(var(--primary) / 0.3);
}

.graphrag-img {
  display: block;
  max-height: 600px;
  border-radius: 6px;
}

.graphrag-img-badge {
  position: absolute;
  bottom: 0;
  right: 0;
  background: hsl(var(--primary) / 0.9);
  color: white;
  padding: 0.25em 0.5em;
  font-size: 0.75em;
  border-radius: 4px 0 0;
  opacity: 0;
  transition: opacity 0.2s;
}

.graphrag-img-wrap:hover .graphrag-img-badge {
  opacity: 1;
}
```

---

## 5. PdfViewer Component (`frontend/components/ui/PdfViewer.vue`)

### Purpose
Display PDFs with canvas-based rendering, bounding box highlighting for GraphRAG-extracted content, and zoom/navigation controls.

### Core Features

#### PDF Loading
```typescript
async function loadPdf() {
  if (!props.pdfUrl) return
  loading.value = true
  error.value = ''

  try {
    await loadPdfJs()
    const loadingTask = pdfjsLib.getDocument({
      url: props.pdfUrl,
      cMapUrl: 'https://cdn.jsdelivr.net/npm/pdfjs-dist@4.0.379/cmaps/',
      cMapPacked: true,
    })
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
```

#### Bounding Box Rendering
```typescript
function drawBboxOverlay(viewport: any, page: any) {
  if (!overlayRef.value) return
  const overlay = overlayRef.value
  overlay.width = viewport.width
  overlay.height = viewport.height

  const ctx = overlay.getContext('2d')!
  ctx.clearRect(0, 0, overlay.width, overlay.height)

  // Only draw if we're on the target page and have bbox
  if (!props.bbox || currentPage.value !== (props.page || 1)) return

  // PDF coordinates: origin bottom-left, viewport transforms to top-left
  const b = props.bbox
  const pageHeight = page.view[3] // MediaBox height

  const x = b.x0 * scale.value
  const y = (pageHeight - b.y0) * scale.value  // flip Y
  const w = (b.x1 - b.x0) * scale.value
  const h = (b.y0 - b.y1) * scale.value

  // Highlight rectangle with dashed border
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
```

#### Zoom & Navigation
```typescript
function zoomIn() {
  scale.value = Math.min(scale.value + 0.25, 4)
  renderPage()
}

function zoomOut() {
  scale.value = Math.max(scale.value - 0.25, 0.5)
  renderPage()
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
```

### Rendering Strategy
- **Two canvas layers**:
  - Main canvas: PDF content
  - Overlay canvas: Bounding box annotation
- **Coordinates**: Converts PDF coordinates (origin bottom-left) to canvas coordinates
- **Colors**: Orange highlight (`#FF6600` primary color) with 15% opacity fill, 70% opacity dashed border
- **Scrolling**: Auto-scrolls to show highlighted area

---

## 6. Shared UI Components

### DataTable (`frontend/components/shared/DataTable.vue`)
Generic table component used across pages. Supports slot-based cell rendering for custom content:
```vue
<template #cell-${key}="{ row, value }">
  <!-- Custom cell content -->
</template>
```

### ConfirmDialog (`frontend/components/shared/ConfirmDialog.vue`)
Modal confirmation dialog using shadcn-vue Dialog component.

### UI Component Library
All shadcn-vue components in `/frontend/components/ui/`:
- Button, Badge, Input, Textarea
- Dialog, Select, Tooltip, Scroll-area
- Card, Skeleton, Tabs, etc.

---

## 7. Tailwind & CSS Configuration

### Tailwind Config (`frontend/tailwind.config.ts`)
- **Dark mode**: `darkMode: 'class'`
- **Theme colors**: HSL-based CSS variables for Grafana-inspired palette
- **Custom utilities**:
  - `.glow-*`: Glow effects for semantic colors
  - `.panel-accent*`: Left border accent colors
  - `.text-gradient`: Gradient text effect

### Global Styles (`frontend/assets/css/tailwind.css`)
- Root CSS variables for light/dark modes
- Smooth scrollbar styling
- Native select dropdown dark mode fixes
- Grafana-style utilities

---

## 8. Rendering Patterns Summary

| Page/Component | Content Type | Rendering Method | Rich Content Support |
|---|---|---|---|
| **Knowledge** | Markdown files | `marked.parse()` + `v-html` | ✅ Markdown, Mermaid, Tables, Code |
| **Issues/RCA** | RCA results | `<pre>` plain text | ❌ None (preserves formatting) |
| **Chat Panel** | Assistant messages | `marked.parse()` + `v-html` + custom renderers | ✅ Markdown, Mermaid, Tables, Code, GraphRAG images |
| **PdfViewer** | PDF documents | `pdfjs-dist` canvas rendering | ✅ Canvas drawing, bbox overlay |
| **Glossary** | Term descriptions | Plain text in dialog | ❌ None |
| **Approvals** | Commands | Inline `<code>` element | ❌ Code syntax only |

---

## 9. Data Flow for Rich Content

### Knowledge Preview
1. User clicks "Preview" button
2. Content loaded from API
3. `renderMarkdown()` converts markdown to HTML
4. Rendered HTML placed in DOM via `v-html`
5. Dialog opens → `watch(showPreviewDialog)` triggers
6. `renderMermaidDiagrams()` runs `mermaid.run()` to render diagrams

### Chat Message
1. Claude API returns markdown text
2. Appended to message stream
3. Watch on `messages.length` triggers
4. `renderMermaid()` called after stream completes
5. Mermaid diagrams rendered on `.chat-markdown .mermaid` selectors
6. GraphRAG images detected via URL pattern, wrapped with clickable badge

### GraphRAG PDF Viewing
1. User hovers over GraphRAG image
2. Badge appears on click
3. `openPdfFromGraphRag()` called with image filename
4. Backend APIs queried: `/api/graphrag/bbox` + `/api/graphrag/pdf-url`
5. PdfViewer opens with URL, page number, and bbox coordinates
6. Bbox overlay drawn on canvas

---

## 10. Key Implementation Notes

### Security
- **v-html usage**: Content comes from trusted sources (API, user input validated)
- **User-generated markdown** (Knowledge files): Rendered without sanitization (potential XSS risk if user inputs malicious HTML)

### Performance
- **Lazy mermaid loading**: Only imported when needed (avoids SSR issues)
- **Canvas-based PDF rendering**: Efficient for large documents
- **Streaming support**: Mermaid rendering waits for stream completion

### Accessibility
- Table headers properly marked with `<th>`
- Images have alt text
- Zoom controls for PDF viewer
- Keyboard shortcuts (Cmd+K for chat panel)

### Browser Compatibility
- PDF viewer uses `pdfjs-dist` with bundled worker
- CSS variables for consistent theming
- Smooth scrolling (not all browsers support `behavior: 'smooth'`)

---

## 11. File Paths Reference

| Path | Purpose |
|---|---|
| `frontend/pages/knowledge/index.vue` | Knowledge base management & preview |
| `frontend/pages/issues/index.vue` | Issue detail & RCA display |
| `frontend/components/layout/ChatPanel.vue` | Main chat interface with markdown |
| `frontend/components/ui/PdfViewer.vue` | PDF viewer with bbox highlighting |
| `frontend/components/shared/DataTable.vue` | Generic table component |
| `frontend/components/shared/ConfirmDialog.vue` | Confirmation modal |
| `frontend/tailwind.config.ts` | Tailwind configuration |
| `frontend/assets/css/tailwind.css` | Global CSS & theme variables |
| `frontend/nuxt.config.ts` | Nuxt configuration |
| `frontend/package.json` | Dependencies (marked, mermaid, pdfjs-dist) |

---

## Recommendations

### Enhancements
1. **Syntax highlighting**: Add `highlight.js` or `prism.js` for code blocks
2. **Markdown sanitization**: Use `DOMPurify` for user-generated content
3. **PDF annotations**: Allow adding notes to highlighted sections
4. **Markdown editing**: Real-time preview during knowledge file editing
5. **Table sorting**: Add column sort functionality for markdown tables
6. **Diagram export**: Export Mermaid diagrams as PNG/SVG

### Improvements
1. **Error boundaries**: Wrap rich content rendering in error handlers
2. **Loading states**: Show skeleton for markdown content
3. **Cache mermaid instances**: Avoid re-initialization on every message
4. **Responsive tables**: Horizontal scroll for mobile-sized tables
5. **Custom diagram themes**: Allow theming per Mermaid diagram type
