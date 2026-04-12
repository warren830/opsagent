# Ops Frontend Rich Content Rendering - Exploration Summary

## 🎯 Mission Complete

I've thoroughly explored how rich content (markdown, tables, Mermaid diagrams) is rendered across the Ops frontend and created comprehensive documentation.

---

## 📊 Key Findings

### 1. **Rendering Architecture**
The app uses a **multi-pattern approach**:
- **Markdown pages** (Knowledge, Chat): `marked.parse()` via custom renderers → `v-html`
- **Plain text pages** (Issues/RCA): `<pre>` element with `whitespace-pre-wrap`
- **PDFs** (GraphRAG): Canvas-based with overlay for bounding boxes

### 2. **Core Technologies**
| Technology | Version | Purpose |
|------------|---------|---------|
| **marked** | 17.0.6 | Markdown parsing & rendering |
| **mermaid** | 11.14.0 | Diagram rendering (flowcharts, sequences, etc.) |
| **pdfjs-dist** | 5.6.205 | PDF canvas rendering |
| **tailwindcss** | Latest | Styling & theming |
| **Nuxt 3** | Latest | Vue.js framework |

### 3. **Rich Content Support by Page**

| Page | Markdown | Tables | Mermaid | Graphs | Images | PDF |
|------|----------|--------|---------|--------|--------|-----|
| **Knowledge** | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ |
| **Chat Panel** | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ |
| **Issues/RCA** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Glossary** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

### 4. **Rendering Patterns**

#### Knowledge Page (`frontend/pages/knowledge/index.vue`)
```typescript
// Custom renderer for mermaid blocks
renderer.code = ({ text, lang }) => {
  if (lang === 'mermaid') return `<pre class="mermaid">${text}</pre>`
  return standardCode
}

// Render and display
v-html="marked.parse(content)"

// Post-render diagram drawing
mermaid.run({ querySelector: '.prose-preview .mermaid' })
```

#### Chat Panel (`frontend/components/layout/ChatPanel.vue`)
```typescript
// Multiple custom renderers:
// 1. Mermaid detection: lang === 'mermaid' → <pre class="mermaid">
// 2. GraphRAG images: URL contains 'graphrag-databucket' → clickable wrapper with PDF badge
// 3. Standard markdown → normal HTML rendering

v-html="marked.parse(message.content)"
mermaid.run({ querySelector: '.chat-markdown .mermaid' })
```

#### PDF Viewer (`frontend/components/ui/PdfViewer.vue`)
```typescript
// Two-layer canvas approach:
// Layer 1: Main canvas - PDF content
// Layer 2: Overlay canvas - Bounding box highlighting

// Coordinate conversion: PDF (origin bottom-left) → Canvas (origin top-left)
// Highlight color: #FF6600 (Grafana orange) with 15% fill + 70% stroke opacity
// Auto-scroll to bbox position when rendered
```

### 5. **CSS Styling**

**Dark Theme Colors:**
```css
--primary: #FF6600           (Grafana Orange)
--background: #111217        (Deep navy)
--card: #181b1f              (Card surfaces)
--secondary: #23262b         (Secondary surfaces)
--foreground: #d8d9da        (Text)
--border: #2c3235            (Borders)
--muted-foreground: #858688  (Muted text)
```

**Key Classes:**
- `.prose-preview` - Knowledge page markdown container
- `.chat-markdown` - Chat panel markdown container
- `.graphrag-img-wrap` - Clickable GraphRAG image wrapper
- `.mermaid` - Diagram container (auto-detected)

### 6. **GraphRAG Integration**
Images from S3 (`graphrag-databucket` URLs) are:
1. Detected in marked's image renderer
2. Wrapped in clickable `<span class="graphrag-img-wrap">`
3. Show "📄 View in PDF" badge on hover
4. On click: Fetch PDF URL + bbox coordinates via backend APIs
5. Open PdfViewer with highlighting overlay

### 7. **Data Flow**

**Knowledge Preview:**
```
Click Preview → Load from API → marked.parse() → v-html → Dialog opens
→ mermaid.run() → SVG diagrams rendered
```

