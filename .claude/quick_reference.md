# OpenOps Rich Content Rendering - Quick Reference

## 🎯 Key Files & Patterns

### Knowledge Page
**File**: `frontend/pages/knowledge/index.vue`
```typescript
// Lines 108-123: Custom marked renderer
const renderer = new marked.Renderer()
renderer.code = function ({ text, lang }) {
  if (lang === 'mermaid') return `<pre class="mermaid">${text}</pre>`
  return origCode.call(this, ...)
}

// Rendering
function renderMarkdown(md: string) {
  return marked.parse(md, { async: false })
}

// In template (line 274)
<div class="prose-preview" v-html="renderMarkdown(content)" />

// Mermaid rendering (line 129)
await mermaid.run({ querySelector: '.prose-preview .mermaid' })
```

### Issues/RCA Page
**File**: `frontend/pages/issues/index.vue`
```vue
<!-- Line 270: Plain text only - NO markdown -->
<pre class="text-xs whitespace-pre-wrap">{{ selectedIssue.rca_result }}</pre>
```

### Chat Panel (Primary Rich Content Hub)
**File**: `frontend/components/layout/ChatPanel.vue`
```typescript
// Lines 19-45: Custom marked renderers
const renderer = new marked.Renderer()

// Mermaid code block detection
renderer.code = function (token) {
  if (token.lang === 'mermaid') {
    return `<pre class="mermaid">${token.text}</pre>`
  }
  return originalCode(token)
}

// GraphRAG image detection & wrapping
renderer.image = function (token) {
  if (token.href.includes('graphrag-databucket')) {
    return `<span class="graphrag-img-wrap" data-img-file="${imgFile}">
      <img src="${href}" alt="${text}" class="graphrag-img" />
      <span class="graphrag-img-badge">📄 View in PDF</span>
    </span>`
  }
  return originalImage(token)
}

// Lines 712-714: Message rendering
function renderMd(text: string): string {
  return marked.parse(text)
}

// Lines 725-744: Lazy mermaid rendering
let mermaidLoaded = false
async function renderMermaid() {
  if (!mermaidLoaded) {
    const m = await import('mermaid')
    m.default.initialize({
      startOnLoad: false,
      theme: 'dark',
      themeVariables: { primaryColor: '#FF6600', ... }
    })
  }
  const m = await import('mermaid')
  await m.default.run({ querySelector: '.chat-markdown .mermaid' })
}

// Template (line ~1042)
<div class="chat-markdown" v-html="renderMd(msg.content)" />
```

### PDF Viewer (GraphRAG Integration)
**File**: `frontend/components/ui/PdfViewer.vue`
```typescript
// Lines 87-128: Bounding box overlay drawing
function drawBboxOverlay(viewport, page) {
  // Convert PDF coords (origin bottom-left) → canvas coords (origin top-left)
  const x = bbox.x0 * scale
  const y = (pageHeight - bbox.y0) * scale  // flip Y

  // Draw with orange highlight
  ctx.fillStyle = 'rgba(255, 102, 0, 0.15)'  // 15% opacity fill
  ctx.fillRect(x, y, w, h)

  ctx.strokeStyle = 'rgba(255, 102, 0, 0.7)'  // 70% opacity border
  ctx.lineWidth = 2
  ctx.setLineDash([6, 3])
  ctx.strokeRect(x, y, w, h)

  // Auto-scroll into view
  containerRef.value.scrollTo({ top: Math.max(0, y - 80) })
}
```

---

## 🎨 CSS Classes & Styling

### Markdown Preview (Knowledge Page)
```css
.prose-preview :deep(h1) { font-size: 1.125rem; }
.prose-preview :deep(code) { background: hsl(var(--secondary)); }
.prose-preview :deep(pre) { background: hsl(var(--secondary) / 0.8); }
.prose-preview :deep(table) { width: 100%; border-collapse: collapse; }
.prose-preview :deep(blockquote) { border-left: 3px solid hsl(var(--border)); }
.prose-preview :deep(pre.mermaid) { background: transparent; text-align: center; }
```

### Chat Markdown (Main Content)
```css
.chat-markdown { font-size: 13px; line-height: 1.6; }
.chat-markdown code { background: hsl(var(--secondary) / 0.6); }
.chat-markdown pre { background: hsl(var(--secondary) / 0.8); }
.chat-markdown h1 { font-size: 1.1em; }
.chat-markdown table { border-collapse: collapse; }
.chat-markdown a { color: hsl(var(--primary)); text-decoration: underline; }
.chat-markdown blockquote { border-left: 2px solid hsl(var(--border)); }
.chat-markdown pre.mermaid { background: transparent; }

/* GraphRAG Images */
.graphrag-img-wrap { position: relative; cursor: pointer; border-radius: 6px; }
.graphrag-img-badge {
  position: absolute;
  bottom: 0; right: 0;
  background: hsl(var(--primary) / 0.9);
  opacity: 0;
}
.graphrag-img-wrap:hover .graphrag-img-badge { opacity: 1; }
```

