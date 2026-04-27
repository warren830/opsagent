# Holographic Glass · Phase ③ Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the app shell from "edge-to-edge panels with borders" to **Floating Glass Islands** — four independent glass surfaces (Sidebar · Topbar · Content · Chat) floating on an Aurora gradient background, separated by 10px gaps. Mount CursorGlow + AuroraBackground layout primitives.

**Architecture:** `layouts/default.vue` is fully rewritten (it's only 27 lines). The three existing layout components (AppHeader / AppSidebar / ChatPanel) have their **outermost container classes** swapped for glass-panel surfaces — internal structure / nav logic / chat logic is NOT touched. Two new layout-level components (`AuroraBackground`, `CursorGlow`) are mounted in the layout root as `fixed` decorative layers.

**Tech Stack:** Vue 3 Composition API, Tailwind, `@vueuse/core` (already installed) for `useMouse` in CursorGlow. No new dependencies.

**Companion Spec:** `docs/superpowers/specs/2026-04-27-frontend-holographic-glass-redesign-design.md` §4  
**Predecessor Plans:** Phase ① Foundation (merged) · Phase ② Components (merged)

**Phase scope:** `frontend/layouts/default.vue` + `frontend/components/layout/*.vue` + 2 new components. Do NOT touch any `pages/`, `components/ui/`, or `assets/css/tailwind.css` — those are done (Phase ①/②) or deferred (Phase ④/⑤).

---

## File Structure

### Files Created
| Path | Responsibility |
|------|---------------|
| `frontend/components/layout/AuroraBackground.vue` | `fixed inset-0 -z-10` decorative radial-gradient layer with slow `aurora-drift` keyframe (drifts 45s loop). Reads `--aurora-sky`, `--aurora-lavender`, `--aurora-mint` CSS vars from Phase ① |
| `frontend/components/layout/CursorGlow.vue` | `fixed pointer-events-none -z-10` soft-light blob that follows the cursor via `useMouse()` from `@vueuse/core`. Respects `prefers-reduced-motion`. |

### Files Modified
| Path | Responsibility of change |
|------|-------------------------|
| `frontend/layouts/default.vue` | Full rewrite: 4 island grid + AuroraBackground + CursorGlow mount; retains `chatFullscreen` + `mobileSidebarOpen` logic |
| `frontend/components/layout/AppSidebar.vue` | Root `<aside>` class swap only — `bg-card/50 border-r border-border/60` → `glass-panel` + soft shadow + rounded-[14px]. KEEP navGroups, collapsed state, mobile overlay, tooltip tree |
| `frontend/components/layout/AppHeader.vue` | Root `<header>` class swap — `sticky top-0 border-b bg-card/80 backdrop-blur-xl` → `glass-panel` island + rounded-[14px]; content inside (logo, lang switch, logout) untouched |
| `frontend/components/layout/ChatPanel.vue` | **ONLY** the root `<aside>` class at line ~1105 — `border-l border-border/60 bg-background` → `glass-panel`. DO NOT change the 1800 lines of chat internals. The sidebar-of-chat header (`bg-card/50` at line ~1120) also gets tweaked. |

### Files Deleted
None.

---

## Execution Rules

- **Each change lives in its own commit**. 8 commits total across 8 tasks.
- **Every commit ends with `cd frontend && npm run build` passing.**
- **Never stage** `frontend/package.json` / `frontend/package-lock.json` — vitest WIP.
- **Don't push**. User will push when Phase ③ concludes.
- **No `Co-Authored-By:` lines. No `--no-verify`.**
- **Dev server**: running on `:9999`. Kiro's `:3000` is user's own.
- **Visual checkpoints**: each task (especially 3/4/5/6) should be visually verified by curl-ing `http://localhost:9999/` (redirects to login, that's fine — check status) AND by looking at `/_style-demo` OR the real dashboard to confirm the new shell renders. A subagent should note if the dev server has fallen over (`npm run build` kills it) and explicitly restart.

---

## Task 1 · Create AuroraBackground component

**Files:**
- Create: `frontend/components/layout/AuroraBackground.vue`

The aurora background is a `fixed inset-0` decorative layer that sits behind every page's content. It uses the CSS variables installed in Phase ① (`--aurora-sky`, `--aurora-lavender`, `--aurora-mint`) and the `aurora-drift` keyframe (also from Phase ①).

- [ ] **Step 1.1: Write the component**

```vue
<!-- frontend/components/layout/AuroraBackground.vue -->
<script setup lang="ts">
// Fixed full-viewport gradient-halo layer.
// Sits at z-index 0 so ALL content is above; layout islands at z-10+.
//
// The radial gradients use the --aurora-* CSS vars defined in
// assets/css/tailwind.css :root. Each "stop" is independently animated
// to create slow drifting motion — 45s loop, respects prefers-reduced-motion.
</script>

<template>
  <div aria-hidden="true" class="fixed inset-0 -z-10 pointer-events-none overflow-hidden bg-white">
    <div
      class="absolute inset-0 motion-safe:animate-[aurora-drift_45s_ease-in-out_infinite]"
      :style="{
        background: `
          radial-gradient(ellipse 50% 40% at 12% 10%, var(--aurora-sky), transparent 55%),
          radial-gradient(ellipse 55% 45% at 88% 22%, var(--aurora-lavender), transparent 55%),
          radial-gradient(ellipse 60% 50% at 50% 100%, var(--aurora-mint), transparent 55%)
        `,
      }"
    />
  </div>
</template>
```

Notes:
- `motion-safe:animate-[...]` only runs the animation for users WITHOUT `prefers-reduced-motion: reduce`.
- `-z-10` puts it behind all normal stacking contexts. Layout islands default to z-0/z-10 and appear on top.
- `bg-white` as fallback so the page is white-bottomed even before the gradients render.

- [ ] **Step 1.2: Verify build**

```bash
cd /Users/ychchen/warren_ws/opsagent/frontend && npm run build
```
Must PASS. (Component isn't used yet — just make sure it compiles.)

- [ ] **Step 1.3: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/layout/AuroraBackground.vue
git commit -m "feat(layout): add AuroraBackground component"
```

---

## Task 2 · Create CursorGlow component

**Files:**
- Create: `frontend/components/layout/CursorGlow.vue`

The cursor glow follows the mouse with a subtle soft-light blob. Visible only on devices with a mouse (disabled on touch + for `prefers-reduced-motion`).

- [ ] **Step 2.1: Write the component**

```vue
<!-- frontend/components/layout/CursorGlow.vue -->
<script setup lang="ts">
import { useMouse, useMediaQuery } from '@vueuse/core'
import { computed } from 'vue'

const { x, y } = useMouse({ type: 'page' })
const reduceMotion = useMediaQuery('(prefers-reduced-motion: reduce)')
const isTouch = useMediaQuery('(hover: none)')

const visible = computed(() => !reduceMotion.value && !isTouch.value)

const transform = computed(() => `translate(${x.value - 220}px, ${y.value - 220}px)`)
</script>

<template>
  <div
    v-if="visible"
    aria-hidden="true"
    class="fixed top-0 left-0 -z-10 pointer-events-none w-[440px] h-[440px] rounded-full opacity-60 mix-blend-multiply transition-[opacity] duration-300"
    :style="{
      transform,
      background: 'radial-gradient(circle, rgba(147,197,253,0.25), rgba(196,181,253,0.1) 40%, transparent 70%)',
      willChange: 'transform',
    }"
  />
</template>
```

Notes:
- `useMouse({ type: 'page' })` gives us page-relative coordinates (works with scroll).
- `mix-blend-multiply` softens the glow against the light aurora underneath.
- `willChange: transform` gives the browser a hint for GPU compositing.
- The glow is 440px square (large enough to feel "ambient" without pinpointing the cursor exactly).
- The `transition-[opacity]` means the glow fades in/out when `visible` toggles (e.g., user plugs in / unplugs a trackpad).

- [ ] **Step 2.2: Verify build**

```bash
cd /Users/ychchen/warren_ws/opsagent/frontend && npm run build
```
Must PASS. Confirm `@vueuse/core` exports `useMouse` and `useMediaQuery` — they should (v14.x). If not, use `import.meta` checks or a `browser-only` wrapper.

- [ ] **Step 2.3: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/layout/CursorGlow.vue
git commit -m "feat(layout): add CursorGlow component with reduced-motion and touch guards"
```

---

## Task 3 · Rewrite `layouts/default.vue` — Floating Glass Islands

**Files:**
- Modify: `frontend/layouts/default.vue` (full rewrite)

Target structure: 10px padding around the viewport, sidebar+main_column+chat laid out as a flex row, main_column holds header+content as a flex column, 10px gap between everything.

- [ ] **Step 3.1: Rewrite the layout**

Replace the ENTIRE contents of `frontend/layouts/default.vue` with:

```vue
<script setup lang="ts">
definePageMeta({ middleware: 'auth' })

const chatFullscreen = useState('chatFullscreen', () => false)
const mobileSidebarOpen = useState('mobileSidebarOpen', () => false)
</script>

<template>
  <div class="relative min-h-screen overflow-hidden">
    <!-- Decorative background layers -->
    <LayoutAuroraBackground />
    <LayoutCursorGlow />

    <!-- Floating Glass Islands grid -->
    <div class="relative z-10 flex h-screen gap-2.5 p-2.5">
      <!-- Mobile sidebar overlay (tap-to-close) -->
      <div
        v-if="mobileSidebarOpen && !chatFullscreen"
        class="fixed inset-0 z-30 bg-slate-900/30 backdrop-blur-sm md:hidden"
        @click="mobileSidebarOpen = false"
      />

      <!-- Island 1: Sidebar (hidden when chat fullscreen) -->
      <LayoutAppSidebar v-if="!chatFullscreen" />

      <!-- Middle column: Topbar + Content, stacked -->
      <div v-if="!chatFullscreen" class="flex min-w-0 flex-1 flex-col gap-2.5">
        <!-- Island 2: Topbar -->
        <LayoutAppHeader />

        <!-- Island 3: Content -->
        <main class="glass-panel min-w-0 flex-1 overflow-auto p-5">
          <slot />
        </main>
      </div>

      <!-- Island 4: Chat Panel -->
      <LayoutChatPanel class="hidden md:flex" :class="{ '!flex !flex-1': chatFullscreen }" />
    </div>
  </div>
</template>
```

Key structural notes:
- Outer wrapper: `relative min-h-screen overflow-hidden` so Aurora + CursorGlow are clipped to viewport
- Grid is `flex gap-2.5 p-2.5` (10px padding + 10px gaps)
- Middle column is also a flex stack with `gap-2.5` between Topbar and Content
- When chat goes fullscreen, Sidebar + middle column are hidden, chat takes everything
- `glass-panel` utility (Phase ①) applied directly to `<main>` for Island 3 — no need for a nested component here
- Islands 1/2/4 get their `glass-panel` treatment from Tasks 4/5/6 (inside their own components)

- [ ] **Step 3.2: Verify build + dev**

```bash
cd /Users/ychchen/warren_ws/opsagent/frontend && npm run build
```
Must PASS.

Restart dev if needed (see Execution Rules). Then visually confirm at `http://localhost:9999/` — there should be VISIBLE 10px gaps around everything, and you should see the Aurora gradient in the gaps.

- [ ] **Step 3.3: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/layouts/default.vue
git commit -m "feat(layout): rewrite default layout as Floating Glass Islands"
```

---

## Task 4 · Restyle AppSidebar root

**File:**
- Modify: `frontend/components/layout/AppSidebar.vue`

Change ONLY the root `<aside>` class (and the adjacent hover/collapse visuals if they break). DO NOT touch nav group logic, collapse state, mobile overlay logic, tooltip setup, or anything inside the `<template>` below line ~170.

- [ ] **Step 4.1: Update root `<aside>` class**

Find (at line ~153):
```vue
  <aside
    class="flex-col border-r border-border/60 bg-card/50 transition-all duration-200 shrink-0"
    :class="[
      collapsed ? 'w-12' : 'w-52',
      mobileSidebarOpen
        ? 'fixed inset-y-0 left-0 z-40 flex h-screen pt-12'
        : 'hidden md:flex'
    ]"
  >
```

Replace with:
```vue
  <aside
    class="glass-panel flex-col overflow-hidden transition-[width,transform] duration-200 shrink-0"
    :class="[
      collapsed ? 'w-14' : 'w-52',
      mobileSidebarOpen
        ? 'fixed inset-y-2.5 left-2.5 z-40 flex h-[calc(100vh-20px)]'
        : 'hidden md:flex'
    ]"
  >
