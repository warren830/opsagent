# Ops 前端重设计：Holographic Glass

**Date**: 2026-04-27
**Status**: Draft (awaiting review)
**Scope**: `frontend/` — 完整视觉重构，从 Grafana 风深色主题迁移到 VisionOS 风全息玻璃浅色主题。

---

## 1. 背景与目标

当前 `frontend/` 以 Grafana 风深色主题为主（`#111217` 背景 + `#FF6600` 橙色强调），配色与组件样式已在项目中使用大半年。希望将整体视觉重做为"白底 + 科幻"风格，显著提升产品的现代感与 demo 冲击力，同时保持运维工具必需的信息密度。

**目标：**
- 视觉风格：Apple VisionOS 式的全息玻璃（holographic glass）
- 背景：纯白 + 多点彩色光晕
- 强调色：天蓝（`#3b82f6`）+ 薰衣草紫（`#8b5cf6`）
- 保留所有现有功能，只做视觉与交互层的重构
- 通过分层推进策略，每一层独立 PR，可验证可回滚

**非目标：**
- 不增删任何功能
- 不改后端 API 或数据结构
- 不改 i18n 文案
- 不做响应式移动端适配（Ops 是桌面端运维工具）

---

## 2. 设计决策（已与用户达成一致）

| # | 决策点 | 选择 | 理由 |
|---|--------|------|------|
| 1 | 整体美学方向 | **C · Holographic Glass** (VisionOS 风) | 用户选择 |
| 2 | 密度策略 | **Hybrid B · 单一玻璃面板** + 浮动玻璃卡片混用 | 列表类用单面板保信息密度，仪表盘用浮动卡片 |
| 3 | 色彩基调 | **Sky & Lavender** (天蓝+薰衣草紫+薄荷) | 中性专业，紫色天然适合 AI 色 |
| 4 | 暗色模式 | **移除，仅保留浅色单模式** | 简化 CSS 维护成本 |
| 5 | 推进节奏 | **分层 B：Token → UI 组件 → Layout → 核心页 → 其余页** | 每层可回滚，中间状态可接受 |
| 6 | 应用外壳 | **Floating Glass Islands**（所有区块都是独立浮动玻璃面板） | 最 wow 的 VisionOS 味道 |
| 7 | 字体 | **Geist (UI) + Geist Mono (代码/数据)** | Vercel 新字体，未来感强 |
| 8 | 动效强度 | **③ 明显活泼**（光标跟随光晕、数字滚动、卡片起伏） | 强化"全息"印象 |

---

## 3. 设计 Token

所有 token 集中在 `frontend/assets/css/tailwind.css` 的 `:root` 块里。移除整个 `.dark` 块及相关逻辑。

### 3.1 颜色

```css
:root {
  /* Base */
  --background: 0 0% 100%;              /* 纯白 */
  --foreground: 222 47% 11%;            /* slate-900 #0f172a */
  --muted: 220 14% 96%;                 /* slate-100 */
  --muted-foreground: 215 20% 45%;      /* slate-500 */
  --border: 220 13% 91%;                /* slate-200 */
  --input: 220 13% 91%;

  /* Accent */
  --primary: 217 91% 60%;               /* sky-500 #3b82f6 */
  --primary-foreground: 0 0% 100%;
  --secondary: 258 90% 66%;             /* violet-500 #8b5cf6 — 预留 AI/Agent 色 */
  --secondary-foreground: 0 0% 100%;
  --accent: 217 91% 97%;                /* sky-50 — hover 底 */
  --accent-foreground: 217 91% 40%;
  --ring: 217 91% 60%;

  /* Semantic */
  --success: 160 84% 39%;               /* emerald-500 */
  --warning: 38 92% 50%;                /* amber-500 */
  --destructive: 0 84% 60%;             /* red-500 */
  --destructive-foreground: 0 0% 100%;
  --info: 199 89% 48%;                  /* sky-600 */

  /* Glass surfaces */
  --glass-bg: 255 255 255 / 0.55;       /* rgba() 友好 */
  --glass-border: 255 255 255 / 0.85;
  --glass-shadow: 0 8px 32px rgba(100, 140, 200, 0.12);
  --glass-shadow-hover: 0 12px 40px rgba(100, 140, 200, 0.18);
  --glass-blur: 22px;
  --glass-blur-subtle: 12px;

  /* Aurora background stops (用于 AuroraBackground 组件) */
  --aurora-sky: rgba(147, 197, 253, 0.45);
  --aurora-lavender: rgba(196, 181, 253, 0.40);
  --aurora-mint: rgba(165, 243, 252, 0.35);

  /* Radius scale */
  --radius: 0.5rem;                     /* 8px 基准（button/input） */
  --radius-sm: 0.25rem;                 /* 4px (pill/chip) */
  --radius-md: 0.625rem;                /* 10px (小卡片/行) */
  --radius-lg: 0.875rem;                /* 14px (大面板/岛) */
  --radius-xl: 1.25rem;                 /* 20px (对话框、特殊容器) */
}
```

