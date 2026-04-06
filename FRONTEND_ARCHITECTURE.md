# OpenOps Frontend - Complete Architecture & UI Analysis

## 1. PROJECT OVERVIEW

**Name:** OpenOps  
**Description:** AI-powered multi-cloud operations platform  
**Tech Stack:** 
- **Framework:** Nuxt 3 (Vue 3)
- **Styling:** Tailwind CSS + Tailwind Animate
- **UI Components:** Radix Vue + shadcn-vue pattern (CVA)
- **Icons:** Lucide Vue Next
- **State Management:** Pinia
- **Internationalization:** @nuxtjs/i18n (English & Chinese)
- **Theme:** Dark/Light mode via @nuxtjs/color-mode
- **Runtime:** Node.js 20 (SSR - Server-Side Rendering)

## 2. FRONTEND DIRECTORY STRUCTURE

```
frontend/
├── app.vue                      # Root component (NuxtLayout + NuxtPage)
├── nuxt.config.ts              # Nuxt configuration
├── tailwind.config.ts           # Tailwind CSS configuration
├── package.json                 # Dependencies & scripts
├── tsconfig.json                # TypeScript config (extends .nuxt/tsconfig.json)
├── 
├── assets/
│   └── css/
│       └── tailwind.css         # Base Tailwind styles with HSL color variables
├── 
├── components/                  # Vue components (auto-imported)
│   └── layout/
│       ├── AppHeader.vue        # Top navigation bar with logo, user info, theme toggle
│       ├── AppSidebar.vue       # Left sidebar navigation with role-based access
│       ├── ThemeToggle.vue      # Dark/Light theme switcher
│       └── LangSwitch.vue       # Language switcher (EN/ZH)
├── 
├── composables/
│   └── useApi.ts                # API client composable (HTTP methods: GET, POST, PUT, DELETE)
├── 
├── layouts/
│   ├── default.vue              # Main layout (AppHeader + AppSidebar)
│   └── auth.vue                 # Auth layout (centered, no nav)
├── 
├── middleware/
│   └── auth.ts                  # Auth guard - redirects unauthenticated to /login
├── 
├── i18n/                        # Internationalization files
│   ├── en.json                  # English translations
│   └── zh.json                  # Chinese (Simplified) translations
├── 
├── pages/                       # File-based routing
│   ├── index.vue                # Dashboard page (/)
│   ├── login.vue                # Login page (/login)
│   ├── chat.vue                 # Chat/AI page (/chat)
│   ├── tenants/
│   │   └── index.vue            # Tenant management (/tenants) - Super Admin only
│   ├── users/
│   │   └── index.vue            # User management (/users) - Super Admin only
│   ├── skills/
│   │   └── index.vue            # Skills management (/skills)
│   ├── knowledge/
│   │   └── index.vue            # Knowledge base (/knowledge)
│   ├── providers/
│   │   └── index.vue            # LLM Providers (/providers)
│   ├── accounts/
│   │   └── index.vue            # Cloud Accounts (/accounts)
│   ├── mcp/
│   │   └── index.vue            # MCP Servers (/mcp)
│   └── settings/
│       └── index.vue            # Settings (/settings)
├── 
├── stores/
│   └── auth.ts                  # Pinia store - Authentication state & actions
├── 
├── server/
│   └── api/                     # (empty) - Reserved for server routes
│
└── plugins/                     # (empty) - Reserved for Nuxt plugins

```

## 3. PAGES & FEATURES BREAKDOWN

### 3.1 Authentication Pages

#### `/login.vue` - Login Page
**Layout:** `auth.vue` (centered, no header/sidebar)  
**Features:**
- Username & password input fields
- Error message display
- Loading state
- Theme toggle (bottom)
- Language switcher (bottom)
- Sign-in button
- Uses `useAuthStore().login()` to authenticate

**Translations (i18n keys):**
```
auth.login, auth.logout, auth.username, auth.password, auth.loginTitle,
auth.loginDescription, auth.loginButton, auth.loginError
```

### 3.2 Dashboard Pages

#### `/index.vue` - Dashboard/Home Page
**Layout:** `default.vue` (with sidebar + header)  
**Features:**
- Welcome message with user's name
- 4 Stats Cards in grid layout:
  - Active Sessions (placeholder: 0)
  - Total Tenants (placeholder: 0)
  - Total Users (placeholder: 0)
  - Total Skills (placeholder: 0)