### Color Variables (Dark Mode)
```css
--primary: 24 100% 50%           /* #FF6600 - Grafana Orange */
--secondary: 220 12% 16%         /* #23262b - Surfaces */
--muted-foreground: 220 6% 55%   /* #858688 - Muted text */
--border: 220 12% 18%            /* #2c3235 - Borders */
--foreground: 210 6% 87%         /* #d8d9da - Body text */
--background: 228 22% 7%         /* #111217 - Deep bg */
```

---

## 🔄 Rendering Flows

### Knowledge Page Flow
```
User clicks "Preview"
    ↓
Content loaded from API
    ↓
renderMarkdown(content)
    ├→ marked.parse() with custom renderer
    └→ Mermaid code blocks → <pre class="mermaid">
    ↓
Dialog opens
    ↓
showPreviewDialog watch triggered
    ↓
renderMermaidDiagrams()
    ↓
mermaid.run({ querySelector: '.prose-preview .mermaid' })
    ↓
SVG diagrams rendered
```

### Chat Message Flow
```
Claude returns markdown text
    ↓
Appended to messages array
    ↓
messages.length watch triggered
    ↓
renderMermaid() called
    ↓
marked.parse() with custom renderers
    ├→ Detect mermaid blocks → <pre class="mermaid">
    ├→ Detect graphrag-databucket URLs → clickable wrapper
    └→ Render normal markdown
    ↓
mermaid.run() on completion
    ↓
All diagrams rendered as SVG
```

### GraphRAG PDF Flow
```
User clicks "📄 View in PDF" badge on image
    ↓
onDocClick handler detects .graphrag-img-wrap
    ↓
openPdfFromGraphRag(imageFilename)
    ↓
POST /api/graphrag/bbox (get page + coordinates)
    ↓
POST /api/graphrag/pdf-url (get PDF presigned URL)
    ↓
PdfViewer opens with:
   - pdfUrl: S3 presigned URL
   - page: page number
   - bbox: { x0, y0, x1, y1 }
    ↓
drawBboxOverlay() renders orange highlight on canvas
    ↓
Auto-scroll to bbox position
```

---

## 📋 Component Overview

| Component | File | Purpose | Rich Content |
|-----------|------|---------|---|
| Knowledge | `pages/knowledge/index.vue` | Browse/edit markdown files | ✅ Full markdown + mermaid |
| Issues | `pages/issues/index.vue` | View RCA results | ❌ Pre-formatted text only |
| ChatPanel | `layout/ChatPanel.vue` | Chat with AI | ✅ Markdown + mermaid + images |
| PdfViewer | `ui/PdfViewer.vue` | View PDFs with highlights | ✅ Canvas + bbox overlay |
| DataTable | `shared/DataTable.vue` | Generic table | ❌ Slot-based rendering |

---

## 🛠️ Technology Stack

- **Markdown**: `marked@17.0.6`
  - Parser & HTML renderer
  - Custom renderers for mermaid + GraphRAG images

- **Diagrams**: `mermaid@11.14.0`
  - Lazy-loaded (avoids SSR issues)
  - Dark theme with `#FF6600` primary

- **PDF**: `pdfjs-dist@5.6.205`
  - Canvas-based rendering
  - Two-layer approach: main canvas + overlay for bbox

- **Styling**: Tailwind + shadcn-vue
  - Grafana-inspired dark theme
  - CSS variables for consistent colors

- **Framework**: Nuxt 3 + Vue 3
  - `v-html` for markdown rendering
  - `:deep()` for scoped styling

---

## ⚠️ Security & Performance Notes

### Security
- ✅ Content from APIs is rendered via `v-html`
- ⚠️ User-generated markdown (Knowledge files) rendered without DOMPurify sanitization
- ⚠️ Potential XSS if user inputs `<script>` tags in markdown

### Performance
- ✅ Mermaid only loaded when needed
- ✅ Canvas rendering efficient for PDFs
- ✅ Streaming content supported (wait for completion before mermaid.run)
- ⚠️ Lazy loading of mermaid may cause flash before rendering

### Accessibility
- ✅ Table headers marked with `<th>`
- ✅ Images have alt text
- ✅ Zoom controls on PDF viewer
- ✅ Keyboard shortcuts (Cmd+K for chat)