### 3.2 字体

在 `nuxt.config.ts` 中引入 Geist：
```ts
app: {
  head: {
    link: [
      { rel: 'preconnect', href: 'https://fonts.googleapis.com' },
      { rel: 'stylesheet', href: 'https://fonts.googleapis.com/css2?family=Geist:wght@300;400;500;600;700&family=Geist+Mono:wght@400;500&display=swap' },
    ],
  },
},
```

Tailwind `theme.extend.fontFamily`:
```ts
fontFamily: {
  sans: ['Geist', 'ui-sans-serif', 'system-ui', 'sans-serif'],
  mono: ['"Geist Mono"', 'ui-monospace', 'SFMono-Regular', 'monospace'],
}
```

### 3.3 间距 / 尺寸 Scale

保留 Tailwind 默认 spacing scale，但约定：
- **岛屿外间距**：`p-2.5` (10px) 围绕整个布局
- **岛屿间间隙**：`gap-2.5` (10px)
- **面板内 padding**：`p-3.5` (14px) 或 `p-4` (16px)
- **行内垂直 padding**：`py-2` (8px) 紧凑行；`py-2.5` (10px) 舒适行
- **按钮/输入高度**：`h-8` (32px) 默认，`h-9` (36px) 大号

### 3.4 动效 Token

放入 `tailwind.config.ts` 的 `theme.extend.keyframes` 与 `theme.extend.animation`，以及 CSS 变量用于 easing：

```css
/* tailwind.css :root */
--motion-ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1);
--motion-ease-smooth: cubic-bezier(0.16, 1, 0.3, 1);
```

```ts
// tailwind.config.ts
animation: {
  'aurora-drift': 'aurora-drift 45s ease-in-out infinite', // 背景光晕缓慢漂移
  'glass-hover': 'glass-hover 200ms var(--motion-ease-spring)', // 卡片上浮 2px
  'neon-sweep': 'neon-sweep 600ms ease',        // 行 hover 流过彩光
  'count-up': 'count-up 800ms ease-out',        // 数字滚动
  'fade-in-up': 'fade-in-up 350ms var(--motion-ease-spring)', // 列表项入场
  'typing-dot': 'typing-dot 1.4s ease-in-out infinite', // AI 打字指示
}
```
keyframes 的具体定义在实施计划阶段细化。

---

## 4. 应用外壳（Floating Glass Islands）

重写 `layouts/default.vue`。新结构：

```
<body>
  └─ <AuroraBackground />              ← fixed，全屏铺底，渐变光晕层
  └─ <CursorGlow />                    ← fixed，跟随鼠标的柔光点
  └─ <div class="layout-root">         ← padding 10px，铺满视口
       ├─ <AppSidebar />               ← 浮动玻璃岛 1，w-48 (192px)
       ├─ <div class="main-column">
       │    ├─ <AppHeader />           ← 浮动玻璃岛 2，h-14 (56px)
       │    └─ <main class="content">  ← 浮动玻璃岛 3，flex-1
       │         <slot />
       │    </main>
       └─ <ChatPanel />                ← 浮动玻璃岛 4，w-80 (320px)
     </div>
</body>
```

