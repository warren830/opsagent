# OpenOps Frontend - UI Layout & Component Hierarchy

## 📐 Overall Application Layout

### Default Layout (Most Pages)
```
┌─────────────────────────────────────────────────────────────┐
│ AppHeader                                    [⚙️ OpenOps logo]
│  [⚡ OpenOps] [Spacer] [👤 username] [🌓] [🌐] [Logout]    
├─────────────────────────────────┬───────────────────────────┤
│                                 │                           │
│       AppSidebar                │                           │
│   ┌─────────────────────┐       │    Main Content Area      │
│   │ 📊 Dashboard   ← Active│       │    (Page <slot />)        │
│   │ 💬 Chat         │       │                           │
│   │ 🏢 Tenants*     │       │    grid, flexbox,      │
│   │ 👥 Users*       │       │    scrollable           │
│   │ 🛠️ Skills         │       │                           │
│   │ 📚 Knowledge    │       │                           │
│   │ 🤖 Providers    │       │                           │
│   │ ☁️ Accounts      │       │                           │
│   │ 🔌 MCP          │       │                           │
│   │ ⚙️ Settings      │       │                           │
│   └─────────────────────┘       │                           │
│   w-60 (240px)                  │ flex-1, p-6             │
│   hidden md:flex                │                           │
│                                 │                           │
└─────────────────────────────────┴───────────────────────────┘

* Super Admin only (filtered by authStore.isSuperAdmin)
```

### Auth Layout (Login Page)
```
┌─────────────────────────────────────────────────────────────┐
│                    min-h-screen centered                      │
│                                                               │
│         ┌─────────────────────────────────────┐              │
│         │   Sign in to OpenOps               │              │
│         │   Enter your credentials            │              │
│         │                                     │              │
│         │ [Username Input]                  │              │
│         │ [Password Input]                  │              │
│         │ [Error Message]                   │              │
│         │                                     │              │
│         │ [Sign in Button]                  │              │
│         │                                     │              │
│         │ [🌓] [🌐]                          │              │
│         └─────────────────────────────────────┘              │
│                      (max-w-sm)                              │
└─────────────────────────────────────────────────────────────┘
```

## 📊 Page Layouts (Content Area)

### Dashboard Page (/)
```
┌─────────────────────────────────────────────────────────────┐
│ Dashboard                                                    │
│ Welcome to OpenOps, username                                │
│                                                              │
│ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌──────────┐
│ │ Active      │ │ Total       │ │ Total Users │ │ Total    │
│ │ Sessions    │ │ Tenants     │ │             │ │ Skills   │
│ │             │ │             │ │             │ │          │
│ │     0       │ │      0      │ │      0      │ │    0     │
│ └─────────────┘ └─────────────┘ └─────────────┘ └──────────┘
│ 
│ (md:grid-cols-2 lg:grid-cols-4)
└─────────────────────────────────────────────────────────────┘
```

### Chat Page (/chat)
```
┌─────────────────────────────────────────────────────────────┐
│ AI Chat                           [New Chat]                │
│                                                              │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ ╭─ Messages Area (flex-1, overflow-auto)         ──╮  ││
│ │ │                                                      │  ││
│ │ │         [User Message Right-Aligned]              │  ││
│ │ │                    ↑                              │  ││
│ │ │         [Assistant Message Left-Aligned]         │  ││
│ │ │                                                      │  ││
│ │ │  🚧 Claude integration coming in Phase 3.         │  ││
│ │ │     Backend is ready!                            │  ││
│ │ │                    ↓                              │  ││
│ │ │              [Thinking...]  (if loading)         │  ││
│ │ │                                                      │  ││
│ │ ╰──────────────────────────────────────────────────────╯  ││
│ │                                                            ││
│ │ ┌─────────────────────────────────────────────────────┐  ││
│ │ │ [Input: Ask a question...]      [Send Button]      │  ││
│ │ └─────────────────────────────────────────────────────┘  ││
│ └─────────────────────────────────────────────────────────┐│
└─────────────────────────────────────────────────────────────┘
```

### Management Pages (/tenants, /users)
```
┌─────────────────────────────────────────────────────────────┐
│ Tenant Management                [Create Tenant]           │
│                                                              │
│ ┌─────────────────────────────────────────────────────────┐│
│ │                                                           ││
│ │               No data                                    ││
│ │                                                           ││
│ └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

### Feature Placeholder Pages (/skills, /knowledge, /providers, /accounts, /mcp)
```
┌─────────────────────────────────────────────────────────────┐
│ Skills                                                       │
│                                                              │
│ ┌─────────────────────────────────────────────────────────┐│
│ │                                                           ││
│ │        🚧 No data — Phase 5                             ││
│ │                                                           ││
│ └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

## 🧩 Component Structure