```

Changes:
- `border-r border-border/60 bg-card/50` → `glass-panel` (Phase ① utility: glass bg + border + shadow + rounded-[14px])
- Added `overflow-hidden` so the rounded corners actually clip content
- Changed `w-12` → `w-14` (slight bump, 56px, better for icon-only mode with glass padding)
- Mobile: `inset-y-0 left-0` → `inset-y-2.5 left-2.5` so mobile overlay respects the 10px island padding. Width calc `h-[calc(100vh-20px)]` accounts for top + bottom padding.
- Removed `pt-12` (mobile header spacing) — no longer needed since header is its own island.
- Transition now targets only `width` + `transform` (not everything).

- [ ] **Step 4.2: Check for other hardcoded colors in the sidebar that need tweaking**

Run:
```bash
grep -nE "bg-card|border-border|bg-background" /Users/ychchen/warren_ws/opsagent/frontend/components/layout/AppSidebar.vue
```

List the results. For each hit:
- If it's on a sub-element (nav item hover state, group header, etc.): leave it — these work fine on glass background
- If it's a big surface (panel-like wrapper): change to match glass
- If unsure: leave alone and flag in your report

- [ ] **Step 4.3: Verify build + visual**

```bash
cd frontend && npm run build
```
Must PASS. If dev server is up, refresh and confirm the sidebar is now an island with gap + soft rounded corners.

- [ ] **Step 4.4: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/layout/AppSidebar.vue
git commit -m "feat(layout): sidebar becomes floating glass island"
```