### 4.1 AppSidebar

- **玻璃材质**：`bg-white/55 border border-white/85 backdrop-blur-[22px] shadow-[var(--glass-shadow)] rounded-[14px]`
- **顶部 Logo 块**：`linear-gradient(135deg, #3b82f6, #8b5cf6)` 渐变方块 + "Ops" 文字
- **导航分组标签**：`text-[9px] uppercase tracking-[1.2px] text-slate-400 font-semibold`（如 OBSERVE / OPERATE / ADMIN）
- **导航项**：
  - 默认：`text-slate-600 hover:bg-white/60 hover:text-slate-900`
  - 激活：`bg-gradient-to-r from-sky-500/15 to-transparent border-l-2 border-sky-500 text-sky-600 font-medium`
- **折叠功能**：保留（支持 collapse 到 icon-only 模式，降级为 56px 图标栏）

### 4.2 AppHeader (Topbar)

- 高度 `h-14` (56px)，玻璃材质同上
- 左：面包屑（`Dashboard / Issues / #1234`）
- 中：全局搜索框（`⌘K`），半透明玻璃底 + 快捷键徽章
- 右：通知铃 + 用户头像（渐变圆形）+ 语言切换

### 4.3 ChatPanel

- 宽度 `w-80` (320px)，可切换全屏模式（现有逻辑保留）
- 玻璃材质同上
- AI 消息气泡：`bg-white/55 border-l-2 border-violet-500` + `backdrop-blur-[18px]`
- 用户消息气泡：`bg-sky-500/12 border border-sky-500/30`
- AI 打字指示：三个点跳动动画 `animate-typing-dot`

### 4.4 性能约束

浮动玻璃岛同时存在 4 个，`backdrop-filter: blur(22px)` 在集成显卡上有风险。要求：
- 每个岛的 `will-change: backdrop-filter`
- `contain: layout style`
- 在 `<main>` 内部禁用 `backdrop-filter`（避免嵌套 blur）
- 测试目标：Intel MacBook Pro 2019（集显）Chrome 满页滚动 ≥ 50 FPS

---

## 5. 页面类型模式（5 种）

### 5.1 List / Table 模式（Hybrid B）

**适用**：issues, accounts, clusters, users, tenants, channels, providers, skills, knowledge, scheduled-jobs, approvals, deployments, repo, mcp

**结构**：
```
┌──────────────────────────────────────┐
│ 页面标题（渐变文字）+ 操作按钮         │
├──────────────────────────────────────┤
│ 过滤/搜索栏（独立浮动玻璃条）          │
├──────────────────────────────────────┤
│ ╭──────────────────────────────────╮ │
│ │ Table Header                     │ │  ← 单一玻璃面板
│ ├──────────────────────────────────┤ │     内部是紧凑行
│ │ Row 1                            │ │     hover 时流过霓虹
│ │ Row 2                            │ │
│ │ ...                              │ │
│ ╰──────────────────────────────────╯ │
│ Pagination                           │
└──────────────────────────────────────┘
```

**交互**：
- Row hover：背景流过 `linear-gradient(90deg, transparent, rgba(147,197,253,0.15), transparent)`，600ms ease
- Row click：打开右侧详情抽屉 (drawer) 或跳详情页
- Selected row：左侧 2px 蓝色描边
- Empty state：显示柔和插画 + "No XXX yet" 文案

### 5.2 Dashboard 模式

**适用**：index (home), resources/dashboard

**结构**：grid 布局 + 浮动玻璃卡片
- Stat card：`bg-white/62 backdrop-blur-[18px] rounded-[10px]` + hover 上浮 2px
- 数字用 `CountUp` 组件带 800ms 滚动动画
- 卡片顶部 2px 色带表示类型（信息蓝、成功绿、警告橙等）

### 5.3 Detail 模式

**适用**：各 `/:id` 详情页（issue detail, cluster detail, account detail...）

