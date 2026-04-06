# OpenOps Frontend - Quick Reference Guide

## 📁 File Navigation Map

```
frontend/
├── 🎯 app.vue                      → Root component wrapper
├── ⚙️  nuxt.config.ts              → Main Nuxt config (modules, i18n, API proxy)
├── 🎨 tailwind.config.ts           → Tailwind theme (colors, breakpoints)
├── 📦 package.json                 → Dependencies (Nuxt, Pinia, Tailwind, i18n)
│
├── 🎨 assets/css/tailwind.css      → HSL color variables for dark/light modes
│
├── 🧩 components/layout/
│   ├── AppHeader.vue              → Logo + Username + Theme + Lang + Logout
│   ├── AppSidebar.vue             → Navigation menu (role-filtered)
│   ├── ThemeToggle.vue            → Dark/Light mode button
│   └── LangSwitch.vue             → EN/ZH language button
│
├── 📚 composables/
│   └── useApi.ts                  → HTTP client (GET, POST, PUT, DELETE)
│
├── 📐 layouts/
│   ├── default.vue                → Header + Sidebar + Main (for app pages)
│   └── auth.vue                   → Centered container (for login)
│
├── 🚪 middleware/
│   └── auth.ts                    → Auth guard (redirects to /login if not auth)
│
├── 🌐 i18n/
│   ├── en.json                    → English translations (all UI labels)
│   └── zh.json                    → Chinese translations (all UI labels)
│
├── 📄 pages/                      → File-based routing (auto-generates routes)
│   ├── login.vue                  → /login (auth layout)
│   ├── index.vue                  → / (dashboard with 4 stat cards)
│   ├── chat.vue                   → /chat (message interface)
│   ├── tenants/index.vue          → /tenants (super admin only)
│   ├── users/index.vue            → /users (super admin only)
│   ├── skills/index.vue           → /skills (placeholder - Phase 5)
│   ├── knowledge/index.vue        → /knowledge (placeholder - Phase 5)
│   ├── providers/index.vue        → /providers (placeholder - Phase 5)
│   ├── accounts/index.vue         → /accounts (placeholder - Phase 5)
│   ├── mcp/index.vue              → /mcp (placeholder - Phase 5)
│   └── settings/index.vue         → /settings (placeholder - Phase 6)
│
├── 🏪 stores/
│   └── auth.ts                    → Pinia store (user state, login/logout/fetchMe)
│
├── 🖥️  server/api/                 → (empty, reserved for Nitro API routes)
└── 🔌 plugins/                    → (empty, reserved for Nuxt plugins)
```

## 🗺️ Navigation Hierarchy

```
LOGIN (/login)
  ↓ [Authenticated]
  ├─ DASHBOARD (/)                    [All users]
  │
  ├─ CHAT (/chat)                     [All users]
  │  └─ → Claude integration (Phase 3)
  │
  ├─ TENANTS (/tenants)               [Super Admin only]
  ├─ USERS (/users)                   [Super Admin only]
  │
  ├─ SKILLS (/skills)                 [All users, Phase 5]
  ├─ KNOWLEDGE (/knowledge)           [All users, Phase 5]
  ├─ PROVIDERS (/providers)           [All users, Phase 5]
  ├─ ACCOUNTS (/accounts)             [All users, Phase 5]
  ├─ MCP (/mcp)                       [All users, Phase 5]
  └─ SETTINGS (/settings)             [All users, Phase 6]
```

## 🔐 Authentication Flow

```
1. User visits /login
   ↓ [auth.vue layout - centered]
2. User enters username + password
   ↓ [handleLogin()]
3. authStore.login(username, password)
   ↓ [POST /api/auth/login]
4. Backend returns { user, token }
   ↓ [authStore.user = user, isAuthenticated = true]
5. Router.push('/') → Dashboard
   ↓ [default.vue layout with sidebar]
6. Auth middleware runs on page mount
   ↓ [authStore.fetchMe() to verify session]
7. If not authenticated → redirect to /login
```

## 🎨 Color System (Tailwind Variables)