---

## Task 5 · Restyle AppHeader root

**File:**
- Modify: `frontend/components/layout/AppHeader.vue`

- [ ] **Step 5.1: Update root `<header>` class**

Find the current header root (line ~17 per earlier inspection):
```vue
<header class="sticky top-0 z-50 w-full border-b border-border/60 bg-card/80 backdrop-blur-xl">
  <div class="flex h-12 items-center px-4 gap-3">
```

Replace with:
```vue
<header class="glass-panel shrink-0 z-50 w-full">
  <div class="flex h-12 items-center px-4 gap-3">
```

Changes:
- Removed `sticky top-0` (it's now a flex sibling, not stuck to viewport edge)
- Removed `border-b border-border/60 bg-card/80 backdrop-blur-xl` — replaced by `glass-panel` utility
- Added `shrink-0` so the header doesn't get squeezed when content is tall
- Kept `z-50` for dropdown/popover layering

The inner `<div>` with `h-12` stays. That's the internal chrome; leave it.

- [ ] **Step 5.2: Check for other header-internal dark colors**

```bash
grep -nE "bg-card|bg-background|text-foreground" /Users/ychchen/warren_ws/opsagent/frontend/components/layout/AppHeader.vue
```

For each hit, evaluate whether it still reads correctly on light glass. Most should be fine (text-foreground is slate-900 now, reads fine on glass).

- [ ] **Step 5.3: Verify build + visual**

```bash
cd frontend && npm run build
```

- [ ] **Step 5.4: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/layout/AppHeader.vue
git commit -m "feat(layout): topbar becomes floating glass island"
```

---

## Task 6 · Restyle ChatPanel root (outer container ONLY)

**File:**
- Modify: `frontend/components/layout/ChatPanel.vue`

**CRITICAL**: Do NOT touch the 1800 lines of chat logic. Only modify the root `<aside>` class and the inner chat-header `<div>` that sits right below it. Nothing else.

- [ ] **Step 6.1: Update root `<aside>` class**

Find (at line ~1105):
```vue
    <aside
      v-if="chatOpen"
      class="flex flex-col border-l border-border/60 bg-background relative"
      :class="chatFullscreen ? 'flex-1' : ''"
      style="height: 100%;"
      :style="chatFullscreen ? {} : { width: `${panelWidth}px` }"
    >
```

Replace with:
```vue
    <aside
      v-if="chatOpen"
      class="glass-panel flex flex-col relative overflow-hidden shrink-0"
      :class="chatFullscreen ? '!flex-1' : ''"
      :style="chatFullscreen ? {} : { width: `${panelWidth}px` }"
    >
```

Changes:
- `border-l border-border/60 bg-background` → `glass-panel`
- Added `overflow-hidden` so rounded corners clip the internal scroll
- Added `shrink-0` so the chat doesn't get compressed when resized
- Removed `style="height: 100%"` — the flex parent in layouts/default.vue already gives it full height of the island row
- Conditional class now uses `!flex-1` to override any `flex` ordering (Tailwind arbitrary-priority)

- [ ] **Step 6.2: Update the internal chat header**

Find (at line ~1120):
```vue
      <div class="flex items-center justify-between px-3 h-10 border-b border-border/60 shrink-0 bg-card/50">
```

Replace with:
```vue
      <div class="flex items-center justify-between px-3 h-10 border-b border-slate-200/60 shrink-0 bg-white/40 backdrop-blur-sm">
```

Rationale: the inner chat header sits inside the glass-panel outer container. It needs a subtle inset to distinguish from the chat body without breaking the glass illusion.

- [ ] **Step 6.3: Check other ChatPanel surfaces**

Run:
```bash
grep -nE "bg-background|bg-card[/ ]|border-border" /Users/ychchen/warren_ws/opsagent/frontend/components/layout/ChatPanel.vue | head -15
```

List and judge each. For THIS task, only change the top-level outer shell + the immediate header. Any internal surfaces (messages, tool calls, input) are OUT OF SCOPE — that will be part of Phase ④ RCA/Chat page work.

- [ ] **Step 6.4: Verify build + visual**

```bash
cd frontend && npm run build
```
Confirm the chat panel now floats with glass + gap.

- [ ] **Step 6.5: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/layout/ChatPanel.vue
git commit -m "feat(layout): chat panel becomes floating glass island (outer shell only)"
```

---

## Task 7 · Accessibility: reduced-motion guards

**Files:**
- Modify: `frontend/assets/css/tailwind.css` — add a `@media (prefers-reduced-motion: reduce)` block that neutralizes all Phase ① motion animations

Phase ① registered `aurora-drift`, `glass-hover`, `neon-sweep`, `fade-in-up`, `typing-dot`, etc. For users with `prefers-reduced-motion: reduce`, all of these should collapse to instant transitions.

- [ ] **Step 7.1: Add a reduced-motion block**

Edit `frontend/assets/css/tailwind.css`. Add at the bottom of the file (after the `@layer utilities` block):

```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0ms !important;
    scroll-behavior: auto !important;
  }
}
```

This is the industry-standard reduced-motion pattern. It collapses all animations and transitions to 0ms while still letting visual state changes happen (e.g., hover still works, just without animation).

Note: The `motion-safe:animate-[aurora-drift_...]` class in AuroraBackground (Task 1) ALREADY respects this preference natively (Tailwind gives us `motion-safe:` which means "only animate if user hasn't opted out"). So that's double-covered.

- [ ] **Step 7.2: Verify build**

```bash
cd frontend && npm run build
```

- [ ] **Step 7.3: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/assets/css/tailwind.css
git commit -m "a11y(css): globally honor prefers-reduced-motion"
```

---

## Task 8 · Final verify — build, lint, tests, visual diff, full audit

**Files:**
- No code change.

- [ ] **Step 8.1: Clean build + lint + tests**

```bash
cd /Users/ychchen/warren_ws/opsagent/frontend && rm -rf .nuxt && npm run build
npm run lint
npm run test
```
Expected: all PASS.

- [ ] **Step 8.2: Restart dev server**

```bash
pkill -9 -f 'nuxt dev' 2>/dev/null; sleep 2
cd /Users/ychchen/warren_ws/opsagent/frontend && rm -rf .nuxt node_modules/.vite
PORT=9999 nohup npm run dev > /tmp/ops-9999.log 2>&1 &
sleep 25
curl -sI http://localhost:9999/style-demo | head -1
```
Expected: `HTTP/1.1 200 OK`.

- [ ] **Step 8.3: Capture after-phase-3 screenshots**

```bash
cd /Users/ychchen/warren_ws/opsagent/e2e
rm -rf screenshots/audit
npx playwright test full-audit.spec.ts --project=chromium 2>&1 | tail -5
```
Expected: 20/20 routes captured.

Then visually inspect:
- `screenshots/audit/home.png` — should show 4 distinct floating islands with visible Aurora gradient in the 10px gaps between them
- `screenshots/audit/accounts.png` — same shell, content (table) inside its Content island
- `screenshots/audit/topology.png` — canvas fills the Content island, aurora visible at edges
- `screenshots/audit/settings.png` — form fits inside the Content island

- [ ] **Step 8.4: Completion check**

```bash
git log --oneline 91a20cc..HEAD  # Phase 3 commits (~8 commits: 2 new components + 4 layout changes + a11y + verify-commit-is-absent-or-just-screenshots)
```

Expect ~7-8 commits. Last one should be this task's screenshot commit (next step).

- [ ] **Step 8.5: Commit audit screenshots**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add e2e/screenshots/audit/
git commit -m "test(e2e): phase 3 floating-glass-islands audit snapshots"
```

- [ ] **Step 8.6: Performance spot-check**

Open DevTools → Performance in Chrome (on user's side). Record 5 seconds of scrolling at `http://localhost:9999/issues` or `/accounts`. Verify frame rate stays ≥ 50 FPS on modern laptops. If the CursorGlow or AuroraBackground drift is causing jank, log it as a concern — a future polish commit can tune blur radius or disable the drift animation.

(Subagent need not actually run DevTools — just flag this as a known follow-up for the controller to ask the user to verify.)

## Phase ③ Completion Criteria

All must be true:

1. ✅ All 8 tasks have every checkbox ticked
2. ✅ `cd frontend && npm run build` exits 0
3. ✅ `cd frontend && npm run lint` exits 0
4. ✅ `cd frontend && npm run test` exits 0
5. ✅ `frontend/components/layout/` now contains `AuroraBackground.vue` + `CursorGlow.vue`
6. ✅ `grep -c "bg-card/50\|bg-card/80\|bg-background" frontend/layouts/default.vue frontend/components/layout/AppHeader.vue frontend/components/layout/AppSidebar.vue` returns 0 for default/header; sidebar may still have sub-element hits which are OK
7. ✅ Visual diff — home/accounts/topology pages show four distinct floating islands
8. ✅ `prefers-reduced-motion: reduce` media query present in tailwind.css
9. ✅ `frontend/package.json` / `frontend/package-lock.json` vitest WIP still uncommitted
10. ✅ Git log shows 7-8 atomic commits, no pushes

---

## What Phase ③ does NOT do

Deliberately deferred:

- ❌ Touching any `pages/*.vue` (Phase ④/⑤)
- ❌ Touching `components/ui/*` (done in Phase ②)
- ❌ Modifying chat internal rendering, markdown, tool-call cards, mermaid styling (Phase ④ will revisit the RCA/chat experience as a hero page)
- ❌ Replacing `SidebarCollapsible` menu structure or icon set (layout-only; nav logic preserved)
- ❌ Adding new nav items or restructuring the nav groups
- ❌ Adding npm deps beyond what's already in package.json (we use `@vueuse/core` which is already installed)
- ❌ Making mobile layouts responsive beyond the existing mobile-sidebar-overlay behavior

**End of Phase ③ plan.**