**结构**：
- 顶部 hero：标题（渐变文字）+ 状态徽章 + 时间信息 + 操作按钮
- 第二层：关键指标玻璃卡行（2-4 个 stat card）
- 第三层：Tab 玻璃面板（Overview / Logs / Events / Settings 等）
- Tab 内容在同一个玻璃面板内切换，不再嵌套玻璃

### 5.4 Conversation 模式

**适用**：rca (Root Cause Analysis), ChatPanel

**结构**：
- 消息列表（时间从上到下）
- AI 气泡：紫色描边玻璃
- User 气泡：蓝色半透明底
- 工具调用：独立 collapsible 玻璃块展示（展开看 tool input/output）
- 关键词高亮：`bg-yellow-200/60 rounded px-1`
- Markdown 渲染保留，但代码块用 `font-geist-mono` + 浅灰玻璃底

### 5.5 Diagram 模式

**适用**：topology (Vue Flow)

**结构**：
- Canvas 背景：浅色 + 极轻的 `radial-gradient` 光晕（透明度降到 0.15 避免干扰节点）
- 节点：变成玻璃卡片（保留 Vue Flow 的交互）
  - 节点圆角 `10px`，玻璃底，蓝色细边
  - 选中节点：加强 shadow + 蓝色 2px 边
- 连接线：`stroke: #3b82f6` + `stroke-opacity: 0.4` + 动画流光（已有 SVG `animate` 可用）
- MiniMap：保留，样式同玻璃

---

## 6. 组件库改造（`components/ui/`）

### 6.1 保留并重做样式层

所有现有 shadcn-vue 组件结构不变，仅改 `class-variance-authority` 的 variant 定义：

| 组件 | 关键改动 |
|------|---------|
| `button` | 主按钮渐变背景 `from-sky-500 to-violet-500` + glow shadow；次按钮玻璃材质 |
| `card` | 毛玻璃底；hover translateY(-2px) + 阴影加深 |
| `input` / `select` / `textarea` | `bg-white/60 border-white/85 backdrop-blur-[12px]`；focus 时 `ring-sky-500/50` |
| `dialog` / `popover` | 毛玻璃 + 大圆角（20px） + 背景叠加光晕 |
| `badge` | 柔和色系 + `rounded-full` + 细描边 |
| `checkbox` / `switch` | 开启态渐变色；关闭态玻璃底 |
| `separator` | 改用 `bg-gradient-to-r from-transparent via-slate-200 to-transparent` |
| `sonner` (toast) | 玻璃卡片 + slide-in spring 动画 |
| `skeleton` | 改用柔和 shimmer `from-slate-100 via-white to-slate-100` |
| `tooltip` | 深色反转玻璃（`bg-slate-900/85`）保可读 |

### 6.2 新增组件

- **`GlassPanel`** (`components/ui/glass-panel/`)：封装 Hybrid B 面板样式，支持 `subtle` / `default` / `strong` 三档模糊
- **`CountUp`** (`components/ui/count-up/`)：数字滚动动画，支持整数/浮点/千分位
- **`CursorGlow`** (`components/layout/CursorGlow.vue`)：layout 级鼠标跟随光晕
- **`AuroraBackground`** (`components/layout/AuroraBackground.vue`)：固定全屏背景光晕层，内部 3-4 个缓慢漂移的径向渐变

---

## 7. 分层推进计划

每个 Phase 独立 feature branch + 独立 PR，可单独 merge/revert。

### Phase ① Token 层（~1 天）

**文件**：
- `frontend/assets/css/tailwind.css` — 重写 `:root`，删除 `.dark` 块、删除 `.dark select option` 规则
- `frontend/tailwind.config.ts` — 删除 `darkMode: 'class'`、增加 Geist 字体、动效 keyframes、glass utilities
- `frontend/nuxt.config.ts` — 删除 `@nuxtjs/color-mode` 模块（`modules` 数组）和 `colorMode: { ... }` 配置块；引入 Geist 字体 `head.link`
- `frontend/package.json` — 从 dependencies 删除 `@nuxtjs/color-mode`
- `frontend/components/layout/ThemeToggle.vue` — 整个文件删除
- 引用 `LayoutThemeToggle` 的位置（搜索 `ThemeToggle` 全局）— 删除 `<LayoutThemeToggle />` 调用

