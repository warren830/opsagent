# Ops Frontend Rich Content Analysis - Documentation Index

## 📚 Available Documents

### 1. **Comprehensive Analysis** (`rich_content_analysis.md`)
Full technical deep-dive covering:
- Core rendering stack and dependencies
- Knowledge page markdown + Mermaid rendering
- Issues/RCA page (plain text format)
- Chat panel with custom renderers
- PDF viewer with bounding box integration
- CSS styling for all content types
- Data flow diagrams
- File paths and recommendations

**Use when**: You need complete technical understanding of the system
**Size**: ~2000 lines

### 2. **Quick Reference** (`quick_reference.md`)
Condensed guide with:
- Key code snippets (with line numbers)
- CSS classes for styling
- Rendering flow diagrams
- Component overview table
- Technology stack summary
- Security & performance notes

**Use when**: You need quick lookups or are debugging specific issues
**Size**: ~400 lines

---

## 🎯 Quick Navigation

### Finding Information About...

#### Markdown Rendering
- **How it works**: See `rich_content_analysis.md` §2 & §4
- **CSS classes**: Check `quick_reference.md` § "CSS Classes & Styling"
- **Code examples**: `quick_reference.md` § "Knowledge Page" & "Chat Panel"

#### Mermaid Diagrams
- **Initialization**: `rich_content_analysis.md` §2 (Knowledge) or §4 (Chat)
- **Rendering**: Watch `quick_reference.md` § "Rendering Flows" → Knowledge Page Flow
- **Theme colors**: `rich_content_analysis.md` §1 or `quick_reference.md` § "Color Variables"

#### PDF Viewer
- **Full documentation**: `rich_content_analysis.md` §5
- **Bounding box logic**: `quick_reference.md` § "PDF Viewer"
- **GraphRAG integration**: `rich_content_analysis.md` §4 & §5

#### Tables
- **Knowledge page**: `rich_content_analysis.md` §2, CSS section
- **Chat panel**: `rich_content_analysis.md` §4, `.chat-markdown table` styles
- **Generic DataTable**: `rich_content_analysis.md` §6

#### Styling & Colors
- **Dark theme variables**: `rich_content_analysis.md` §1
- **CSS classes**: `quick_reference.md` § "CSS Classes & Styling"
- **Grafana inspiration**: `rich_content_analysis.md` §7

#### Security & Performance
- **Known issues**: `rich_content_analysis.md` §10 & `quick_reference.md` § "Security & Performance Notes"
- **Recommendations**: `rich_content_analysis.md` § "Recommendations"

---

## 📋 File References

### Pages
- **Knowledge**: `frontend/pages/knowledge/index.vue`
- **Issues/RCA**: `frontend/pages/issues/index.vue`

### Components
- **Chat Panel**: `frontend/components/layout/ChatPanel.vue`
- **PDF Viewer**: `frontend/components/ui/PdfViewer.vue`
- **DataTable**: `frontend/components/shared/DataTable.vue`
- **ConfirmDialog**: `frontend/components/shared/ConfirmDialog.vue`

### Configuration
- **Tailwind**: `frontend/tailwind.config.ts`
- **Global CSS**: `frontend/assets/css/tailwind.css`
- **Nuxt**: `frontend/nuxt.config.ts`
- **Dependencies**: `frontend/package.json`

---

## 🔍 Common Tasks

### Task: Add syntax highlighting to code blocks
1. Read `rich_content_analysis.md` §4 - Chat Panel code block section
2. Check current CSS in `quick_reference.md` - `.chat-markdown code` & `pre` classes
3. Recommendation in `rich_content_analysis.md` §11 suggests highlight.js or prism.js
4. Modify custom marked renderer in ChatPanel.vue (lines 19-45)

### Task: Customize Mermaid theme
1. Find mermaid initialization in `rich_content_analysis.md` §2 & §4
2. Modify `themeVariables` in Knowledge page (line 106) or Chat panel (line 735)
3. Reference color scheme in §1 for available colors

### Task: Add DOMPurify sanitization
1. Check security notes in `rich_content_analysis.md` §10
2. Install `dompurify` package
3. Wrap `v-html="renderMarkdown()"` with DOMPurify sanitization
4. Apply to Knowledge page (line 274) and Chat panel (line ~1042)

### Task: Export Mermaid diagrams
1. Find where diagrams are rendered: `rich_content_analysis.md` §2 & §4
2. After `mermaid.run()` completes, access rendered SVGs
3. Use mermaid's built-in export functions or canvas-based export

---

## 🚀 Implementation Overview

```
User Input
    ↓
┌───────────────────────────────────────┐
│  Knowledge Page / Chat Panel / Issues │
└───────────────────────────────────────┘
    ↓
    ├─→ Plain Text (Issues/RCA) → <pre> element
    │
    ├─→ Markdown Content
    │   ├─→ marked.parse() with custom renderer
    │   ├─→ Detect mermaid blocks → <pre class="mermaid">
    │   ├─→ Detect images (chat) → GraphRAG wrapper
    │   └─→ Output HTML
    │
    ├─→ HTML Rendered
    │   ├─→ via v-html directive
    │   └─→ CSS applied (prose-preview / chat-markdown)
    │
    └─→ Post-render Processing
        ├─→ mermaid.run() for diagram rendering
        ├─→ graphrag-img-wrap click handlers for PDF viewing
        └─→ CSS animations & styling
```

---

## 💡 Key Insights

1. **Dual Renderer Pattern**: Different pages use custom marked renderers (Knowledge + Chat) vs plain text (Issues)

2. **Lazy Mermaid Loading**: Mermaid is only imported when needed to avoid SSR issues in Nuxt

3. **GraphRAG Integration**: Images from S3 are detected via URL pattern and made clickable to view source PDFs with highlighting

4. **Canvas-based PDFs**: PDFs rendered on canvas with overlay layer for bounding box highlighting (two-layer approach)

5. **Color Consistency**: Grafana-inspired dark theme with HSL CSS variables for easy theming

6. **No Built-in Sanitization**: User-generated markdown (Knowledge files) rendered without DOMPurify - potential XSS vector

---

## 📞 Questions?

For specific issues:
1. Search by component/file in the file references
2. Check the rendering flows in quick_reference.md
3. Review CSS styling sections for visual issues
4. See security/performance notes for non-functional concerns