### AppHeader Component
```
<header>  [sticky z-50 border-b backdrop-blur]
  <div class="flex items-center px-4 gap-4 h-14">
    
    ┌─ Logo Section ──────────────────────────┐
    │ <span>⚡</span> <span>OpenOps</span>     │
    │ (text-lg font-semibold)                 │
    └──────────────────────────────────────────┘
    
    ┌─ Flex Spacer ────────────────────────────┐
    │ <div class="flex-1" />                   │
    └──────────────────────────────────────────┘
    
    ┌─ User Info (hidden md:inline) ───────────┐
    │ "username (Super Admin | Tenant Admin)"  │
    │ (text-sm text-muted-foreground)          │
    └──────────────────────────────────────────┘
    
    ┌─ Theme Toggle ───────────────────────────┐
    │ [🌙] or [☀️] (h-9 w-9)                   │
    └──────────────────────────────────────────┘
    
    ┌─ Language Switch ────────────────────────┐
    │ [中] or [EN] (h-9 px-3)                  │
    └──────────────────────────────────────────┘
    
    ┌─ Logout Button ──────────────────────────┐
    │ [Logout] (h-9 px-3)                      │
    └──────────────────────────────────────────┘
  </div>
</header>
```

### AppSidebar Component
```
<aside>  [hidden md:flex w-60 border-r bg-background]
  <nav class="flex-1 space-y-1 p-3">
    
    ┌─ Navigation Item (Active) ──────────┐
    │ ┌──────────────────────────────────┐│
    │ │ 📊 Dashboard                      ││  [bg-accent text-accent-foreground]
    │ │ (text-sm rounded-lg px-3 py-2)   ││  [font-medium] ← Active
    │ └──────────────────────────────────┘│
    └──────────────────────────────────────┘
    
    ┌─ Navigation Item (Inactive/Hover) ──┐
    │ ┌──────────────────────────────────┐│
    │ │ 💬 Chat                           ││  [text-muted-foreground]
    │ │ (text-sm rounded-lg px-3 py-2)   ││  [hover:bg-accent hover:text-accent-foreground]
    │ └──────────────────────────────────┘│
    └──────────────────────────────────────┘
    
    ┌─ Navigation Item (Super Admin Only) ┐
    │ ┌──────────────────────────────────┐│
    │ │ 🏢 Tenants                        ││  [Filtered by isSuperAdmin]
    │ │ (if authStore.isSuperAdmin)      ││
    │ └──────────────────────────────────┘│
    └──────────────────────────────────────┘
    
    ... (more items follow pattern)
    
  </nav>
</aside>
```

### Login Form Component
```
<div class="w-full max-w-sm space-y-6">
  
  ┌─ Header ──────────────────────────┐
  │ <h1>Sign in to OpenOps</h1>       │
  │ (text-2xl font-bold)              │
  │ <p>Enter your credentials</p>     │
  │ (text-sm text-muted-foreground)   │
  └───────────────────────────────────┘
  
  ┌─ Form ────────────────────────────┐
  │ <form @submit.prevent>            │
  │                                    │
  │  ┌─ Username Field ──────────────┐│
  │  │ <label>Username</label>        ││
  │  │ <input type="text" />          ││
  │  │ (h-10 px-3 rounded border)     ││
  │  └────────────────────────────────┘│
  │                                    │
  │  ┌─ Password Field ──────────────┐│
  │  │ <label>Password</label>        ││
  │  │ <input type="password" />      ││
  │  │ (h-10 px-3 rounded border)     ││
  │  └────────────────────────────────┘│
  │                                    │
  │  ┌─ Error Message (if error) ───┐│
  │  │ "Invalid username or password" ││
  │  │ (text-sm text-destructive)     ││
  │  └────────────────────────────────┘│
  │                                    │
  │  [Sign in] (h-10 w-full bg-primary)│
  │  (disabled if loading)             │
  │                                    │
  │ </form>                            │
  └───────────────────────────────────┘
  
  ┌─ Bottom Controls ─────────────────┐
  │ [🌙/☀️] [中/EN]                   │
  │ (flex justify-center gap-2)       │
  └───────────────────────────────────┘
  
</div>
```

## 🎨 Tailwind Color Palette in Use

### Light Mode
```
┌─────────────────────────────────────┐
│ Background: White                   │  --background: 0 0% 100%
│ Foreground: Dark Blue-Black         │  --foreground: 222.2 84% 4.9%
│ Primary: Dark Blue                  │  --primary: 222.2 47.4% 11.2%
│ Secondary: Light Gray-Blue          │  --secondary: 210 40% 96.1%
│ Accent: Light Gray-Blue             │  --accent: 210 40% 96.1%
│ Muted: Light Gray-Blue              │  --muted: 210 40% 96.1%
│ Border: Very Light Gray             │  --border: 214.3 31.8% 91.4%
│ Destructive (Error): Red            │  --destructive: 0 84.2% 60.2%
└─────────────────────────────────────┘
```