**验证**：`npm run dev` 启动，全站变白底 + Geist 字体；旧组件样式可能错乱但能启动；无 console 报错。

### Phase ② 基础 UI 组件（~2 天）

**文件**：`frontend/components/ui/` 下所有子目录

**新增组件**：
- `components/ui/glass-panel/GlassPanel.vue` + `index.ts`
- `components/ui/count-up/CountUp.vue` + `index.ts`

**验证**：启动一个 Storybook-like 内部 demo 页（临时 `pages/_style-demo.vue`），枚举所有组件 variant 截图对比。

### Phase ③ Layout 层（~1 天）

**文件**：
- `frontend/layouts/default.vue` — 重写为 Floating Glass Islands 结构
- `frontend/components/layout/AppHeader.vue` — 重写
- `frontend/components/layout/AppSidebar.vue` — 重写（保留折叠逻辑）
- `frontend/components/layout/ChatPanel.vue` — 重写
- `frontend/components/layout/AuroraBackground.vue` — 新增
- `frontend/components/layout/CursorGlow.vue` — 新增

**验证**：登录后浏览各页，外壳已是新风格但 content 区域仍显示旧样式页面。截图对比。性能：Chrome DevTools Performance 录制，检查帧率。

### Phase ④ 核心页（~3 天）

**目标页**（5 个 hero 页）：
1. `pages/index.vue` — Dashboard 模式
2. `pages/issues/index.vue` + `pages/issues/[id].vue` — List + Detail 模式
3. `pages/rca/*` — Conversation 模式
4. `pages/topology/*` — Diagram 模式
5. `pages/clusters/index.vue` + `pages/clusters/[id].vue` — List + Detail 模式

**验证**：这 5 个页面是 demo 场景，重点做视觉回归截图 + 手动流程测试。

### Phase ⑤ 其余页面（~3 天）

按 List/Detail 模式批量改造：
- `pages/accounts/*`
- `pages/users/*`
- `pages/tenants/*`
- `pages/channels/*`
- `pages/providers/*`
- `pages/skills/*`
- `pages/knowledge/*`
- `pages/scheduled-jobs/*`
- `pages/approvals/*`
- `pages/deployments/*`
- `pages/repo/*`
- `pages/mcp/*`
- `pages/settings/*`
- `pages/telemetry/*`
- `pages/resources/*`
- `pages/glossary/*`
- `pages/auth/login.vue` — 独立美化：全屏 Aurora 背景 + 中央单一玻璃卡登录表单

**验证**：e2e/a11y.spec.ts 跑通；人工巡检每个页面一次。

---

## 8. 验证策略

### 8.1 视觉回归

- 每个 Phase 在 `e2e/screenshots/` 下留一组截图（已有 Playwright 基础设施）
- 对比点：登录页、首页、issues 列表、issue 详情、rca 对话、topology、cluster 详情、settings

### 8.2 性能

- **工具**：Chrome DevTools Performance 录制 10 秒滚动交互
- **指标**：
  - 首页 FCP < 1.5s
  - 全站滚动 FPS ≥ 50（Intel Mac 基线）
  - `backdrop-filter` 层 ≤ 4（避免嵌套模糊）
  - JS Bundle 增量 ≤ 20KB（Geist 字体除外）

### 8.3 可访问性（WCAG 2.1 AA）

**这是玻璃风最容易翻车的点**。
- 所有文本对比度 ≥ 4.5:1（运行 `axe-core` 自动扫描）
- 玻璃卡片上的文本强制 `text-slate-900` 或更深
- 次要信息（`text-slate-500`）只用在 `bg-white` 实色底上，不用在玻璃上
- Focus ring 在所有交互元素上可见（`ring-sky-500/50` 2px）
- Respect `prefers-reduced-motion`：禁用 aurora drift、cursor glow、count-up 等装饰动画

---

