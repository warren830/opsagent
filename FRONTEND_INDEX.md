# OpenOps Frontend - Complete Documentation Index

## 📚 Documentation Files

This directory contains comprehensive documentation for the OpenOps frontend application. Three files provide different perspectives:

### 1. **FRONTEND_ARCHITECTURE.md** (Complete Reference)
**Size:** ~633 lines | **Depth:** Very Deep  
**Best For:** Understanding the full system, building new features, detailed reference  
**Contents:**
- Full project overview and tech stack
- Complete directory structure with descriptions
- All 10 page components with features and i18n keys
- Layout system (default.vue, auth.vue)
- All UI layout components (AppHeader, AppSidebar, etc.)
- Complete Pinia store documentation
- useApi composable reference
- Middleware details
- Full i18n keys structure
- Tailwind CSS color system
- Configuration files explanation
- Deployment strategy
- Feature completion status table
- Development insights and patterns
- Next development steps (Phase 3-6)

**Start here if you want:** Complete understanding, building new pages, adding features

---

### 2. **FRONTEND_QUICK_REFERENCE.md** (Quick Lookup)
**Size:** ~360 lines | **Depth:** Medium  
**Best For:** Quick lookups, API reference, common patterns  
**Contents:**
- File navigation map with emojis
- Navigation hierarchy (site map)
- Authentication flow
- Color system table (light/dark modes)
- i18n keys quick lookup (all keys organized)
- Pinia store API reference
- useApi composable examples
- Common page patterns (code snippets)
- Component patterns (button, card, input, link)
- npm scripts
- Feature status table
- Expected API endpoints

**Start here if you want:** Quick answers, code snippets, specific lookups

---

### 3. **FRONTEND_VISUAL_LAYOUT.md** (Diagrams & Flows)
**Size:** ~320 lines | **Depth:** Visual/Conceptual  
**Best For:** Understanding UI layout, data flows, component hierarchy  
**Contents:**
- ASCII diagrams of all page layouts
- Component structure diagrams
- Color palette visualization
- Responsive behavior (mobile vs desktop)
- Data flow architecture diagram
- Authentication state flow diagram
- Component composition breakdowns

**Start here if you want:** Visual understanding, design decisions, component relationships

---

## 🎯 Quick Navigation by Task

### "I need to..."

#### 🔍 **...understand the overall frontend structure**
→ Read: **FRONTEND_ARCHITECTURE.md** (Sections 1-5)

#### 🏗️ **...add a new page**
→ Read: **FRONTEND_ARCHITECTURE.md** (Section 3) + **FRONTEND_QUICK_REFERENCE.md** (Component Patterns)

#### 🎨 **...modify the layout or navigation**
→ Read: **FRONTEND_VISUAL_LAYOUT.md** (Layout sections) + **FRONTEND_ARCHITECTURE.md** (Section 5)

#### 🔐 **...understand authentication**
→ Read: **FRONTEND_QUICK_REFERENCE.md** (Authentication Flow) + **FRONTEND_VISUAL_LAYOUT.md** (Authentication State Flow)

#### 📱 **...make it responsive**
→ Read: **FRONTEND_VISUAL_LAYOUT.md** (Responsive Behavior) + **FRONTEND_ARCHITECTURE.md** (Section 10.4)

#### 🌍 **...add or modify translations**
→ Read: **FRONTEND_ARCHITECTURE.md** (Section 9) + **FRONTEND_QUICK_REFERENCE.md** (i18n Keys)

#### 🎨 **...change colors or theme**
→ Read: **FRONTEND_VISUAL_LAYOUT.md** (Color Palette) + **FRONTEND_ARCHITECTURE.md** (Section 10)

#### 📡 **...integrate with backend API**
→ Read: **FRONTEND_QUICK_REFERENCE.md** (useApi Composable) + **FRONTEND_ARCHITECTURE.md** (Section 7.1)

#### 🗺️ **...understand site navigation**
→ Read: **FRONTEND_QUICK_REFERENCE.md** (Navigation Hierarchy) + **FRONTEND_VISUAL_LAYOUT.md** (Overall Layout)

#### 👤 **...implement role-based access**
→ Read: **FRONTEND_ARCHITECTURE.md** (Section 5.2) + **FRONTEND_QUICK_REFERENCE.md** (State Management)

#### 🚀 **...prepare for production deployment**
→ Read: **FRONTEND_ARCHITECTURE.md** (Sections 11, 12) + **FRONTEND_QUICK_REFERENCE.md** (Docker Build)