### Dark Mode (.dark)
```
┌─────────────────────────────────────┐
│ Background: Dark Blue-Black         │  --background: 222.2 84% 4.9%
│ Foreground: Almost White            │  --foreground: 210 40% 98%
│ Primary: Almost White               │  --primary: 210 40% 98%
│ Secondary: Dark Gray-Blue           │  --secondary: 217.2 32.6% 17.5%
│ Accent: Dark Gray-Blue              │  --accent: 217.2 32.6% 17.5%
│ Muted: Dark Gray-Blue               │  --muted: 217.2 32.6% 17.5%
│ Border: Dark Gray-Blue              │  --border: 217.2 32.6% 17.5%
│ Destructive (Error): Dark Red       │  --destructive: 0 62.8% 30.6%
└─────────────────────────────────────┘
```

## 📱 Responsive Behavior

### Mobile (< md breakpoint - 768px)
```
┌──────────────────┐
│ AppHeader        │  ← Logo + Theme + Lang + Logout (no user info)
├──────────────────┤
│                  │
│  Main Content    │  ← Full width, p-6
│  (No Sidebar)    │
│                  │
│                  │
└──────────────────┘

AppSidebar is HIDDEN (hidden md:flex)
```

### Desktop (md+ breakpoint - 768px+)
```
┌────────────────────────────────────────────────┐
│ AppHeader (user info visible)                  │
├────────────┬──────────────────────────────────┤
│            │                                  │
│ AppSidebar │ Main Content                     │
│ (w-60)     │ (flex-1)                         │
│            │                                  │
│            │                                  │
│            │                                  │
└────────────┴──────────────────────────────────┘
```

## 🔄 Data Flow Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    User Browser                              │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ Vue Component (e.g., pages/tenants/index.vue)      │   │
│  │  - Uses useAuthStore() for permissions             │   │
│  │  - Uses useApi() for HTTP calls                    │   │
│  │  - Uses useI18n() for labels                       │   │
│  │  - Uses useRouter(), useRoute() for navigation     │   │
│  └─────────────────────────────────────────────────────┘   │
│           ↓ useApi()            ↓ useAuthStore           │
│           ↓                      ↓                          │
│  ┌─────────────────────────┐  ┌──────────────────────┐   │
│  │ Composable: useApi()    │  │ Store: Pinia/auth    │   │
│  │ - fetch() wrapper       │  │ - user state         │   │
│  │ - JSON serialization    │  │ - isAuthenticated    │   │
│  │ - Cookie credentials    │  │ - login/logout/me    │   │
│  │ - Error handling        │  │ - isSuperAdmin       │   │
│  └─────────────────────────┘  └──────────────────────┘   │
│           ↓                              ↓                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ HTTP Client (with credentials: include)             │  │
│  │ Base URL: http://localhost:3080/api (dev)          │  │
│  └──────────────────────────────────────────────────────┘  │
│           ↓                                                 │
└───────────┼─────────────────────────────────────────────────┘
            ↓
    ┌───────────────────┐
    │ Rust Backend      │
    │ (localhost:3080)  │
    │                   │
    │ /api/auth/login   │
    │ /api/auth/me      │
    │ /api/tenants      │
    │ /api/users        │
    │ ... etc           │
    └───────────────────┘
```

## 🔐 Authentication State Flow

```
┌─ Start ─────────────────────────────────────────────────────┐
│ authStore: { user: null, isAuthenticated: false }           │
└────────────────────────────────────────────────────────────┬┘
                                                              │
                          User navigates to /login
                                ↓
┌─ Auth Middleware ───────────────────────────────────────────┤
│ if (to.path === '/login') return  // Skip check              │
│ if (!authStore.user && authStore.isLoading)                 │
│   await authStore.fetchMe()  // GET /api/auth/me            │
│ if (!authStore.isAuthenticated) redirectTo('/login')        │
└────────────────────────────────────────────────────────────┬┘
                                                              │
                    User submits login form
                                ↓
┌─ handleLogin() ─────────────────────────────────────────────┤
│ authStore.login(username, password)                         │
│   POST /api/auth/login { username, password }              │
│   Response: { user, token }                                 │
│   authStore.user = user                                     │
│   authStore.isAuthenticated = true                          │
│   Cookie: saved (credentials: include)                      │
└────────────────────────────────────────────────────────────┬┘
                                                              │
                  router.push('/')  → Dashboard
                                ↓
┌─ Dashboard Page ────────────────────────────────────────────┤
│ const authStore = useAuthStore()                            │
│ Shows: user.username, user.role                             │
│ Shows: 4 stat cards (from dashboard data)                   │
└────────────────────────────────────────────────────────────┬┘
                                                              │
            User clicks Logout button (in AppHeader)
                                ↓
┌─ handleLogout() ────────────────────────────────────────────┤
│ authStore.logout()                                          │
│   POST /api/auth/logout                                     │
│   authStore.user = null                                     │
│   authStore.isAuthenticated = false                         │
│   Cookie: cleared                                           │
│ navigateTo('/login')                                        │
└────────────────────────────────────────────────────────────┬┘
                                                              │
                    Back to Login page
                                ↓
```

---

**Generated:** 2026-04-05 | **Framework:** Nuxt 3 + Vue 3 | **Styling:** Tailwind CSS