## 9. 已识别的风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| `backdrop-filter` 在 Intel 集显上掉帧 | 低端设备体验差 | 嵌套深度 ≤2；`will-change`；有 `prefers-reduced-motion` fallback |
| 玻璃上次要文字对比度不足 | a11y 失败 | 所有次要文字改放在实色 surface 上，玻璃只承载主要文字 |
| Geist 字体 CDN 加载慢/失败 | 首屏字体闪烁 | `font-display: swap` + 本地 fallback `ui-sans-serif` |
| 大面积渐变 + blur 导致 Safari 渲染异常 | Safari 用户视觉 bug | 手动在 Safari 上测每个 Phase；必要时用 `@supports` fallback |
| 分层过渡期视觉不一致 | 1-2 周混搭期 | 用户提前沟通；每个 Phase ≤3 天 |
| 原项目没有 `prefers-reduced-motion` 处理 | 动效强化后晕眩风险 | Phase ② 时统一引入 motion reduce |

---

## 10. 范围与边界

**In scope:**
- 所有 `frontend/` 下的视觉与样式改造
- 新增 4 个 layout 级组件（Aurora / CursorGlow / GlassPanel / CountUp）
- 移除 dark mode 相关代码
- 引入 Geist 字体

**Out of scope:**
- 后端 / API 变更
- i18n 文案修改
- 功能增删
- 移动端响应式（Ops 是桌面工具）
- 性能优化超出"不掉帧"范畴
- 新页面新增

---

## 11. 成功标准

重设计完成后，以下条件同时满足才算"完成"：

1. ✅ 所有 Phase 的 PR merge 到 main
2. ✅ `npm run lint` + `npm run test` + `npm run build` 全绿
3. ✅ `e2e/*.spec.ts` 全绿
4. ✅ 登录页、首页、5 个 hero 页在 Chrome/Safari 上视觉无明显 bug
5. ✅ `axe-core` a11y 扫描无 critical/serious 问题
6. ✅ 性能基线满足（FCP < 1.5s，滚动 FPS ≥ 50）
7. ✅ 设计师/PM 视觉走查通过

---

## 附录 A：关键 ASCII 草图

### Floating Glass Islands 外壳
```
╭─ 10px padding ────────────────────────────────────╮
│                                                   │
│  ╭─Sidebar─╮  ╭──Topbar (h-14)──────╮  ╭──Chat──╮ │
│  │  Logo   │  │ ⟨ Dashboard / Home  │  │  AI    │ │
│  │  ───    │  │ 🔍 Search  ⌘K  🔔 │  │  💬    │ │
│  │OBSERVE  │  ╰────────────────────╯  │        │ │
│  │• Dash.  │  ╭──Content (flex-1)──╮  │        │ │
│  │ Issues  │  │                    │  │        │ │
│  │ Telem.  │  │  <page slot />     │  │        │ │
│  │OPERATE  │  │                    │  │        │ │
│  │ Cluster │  │                    │  │        │ │
│  │ Rollout │  │                    │  │        │ │
│  ╰─────────╯  ╰────────────────────╯  ╰────────╯ │
│                                                   │
│   ◯  aurora sky @ 12% 10%                        │
│        ◯  aurora lavender @ 88% 20%              │
│                 ◯  aurora mint @ 50% 100%         │
╰───────────────────────────────────────────────────╯
```

### Hybrid B 单一玻璃面板表格
```
╭── Glass Panel (bg-white/55, blur 22) ──────────╮
│  ISSUE              CLOUD  REGION  AGE  OWNER  │  ← glass header
├────────────────────────────────────────────────┤
│ ● High CPU on ...    [AWS] us-east  2m  @x     │  ← row hover 流过彩光
│ ● RDS conn pool ...  [AWS] eu-west  5m  @y     │
│ ● 5xx spike api-gw   [GCP] us-cent  7m  @z     │
│ ◐ Memory > 80% ...   [GCP] asia-ne  12m @ops   │
│ ◐ Canary paused      [AWS] us-west  18m @dep   │
│  ...                                           │
╰────────────────────────────────────────────────╯
```

---

**End of spec.**