| Variable | Light | Dark |
|----------|-------|------|
| `--background` | White (#fff) | Dark Blue (#1e1a2e) |
| `--foreground` | Dark Blue (#0f172a) | Almost White (#f0f9ff) |
| `--primary` | Dark Blue (#1e3a8a) | Almost White (#f0f9ff) |
| `--secondary` | Light Gray (#f1f5f9) | Dark Gray (#334155) |
| `--accent` | Light Gray (#f1f5f9) | Dark Gray (#334155) |
| `--muted` | Light Gray (#f1f5f9) | Dark Gray (#334155) |
| `--destructive` | Red (#dc2626) | Dark Red (#7f1d1d) |

**All colors use HSL for better theme switching!**

## 📝 i18n Keys Quick Lookup

### Navigation (nav)
```
nav.dashboard, nav.chat, nav.tenants, nav.users
nav.skills, nav.knowledge, nav.providers, nav.accounts
nav.mcp, nav.settings
```

### Authentication (auth)
```
auth.login, auth.logout, auth.username, auth.password
auth.loginTitle, auth.loginDescription, auth.loginButton, auth.loginError
```

### Dashboard (dashboard)
```
dashboard.title, dashboard.welcome, dashboard.activeSessions
dashboard.totalTenants, dashboard.totalUsers, dashboard.totalSkills
```

### Chat (chat)
```
chat.title, chat.placeholder, chat.send, chat.thinking
chat.usingTool, chat.newChat, chat.history
```

### Common (common)
```
common.save, common.cancel, common.confirm, common.search
common.loading, common.noData, common.success, common.error
common.enabled, common.disabled
```

### Theme (theme)
```
theme.light, theme.dark, theme.system
```

## 💾 State Management (Pinia)

### Auth Store (`stores/auth.ts`)

**State:**
```typescript
{
  user: {
    id: string
    username: string
    role: 'super_admin' | 'tenant_admin'
    tenant_id: string | null
    email: string | null
  } | null
  isAuthenticated: boolean
  isLoading: boolean
}
```

**Getters:**
```typescript
authStore.isSuperAdmin        // boolean
authStore.tenantId            // string | null
authStore.user                // User | null
authStore.isAuthenticated     // boolean
authStore.isLoading           // boolean
```

**Actions:**
```typescript
await authStore.fetchMe()                    // GET /api/auth/me
await authStore.login(username, password)   // POST /api/auth/login
await authStore.logout()                    // POST /api/auth/logout
```

## 🌐 API Composable (`composables/useApi.ts`)

```typescript
const api = useApi()

// Methods (all return Promise<T>)
await api.get<T>(url: string)
await api.post<T>(url: string, body?: any)
await api.put<T>(url: string, body?: any)
await api.del<T>(url: string)

// Example:
const user = await api.get<User>('/api/auth/me')
const result = await api.post('/api/tenants', { name: 'New Tenant' })

// Error handling:
try {
  await api.post(...)
} catch (error) {
  if (error instanceof ApiError) {
    console.log(error.status)    // HTTP status
    console.log(error.message)   // Error message
  }
}
```

## 🎯 Common Page Patterns

### Empty Scaffold (Phase 5/6)
```vue
<script setup lang="ts">
definePageMeta({ middleware: 'auth' })
const { t } = useI18n()
</script>

<template>
  <div class="space-y-6">
    <h1 class="text-2xl font-bold">{{ t('nav.skills') }}</h1>
    <div class="rounded-lg border bg-card text-card-foreground shadow-sm p-6">
      <p class="text-muted-foreground">🚧 {{ t('common.noData') }} — Phase 5</p>
    </div>
  </div>
</template>
```

### Management Page (with Create button)
```vue
<script setup lang="ts">
definePageMeta({ middleware: 'auth' })
const { t } = useI18n()
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <h1 class="text-2xl font-bold">{{ t('tenant.title') }}</h1>
      <button class="inline-flex items-center justify-center rounded-md text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 h-9 px-4">
        {{ t('tenant.create') }}
      </button>
    </div>
    <div class="rounded-lg border bg-card text-card-foreground shadow-sm p-6">
      <p class="text-muted-foreground">{{ t('common.noData') }}</p>
    </div>
  </div>
</template>
```

## 🎭 Component Patterns (No Component Library!)

### Button
```vue
<button
  class="inline-flex items-center justify-center rounded-md text-sm font-medium 
         ring-offset-background transition-colors bg-primary text-primary-foreground 
         hover:bg-primary/90 h-9 px-4"
>
  Label
</button>
```

### Card
```vue
<div class="rounded-lg border bg-card text-card-foreground shadow-sm p-6">
  Content
</div>
```

### Input
```vue
<input
  class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm 
         ring-offset-background placeholder:text-muted-foreground 
         focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
/>
```

### Navigation Link (Active State)
```vue
<NuxtLink
  to="/path"
  class="flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors"
  :class="[
    isActive ? 'bg-accent text-accent-foreground font-medium' : 'text-muted-foreground hover:bg-accent'
  ]"
>
  Icon + Label
</NuxtLink>
```

## 📊 Directory Commands

```bash
# Development
npm run dev                # Start dev server (port 3000)
npm run build              # Build for production
npm run preview            # Preview production build
npm run lint               # Run ESLint

# API Proxy (dev only)
# /api/** → http://localhost:3080/api/**
# /health → http://localhost:3080/health
```

## 🐳 Docker Build

```dockerfile
# Multi-stage: Builder + Runtime
# Stage 1: npm ci, npm run build → .output
# Stage 2: node .output/server/index.mjs (Port 3000)
```

## 📱 Responsive Breakpoints (Tailwind)

- `hidden md:flex` → Hide on mobile, show on medium+ screens
- `md:grid-cols-2 lg:grid-cols-4` → 2 cols on tablet, 4 cols on desktop

## ✨ Key Features Status

| Feature | ✅/🚧/🔨 | Location | Phase |
|---------|---------|----------|-------|
| Login/Auth | ✅ | pages/login.vue, stores/auth.ts | - |
| Dashboard | ✅ | pages/index.vue | 2 |
| Chat | 🚧 | pages/chat.vue | 3 |
| Tenant Mgmt | 🔨 | pages/tenants/index.vue | 4 |
| User Mgmt | 🔨 | pages/users/index.vue | 4 |
| Skills | 🔨 | pages/skills/index.vue | 5 |
| Knowledge | 🔨 | pages/knowledge/index.vue | 5 |
| Providers | 🔨 | pages/providers/index.vue | 5 |
| Accounts | 🔨 | pages/accounts/index.vue | 5 |
| MCP | 🔨 | pages/mcp/index.vue | 5 |
| Settings | 🔨 | pages/settings/index.vue | 6 |

## 🔗 API Endpoints Expected

```
POST   /api/auth/login         → { user, token }
POST   /api/auth/logout        → void
GET    /api/auth/me            → User
GET    /api/tenants            → Tenant[]
POST   /api/tenants            → Tenant
PUT    /api/tenants/:id        → Tenant
DELETE /api/tenants/:id        → void
GET    /api/users              → User[]
POST   /api/users              → User
PUT    /api/users/:id          → User
DELETE /api/users/:id          → void
GET    /health                 → { status: 'ok' }
```

---

**Generated:** 2026-04-05  
**Last Updated:** By Claude Code Exploration  
**Framework:** Nuxt 3 + Vue 3 + Tailwind CSS  