**Chat Message:**
```
Claude responds → Append to messages → Watch triggers → marked.parse() with custom renderers
→ v-html render → mermaid.run() on completion → All diagrams rendered
```

**GraphRAG PDF:**
```
Hover image → Click badge → openPdfFromGraphRag() → Backend API calls
→ PdfViewer opens → drawBboxOverlay() → Orange highlight rendered → Auto-scroll
```

---

## 📁 File Structure

All code resides in `/frontend/`:

### Pages
- `pages/knowledge/index.vue` - Knowledge base CRUD + preview
- `pages/issues/index.vue` - Issue detail view with RCA

### Components
- `components/layout/ChatPanel.vue` - **Main rich content hub** (1400+ lines)
- `components/ui/PdfViewer.vue` - PDF viewer with bbox overlay
- `components/shared/DataTable.vue` - Generic table component
- `components/shared/ConfirmDialog.vue` - Modal component

### Configuration
- `tailwind.config.ts` - Tailwind configuration
- `assets/css/tailwind.css` - Global CSS + CSS variables
- `nuxt.config.ts` - Nuxt configuration
- `package.json` - Dependencies

---

## 💡 Key Insights

1. **Two Renderer Patterns**: Custom marked renderers (Knowledge, Chat) vs plain text (Issues)
2. **Lazy Mermaid**: Only imported when needed (avoids SSR issues)
3. **GraphRAG Smart Integration**: Automatic S3 image detection + clickable PDF linking
4. **Canvas PDFs**: Efficient two-layer rendering with coordinate transformation
5. **Grafana Colors**: Consistent dark theme via CSS variables
6. **No Sanitization**: User markdown rendered without DOMPurify (XSS vector)

---

## ⚠️ Important Notes

### Security
- ✅ Markdown from APIs is trusted
- ⚠️ **User-generated markdown** (Knowledge files) **NOT sanitized** - potential XSS risk

### Performance
- ✅ Mermaid lazy-loaded
- ✅ Canvas PDF rendering efficient
- ✅ Streaming support (waits for completion)

### Browser Support
- PDF viewer uses `pdfjs-dist` with bundled worker
- CSS variables for theming (modern browsers)
- Smooth scrolling not universal

---

## 📚 Documentation Deliverables

Three documents saved to `/frontend/.claude/`:

### 1. **SUMMARY.md** (This file)
Overview of findings and quick reference

### 2. **INDEX.md**
Navigation guide, file references, quick task lookups

### 3. **quick_reference.md**
Code snippets, CSS classes, rendering flows, component table

### 4. **rich_content_analysis.md**
Comprehensive technical deep-dive with all details

---

## 🎯 Quick Implementation Examples

### Example 1: Add Syntax Highlighting
```typescript
// In ChatPanel.vue renderer
renderer.code = (token) => {
  const highlighted = hljs.highlight(token.text, { language: token.lang }).value
  return `<pre><code>${highlighted}</code></pre>`
}
```

### Example 2: Sanitize User Markdown
```typescript
// Before rendering
import DOMPurify from 'dompurify'
const safe = DOMPurify.sanitize(marked.parse(content))
```

### Example 3: Custom Mermaid Theme
```typescript
mermaid.initialize({
  theme: 'dark',
  themeVariables: {
    primaryColor: '#FF9830',     // Change from orange to amber
    primaryTextColor: '#ffffff',
    primaryBorderColor: '#FF6600'
  }
})
```

---

## 🚀 Next Steps (Optional Enhancements)

1. Add DOMPurify sanitization for knowledge files
2. Implement syntax highlighting (highlight.js or prism.js)
3. Add Mermaid diagram export functionality
4. Create real-time markdown preview editor
5. Add table column sorting
6. Implement custom diagram themes per content type

---

## 📞 Using the Documentation

**Start with**: `SUMMARY.md` (this file) for overview
**For navigation**: `INDEX.md` to find specific topics
**For quick lookups**: `quick_reference.md` (code snippets, CSS, flows)
**For deep understanding**: `rich_content_analysis.md` (full technical details)

All documents include:
- Line number references to source code
- Code examples with context
- CSS class definitions
- Data flow diagrams
- File paths for easy navigation