#### 💾 **...understand state management**
→ Read: **FRONTEND_QUICK_REFERENCE.md** (State Management section) + **FRONTEND_ARCHITECTURE.md** (Section 6)

---

## 🔧 File Reference Quick Links

| File | Used For | Location |
|------|----------|----------|
| `app.vue` | Root wrapper | `/frontend/app.vue` |
| `nuxt.config.ts` | Nuxt config | `/frontend/nuxt.config.ts` |
| `tailwind.config.ts` | Tailwind theme | `/frontend/tailwind.config.ts` |
| `AppHeader.vue` | Top nav | `/frontend/components/layout/AppHeader.vue` |
| `AppSidebar.vue` | Left nav | `/frontend/components/layout/AppSidebar.vue` |
| `useApi.ts` | HTTP client | `/frontend/composables/useApi.ts` |
| `auth.ts` (store) | Auth state | `/frontend/stores/auth.ts` |
| `auth.ts` (middleware) | Auth guard | `/frontend/middleware/auth.ts` |
| `en.json` | English labels | `/frontend/i18n/en.json` |
| `zh.json` | Chinese labels | `/frontend/i18n/zh.json` |
| `login.vue` | Login page | `/frontend/pages/login.vue` |
| `index.vue` | Dashboard | `/frontend/pages/index.vue` |
| `chat.vue` | Chat page | `/frontend/pages/chat.vue` |
| `tailwind.css` | Theme vars | `/frontend/assets/css/tailwind.css` |

---

## 📊 Feature Status at a Glance

| Feature | Status | Phase | Where |
|---------|--------|-------|-------|
| ✅ Authentication | Complete | - | `stores/auth.ts` + `pages/login.vue` |
| ✅ Dashboard | Basic | 2 | `pages/index.vue` |
| 🚧 Chat/AI | In Progress | 3 | `pages/chat.vue` |
| 🔨 Tenant Management | Scaffolded | 4 | `pages/tenants/index.vue` |
| 🔨 User Management | Scaffolded | 4 | `pages/users/index.vue` |
| 🔨 Skills | Planned | 5 | `pages/skills/index.vue` |
| 🔨 Knowledge Base | Planned | 5 | `pages/knowledge/index.vue` |
| 🔨 LLM Providers | Planned | 5 | `pages/providers/index.vue` |
| 🔨 Cloud Accounts | Planned | 5 | `pages/accounts/index.vue` |
| 🔨 MCP Servers | Planned | 5 | `pages/mcp/index.vue` |
| 🔨 Settings | Planned | 6 | `pages/settings/index.vue` |

---

## 🏗️ Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│ Frontend: Nuxt 3 + Vue 3 + Tailwind CSS             │
├─────────────────────────────────────────────────────┤
│                                                      │
│ Pages (File-based routing)                          │
│  ├─ login.vue (/login)                             │
│  ├─ index.vue (/)                                  │
│  ├─ chat.vue (/chat)                               │
│  ├─ tenants/index.vue (/tenants)                   │
│  ├─ users/index.vue (/users)                       │
│  └─ ... (9 more pages)                             │
│                                                      │
│ Components (Auto-imported)                          │
│  ├─ AppHeader (sticky top nav)                     │
│  ├─ AppSidebar (left nav with role filter)         │
│  ├─ ThemeToggle (dark/light)                       │
│  └─ LangSwitch (EN/ZH)                             │
│                                                      │
│ Layouts (Page wrappers)                             │
│  ├─ default.vue (header + sidebar)                 │
│  └─ auth.vue (centered login)                      │
│                                                      │
│ Composables (Shared logic)                          │
│  └─ useApi (HTTP wrapper)                          │
│                                                      │
│ Stores (Pinia state management)                     │
│  └─ auth (user state, login/logout)                │
│                                                      │
│ Middleware (Route guards)                           │
│  └─ auth (redirects to /login if needed)           │
│                                                      │
│ i18n (Internationalization)                         │
│  ├─ en.json (English)                              │
│  └─ zh.json (Chinese)                              │
│                                                      │
└─────────────────────────────────────────────────────┘
        ↓ useApi()