- Role-aware display (user's username and role shown in header)

**Translations (i18n keys):**
```
dashboard.title, dashboard.welcome, dashboard.activeSessions,
dashboard.totalTenants, dashboard.totalUsers, dashboard.totalSkills
```

#### `/chat.vue` - AI Chat/Assistant Page
**Layout:** `default.vue`  
**Status:** Phase 3 (Claude integration coming)  
**Features:**
- Message display area (user messages right-aligned, assistant left-aligned)
- Input box for new messages
- "New Chat" button (top-right)
- Loading state with pulsing "Thinking..." message
- Message history display
- Currently shows placeholder: "🚧 Claude integration coming in Phase 3. Backend is ready!"

**Translations (i18n keys):**
```
chat.title, chat.placeholder, chat.send, chat.thinking,
chat.usingTool, chat.newChat, chat.history
```

### 3.3 Management Pages (Super Admin Only)

#### `/tenants/index.vue` - Tenant Management
**Layout:** `default.vue`  
**Access:** Super Admin only (filtered in sidebar)  
**Status:** Scaffolded, no data implementation  
**Features:**
- Title header
- "Create Tenant" button
- Empty state message

**Translations (i18n keys):**
```
tenant.title, tenant.create, tenant.name, tenant.slug,
tenant.awsAccounts, tenant.actions, tenant.edit, tenant.delete,
tenant.confirmDelete
```

#### `/users/index.vue` - User Management
**Layout:** `default.vue`  
**Access:** Super Admin only (filtered in sidebar)  
**Status:** Scaffolded, no data implementation  
**Features:**
- Title header
- "Create User" button
- Empty state message

**Translations (i18n keys):**
```
user.title, user.create, user.username, user.email,
user.role, user.tenant, user.status, user.active,
user.inactive, user.superAdmin, user.tenantAdmin
```

### 3.4 Feature Pages (All Authenticated Users)

#### `/skills/index.vue` - Skills Management
**Layout:** `default.vue`  
**Status:** Phase 5 (Planned)  
**Features:** Placeholder only - "🚧 No data — Phase 5"

#### `/knowledge/index.vue` - Knowledge Base
**Layout:** `default.vue`  
**Status:** Phase 5 (Planned)  
**Features:** Placeholder only - "🚧 No data — Phase 5"

#### `/providers/index.vue` - LLM Providers
**Layout:** `default.vue`  
**Status:** Phase 5 (Planned)  
**Features:** Placeholder only - "🚧 No data — Phase 5"

#### `/accounts/index.vue` - Cloud Accounts
**Layout:** `default.vue`  
**Status:** Phase 5 (Planned)  
**Features:** Placeholder only - "🚧 No data — Phase 5"

#### `/mcp/index.vue` - MCP Servers
**Layout:** `default.vue`  
**Status:** Phase 5 (Planned)  
**Features:** Placeholder only - "🚧 No data — Phase 5"

#### `/settings/index.vue` - Settings
**Layout:** `default.vue`  
**Status:** Phase 6 (Planned)  
**Features:** Placeholder only - "🚧 No data — Phase 6"

## 4. LAYOUT COMPONENTS

### 4.1 `default.vue` Layout
**Used by:** All authenticated pages  
**Middleware:** `auth` middleware applied  
**Structure:**
```
<div class="min-h-screen flex flex-col">
  <AppHeader />
  <div class="flex flex-1">
    <AppSidebar />
    <main class="flex-1 p-6 overflow-auto">
      <slot />
    </main>
  </div>
</div>
```

### 4.2 `auth.vue` Layout
**Used by:** Login page  
**Structure:**
```
<div class="min-h-screen flex items-center justify-center bg-background">
  <slot />
</div>
```

## 5. LAYOUT COMPONENTS (UI)

### 5.1 `AppHeader.vue` - Top Navigation Bar
**Sticky, z-50 positioning**  
**Contents (left to right):**
1. **Logo Section:** ⚡ icon + "OpenOps" brand name
2. **Spacer** (flex-1)
3. **User Info:** Shows username + role (hidden on mobile < md)
   - Role displayed as: "Super Admin" or "Tenant Admin"
4. **Theme Toggle Button** (ThemeToggle component)
5. **Language Switch Button** (LangSwitch component) 
6. **Logout Button** (if authenticated)

**Styling:** 
- Border-bottom, backdrop blur, translucent background
- Height: 56px (h-14)
- Responsive padding

**Translations used:**
```
app.name, user.superAdmin, user.tenantAdmin, theme.light, theme.dark, auth.logout
```

### 5.2 `AppSidebar.vue` - Left Navigation Sidebar
**Responsive:** Hidden on mobile (md: breakpoint and up)  
**Width:** 240px (w-60)  
**Features:**
- Navigation menu (flex column)
- Dynamic nav items based on user role
- Active route highlighting
- Smooth transitions and hover effects

**Navigation Items (NavItem structure):**
```typescript
interface NavItem {
  label: string           // i18n translated label
  to: string             // Route path
  icon: string           // Emoji icon
  superAdminOnly?: boolean  // Role access control
}
```

**Navigation Menu:**
| Icon | Label | Route | Super Admin Only |
|------|-------|-------|-----------------|
| 📊 | Dashboard | / | No |
| 💬 | Chat | /chat | No |
| 🏢 | Tenants | /tenants | Yes |
| 👥 | Users | /users | Yes |
| 🛠️ | Skills | /skills | No |
| 📚 | Knowledge | /knowledge | No |
| 🤖 | LLM Providers | /providers | No |
| ☁️ | Cloud Accounts | /accounts | No |
| 🔌 | MCP Servers | /mcp | No |
| ⚙️ | Settings | /settings | No |

**Styling:**
- Active route: Highlighted with accent background + bold text
- Hover: Accent background color
- Text: Small (sm) with muted foreground, 16px emoji icons

### 5.3 `ThemeToggle.vue` - Dark/Light Theme Switcher
**Type:** Icon button  
**Icons:**
- Sun icon (shown when in dark mode - toggles to light)
- Moon icon (shown when in light mode - toggles to dark)
**Size:** 18x18px, centered in 36px button
**Title:** Dynamically set to "Light" or "Dark" i18n keys

### 5.4 `LangSwitch.vue` - Language Switcher
**Type:** Text button  
**Display:** 
- Shows "中" when locale is English → clicking sets to Chinese
- Shows "EN" when locale is Chinese → clicking sets to English
**Locales:** 'zh' (Chinese) and 'en' (English)
**Default:** Chinese (zh)

## 6. STATE MANAGEMENT (Pinia Store)

### 6.1 `stores/auth.ts` - Authentication Store

**State:**
```typescript
{
  user: User | null                    // Current logged-in user
  isAuthenticated: boolean             // Auth status flag
  isLoading: boolean                   // Loading state during auth checks
}
```

**User Interface:**
```typescript
interface User {
  id: string
  username: string
  role: 'super_admin' | 'tenant_admin'
  tenant_id: string | null              // Null for super_admin
  email: string | null
}
```

**Getters:**
- `isSuperAdmin`: Returns true if user.role === 'super_admin'
- `tenantId`: Returns user's tenant_id

**Actions:**
- `fetchMe()`: Fetch current user from `/api/auth/me`
- `login(username, password)`: POST to `/api/auth/login` with credentials
- `logout()`: POST to `/api/auth/logout`

## 7. COMPOSABLES & UTILITIES

### 7.1 `composables/useApi.ts` - HTTP Client

**Purpose:** Centralized API client for all backend calls  
**Base URL:** Uses `config.public.apiBase` from Nuxt runtime config  
**Features:**
- Automatic JSON serialization/deserialization
- HttpOnly cookie support (credentials: 'include')
- Default 'Content-Type: application/json' header
- Error handling with custom ApiError class

**Available Methods:**
```typescript
useApi().get<T>(url: string): Promise<T>
useApi().post<T>(url: string, body?: unknown): Promise<T>
useApi().put<T>(url: string, body?: unknown): Promise<T>
useApi().del<T>(url: string): Promise<T>
```

**Error Handling:**
- Throws `ApiError` on non-200 responses
- ApiError has `status` property and message

**Configuration:**
- API proxy in dev: `/api/**` → `http://localhost:3080/api/**`
- Production: Uses `NUXT_PUBLIC_API_BASE` environment variable

## 8. MIDDLEWARE

### 8.1 `middleware/auth.ts` - Authentication Guard
**Applied to:** All pages in `default.vue` layout  
**Behavior:**
1. Skips auth check for `/login` page
2. Calls `authStore.fetchMe()` if user not loaded
3. Redirects to `/login` if not authenticated
4. Allows access to protected pages if authenticated

## 9. INTERNATIONALIZATION (i18n)

**Supported Languages:**
- **English** (en) - `i18n/en.json`
- **Chinese Simplified** (zh) - `i18n/zh.json`

**Default Language:** Chinese (zh)  
**Strategy:** no_prefix (locale not in URL path)  
**Storage:** Cookie `i18n_locale` for persistence  
**Detection:** Browser language detection with fallback to Chinese

### 9.1 i18n Keys Structure

**app:**
- `name`: App display name
- `description`: App description

**nav:** (All sidebar navigation labels)
- `dashboard`, `chat`, `tenants`, `users`, `skills`, `knowledge`, `providers`, `accounts`, `mcp`, `settings`

**auth:** (Authentication related)
- `login`, `logout`, `username`, `password`, `loginTitle`, `loginDescription`, `loginButton`, `loginError`
- `changePassword`, `currentPassword`, `newPassword`, `confirmPassword`

**dashboard:** (Dashboard page)
- `title`, `welcome`, `activeSessions`, `totalTenants`, `totalUsers`, `totalSkills`

**tenant:** (Tenant management)
- `title`, `create`, `name`, `slug`, `awsAccounts`, `actions`, `edit`, `delete`, `confirmDelete`

**user:** (User management)
- `title`, `create`, `username`, `email`, `role`, `tenant`, `status`, `active`, `inactive`, `superAdmin`, `tenantAdmin`

**chat:** (Chat/AI assistant)
- `title`, `placeholder`, `send`, `thinking`, `usingTool`, `newChat`, `history`

**common:** (Generic/reusable)
- `save`, `cancel`, `confirm`, `search`, `loading`, `noData`, `success`, `error`, `enabled`, `disabled`

**theme:** (Theme mode)
- `light`, `dark`, `system`

## 10. STYLING SYSTEM

### 10.1 Tailwind CSS Configuration
**Dark Mode:** Class-based (`class`)  
**Color Scheme:** HSL-based CSS variables (for themability)  
**Plugins:** `tailwindcss-animate`

### 10.2 Color Variables (Light/Dark Modes)

**Light Mode (`:root`):**
```css
--background: 0 0% 100%;              /* White */
--foreground: 222.2 84% 4.9%;         /* Dark blue-black */
--primary: 222.2 47.4% 11.2%;         /* Dark blue */
--secondary: 210 40% 96.1%;           /* Light gray-blue */
--accent: 210 40% 96.1%;              /* Light gray-blue */
--destructive: 0 84.2% 60.2%;         /* Red */
--muted: 210 40% 96.1%;               /* Light gray-blue */
```

**Dark Mode (`.dark`):**
```css
--background: 222.2 84% 4.9%;         /* Dark blue-black */
--foreground: 210 40% 98%;            /* Almost white */
--primary: 210 40% 98%;               /* Almost white */
--secondary: 217.2 32.6% 17.5%;       /* Dark gray-blue */
--accent: 217.2 32.6% 17.5%;          /* Dark gray-blue */
--destructive: 0 62.8% 30.6%;         /* Dark red */
--muted: 217.2 32.6% 17.5%;           /* Dark gray-blue */
```

### 10.3 Tailwind Utilities Extended
- Container: max-width 1400px, centered, 2rem padding
- Border radius: `lg` (0.5rem), `md` (0.375rem), `sm` (0.125rem)
- All semantic color utilities: `border`, `input`, `ring`, `card`, `popover`, etc.

### 10.4 UI Component Patterns
**Pattern:** shadcn-vue style (no direct component library used)  
**Custom components:** Buttons, inputs, cards built with:
- Tailwind CSS classes
- Class variance authority (CVA) for variants
- clsx for conditional classes
- tailwind-merge for utility merging

**Common Button Pattern:**
```vue
<button
  class="inline-flex items-center justify-center rounded-md text-sm font-medium 
         ring-offset-background transition-colors bg-primary text-primary-foreground 
         hover:bg-primary/90 h-9 px-4"
>
  Label
</button>
```

**Common Card Pattern:**
```vue
<div class="rounded-lg border bg-card text-card-foreground shadow-sm p-6">
  <!-- Content -->
</div>
```

**Common Input Pattern:**
```vue
<input
  class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm 
         ring-offset-background placeholder:text-muted-foreground 
         focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
/>
```

## 11. CONFIGURATION FILES

### 11.1 `nuxt.config.ts` - Main Nuxt Configuration

**Key Modules:**
- `@nuxtjs/tailwindcss`: Tailwind CSS integration
- `@nuxtjs/color-mode`: Dark/light theme support
- `@nuxtjs/i18n`: Internationalization
- `@pinia/nuxt`: State management

**Color Mode:**
- Class suffix: '' (no suffix, uses `.dark` class)
- Preference: 'system' (follows OS)
- Fallback: 'light'

**API Proxy Rules (Development):**
- `/api/**` → `http://localhost:3080/api/**` (Rust backend)
- `/health` → `http://localhost:3080/health`

**TypeScript:** Strict mode enabled

**Devtools:** Enabled

**Telemetry:** Disabled

### 11.2 `package.json` - Dependencies

**Production:**
- `nuxt@3.16.2`: Latest Nuxt 3
- `vue@3.5.13`: Vue 3
- `vue-router@4.5.0`: Router (auto-managed by Nuxt)
- `pinia@2.3.1`: State management
- `@pinia/nuxt@0.9.0`: Pinia Nuxt integration
- `@nuxtjs/i18n@9.5.3`: i18n module
- `@nuxtjs/color-mode@3.5.2`: Theme switching
- `lucide-vue-next@0.468.0`: Icon library
- `radix-vue@1.9.12`: UI primitives
- `class-variance-authority@0.7.1`: Component variants
- `clsx@2.1.1`: Utility merging
- `tailwind-merge@3.0.1`: Tailwind class merging

**Dev:**
- `@nuxtjs/tailwindcss@6.13.2`: Tailwind CSS module
- `tailwindcss-animate@1.0.7`: Animation utilities
- `typescript@5.7.0`: TypeScript

**Scripts:**
- `dev`: Nuxt dev server
- `build`: Production build
- `generate`: Static site generation
- `preview`: Preview prod build
- `lint`: ESLint check

## 12. DEPLOYMENT

### Dockerfile Strategy (Multi-stage)
**Stage 1 - Builder:**
- Base: `node:20-slim`
- Installs deps: `npm ci`
- Runs build: `npm run build`

**Stage 2 - Runtime:**
- Base: `node:20-slim`
- Copies built `.output` directory
- Runs: `node .output/server/index.mjs`
- Port: 3000
- Host: 0.0.0.0

## 13. FEATURE COMPLETION STATUS

| Feature | Status | Phase | Notes |
|---------|--------|-------|-------|
| Authentication | ✅ Implemented | - | Login/logout, role-based access |
| Dashboard | ✅ Basic | Phase 2 | Placeholder stats, ready for API |
| Chat/AI | 🚧 In Progress | Phase 3 | Claude integration coming |
| Layout System | ✅ Complete | - | Header, sidebar, responsive |
| Dark/Light Theme | ✅ Complete | - | Auto dark mode, system detection |
| i18n (EN/ZH) | ✅ Complete | - | All pages translated |
| Tenant Mgmt | 🔨 Scaffolded | Phase 4 | Super admin only |
| User Mgmt | 🔨 Scaffolded | Phase 4 | Super admin only |
| Skills | 🔨 Planned | Phase 5 | Placeholder |
| Knowledge Base | 🔨 Planned | Phase 5 | Placeholder |
| LLM Providers | 🔨 Planned | Phase 5 | Placeholder |
| Cloud Accounts | 🔨 Planned | Phase 5 | Placeholder |
| MCP Servers | 🔨 Planned | Phase 5 | Placeholder |
| Settings | 🔨 Planned | Phase 6 | Placeholder |

## 14. KEY DEVELOPMENT INSIGHTS

### Strengths:
1. **Clean Architecture:** Clear separation of concerns (pages, components, stores, composables)
2. **Type-Safe:** Full TypeScript support with strict mode
3. **i18n Ready:** Bilingual (English/Chinese) with cookie persistence
4. **Responsive:** Mobile-first design with Tailwind breakpoints
5. **Themeable:** Dark/light mode with HSL color variables
6. **API-Agnostic:** useApi composable abstracts backend integration
7. **Role-Based Access:** Built-in super_admin vs tenant_admin filtering
8. **SSR-Ready:** Nuxt SSR for better performance and SEO

### Development Patterns:
- **Composition API:** All components use `<script setup>` syntax
- **Auto-imports:** Components and composables auto-imported by Nuxt
- **Reactive Forms:** v-model binding for two-way form data
- **Dynamic i18n:** All UI labels use `$t()` or `useI18n()`
- **Error Handling:** Try-catch in async actions with user feedback

### No Component Libraries:
- No shadcn-vue or pre-built component imports
- UI built with raw Tailwind classes
- Pattern-based consistency via class templates
- Gives full control but requires careful maintenance

## 15. NEXT DEVELOPMENT STEPS

### Immediate (Phase 3):
1. Implement SSE streaming for chat page
2. Integrate Claude API backend
3. Add message history persistence
4. Implement tool use display

### Short-term (Phase 4):
1. Build tenant CRUD UI + API integration
2. Build user CRUD UI + API integration
3. Add form validation
4. Add confirmation dialogs

### Medium-term (Phase 5):
1. Implement Skills management
2. Implement Knowledge base UI
3. Implement LLM Providers configuration
4. Implement Cloud Accounts management
5. Implement MCP Server configuration

### Long-term (Phase 6):
1. Implement Settings page (password change, preferences)
2. Add audit logging
3. Add activity feeds
4. Performance optimization
5. Mobile app version