┌─────────────────────────────────────────────────────┐
│ Backend: Rust + HTTP API (localhost:3080)           │
├─────────────────────────────────────────────────────┤
│ /api/auth/login, /api/auth/logout, /api/auth/me   │
│ /api/tenants, /api/users, etc.                     │
└─────────────────────────────────────────────────────┘
```

---

## 💡 Key Concepts

### 1. **Nuxt 3**
- File-based routing (pages → routes)
- Auto-imports (no import statements needed)
- SSR (Server-Side Rendering)
- Composition API with `<script setup>`

### 2. **Vue 3**
- Reactive references (`ref`, `computed`)
- Component composition
- Template syntax with directives

### 3. **Pinia**
- Centralized state management
- Stores for authentication
- Getters for computed state
- Actions for mutations

### 4. **Tailwind CSS**
- Utility-first CSS
- HSL color variables for theming
- Responsive classes (md:, lg:, etc.)
- Dark mode support (`.dark` class)

### 5. **i18n**
- Multi-language support (EN + ZH)
- `useI18n()` for accessing translations
- `$t()` for inline translations
- Cookie persistence

---

## 🔗 Relationships

### Authentication Flow
```
pages/login.vue
  ↓ calls
useAuthStore().login()
  ↓ calls
useApi().post('/api/auth/login')
  ↓ returns
{ user: User, token: string }
  ↓ sets
authStore.user, authStore.isAuthenticated
  ↓ navigates to
pages/index.vue (dashboard)
  ↓ middleware runs
middleware/auth.ts
  ↓ allows access because
authStore.isAuthenticated === true
```

### Navigation Filtering
```
components/layout/AppSidebar.vue
  ↓ filters items by
authStore.isSuperAdmin
  ↓ if true, shows
/tenants, /users (super admin only)
  ↓ if false, hides those items
```

### i18n Integration
```
Any component
  ↓ uses
const { t } = useI18n()
  ↓ calls
t('nav.dashboard')
  ↓ looks up in
i18n/zh.json (or i18n/en.json)
  ↓ returns
"仪表盘" (Chinese) or "Dashboard" (English)
```

---

## 📋 Development Workflow

### Adding a New Page
1. Create file: `pages/myfeature/index.vue`
2. Add i18n keys to `i18n/en.json` and `i18n/zh.json`
3. Add navigation item to `AppSidebar.vue` (in navItems array)
4. Use `useApi()`, `useAuthStore()`, `useI18n()` as needed
5. Route is auto-generated as `/myfeature`

### Modifying Colors
1. Edit `assets/css/tailwind.css` (HSL variables)
2. For light mode: modify `:root` section
3. For dark mode: modify `.dark` section
4. All color-based classes auto-update

### Adding New Translations
1. Add key to `i18n/en.json` with English value
2. Add key to `i18n/zh.json` with Chinese value
3. In component: `const { t } = useI18n()`
4. Use: `{{ t('section.key') }}`

### Calling Backend API
1. In component: `const api = useApi()`
2. Call: `const data = await api.get('/api/endpoint')`
3. Or: `await api.post('/api/endpoint', { body })`
4. Errors are thrown as `ApiError` with `.status` property

---

## 🚀 Production Deployment

**Build Process:**
```bash
npm ci              # Install exact dependencies
npm run build       # Generates .output/ directory
```

**Docker Deployment:**
```dockerfile
Stage 1: npm ci + npm run build
Stage 2: node .output/server/index.mjs (Port 3000)
```

**Environment Variables:**
- `NUXT_PUBLIC_API_BASE`: Backend API base URL

---

## 📞 Support

### Common Issues

**Q: Where do I add a new navigation item?**
A: Edit `AppSidebar.vue` → `navItems` computed property (add to array)

**Q: How do I make something super-admin only?**
A: Add `superAdminOnly: true` to the nav item, or check `authStore.isSuperAdmin` in component

**Q: Where are the API calls made?**
A: All go through `useApi()` composable (GET, POST, PUT, DELETE methods)

**Q: How do I switch languages?**
A: Click the language button in the header, or use `setLocale()` in code

**Q: Where are the colors defined?**
A: `assets/css/tailwind.css` (HSL variables) + `tailwind.config.ts` (theme mapping)

---

## 📄 Document Versions

| Document | Lines | Updated | Focus |
|----------|-------|---------|-------|
| FRONTEND_ARCHITECTURE.md | 633 | 2026-04-05 | Complete reference |
| FRONTEND_QUICK_REFERENCE.md | 360 | 2026-04-05 | Quick lookups |
| FRONTEND_VISUAL_LAYOUT.md | 320 | 2026-04-05 | Diagrams & flows |
| FRONTEND_INDEX.md | This file | 2026-04-05 | Navigation hub |

---

**Last Updated:** 2026-04-05  
**Framework:** Nuxt 3 + Vue 3 + Tailwind CSS  
**For Questions:** Refer to appropriate documentation file above
