# Holographic Glass · Phase ② Component Library Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle every UI component in `frontend/components/ui/` to match the Sky & Lavender glass aesthetic from Phase ①, and add two new primitives (GlassPanel, CountUp) that later phases depend on.

**Architecture:** shadcn-vue components stay structurally intact — we only rewrite `cva` variant strings and replace hardcoded dark colors with token-based / light-theme equivalents. Two new components live in `components/ui/glass-panel/` and `components/ui/count-up/` following the existing folder + `index.ts` pattern.

**Tech Stack:** Vue 3 Composition API, Tailwind CSS, `class-variance-authority` (cva), Radix Vue primitives, existing shadcn-vue structure. No new runtime dependencies.

**Companion Spec:** `docs/superpowers/specs/2026-04-27-frontend-holographic-glass-redesign-design.md`  
**Predecessor Plan:** `docs/superpowers/plans/2026-04-27-holographic-glass-phase-1-foundation.md` (merged to `main`)

**Phase scope:** ONLY `frontend/components/ui/` + one demo page. Do NOT touch `layouts/`, `pages/` (other than the temporary demo page), `assets/`, or `tailwind.config.ts` — those belong to Phase ① (done) / ③ (next).

---

## File Structure

### Files Created
| Path | Responsibility |
|------|---------------|
| `frontend/components/ui/glass-panel/GlassPanel.vue` | Single floating / nested glass surface primitive with `subtle`/`default`/`strong` variants; used by list pages + dashboards in Phase ④ |
| `frontend/components/ui/glass-panel/index.ts` | Barrel export following existing UI pattern |
| `frontend/components/ui/count-up/CountUp.vue` | Number scrolling animation; used in Phase ④ dashboard stat cards |
| `frontend/components/ui/count-up/index.ts` | Barrel export |
| `frontend/pages/_style-demo.vue` | Internal live reference of every UI component + variant — NOT linked from navigation; used only for visual regression |
| `e2e/phase2-components.spec.ts` | Screenshot spec that captures the demo page for before/after diff |

### Files Modified
| Path | Responsibility of change |
|------|-------------------------|
| `frontend/components/ui/button/Button.vue` | Gradient default variant + glass secondary/outline; sky/violet ring |
| `frontend/components/ui/card/Card.vue` | Glass surface + hover lift |
| `frontend/components/ui/badge/Badge.vue` | Soft tint variants + `rounded-full`; fixes Phase ① "purple chip explosion" |
| `frontend/components/ui/input/Input.vue` | Glass input with sky focus ring |
| `frontend/components/ui/textarea/Textarea.vue` | Match Input treatment |
| `frontend/components/ui/select/SelectTrigger.vue` | Match Input treatment |
| `frontend/components/ui/select/SelectContent.vue` | Glass popover with large radius |
| `frontend/components/ui/select/SelectItem.vue` | Light hover + sky active |
| `frontend/components/ui/checkbox/Checkbox.vue` | Gradient checked state + glass unchecked |
| `frontend/components/ui/switch/Switch.vue` | Gradient on, glass off |
| `frontend/components/ui/separator/Separator.vue` | Fading gradient line replaces solid |
| `frontend/components/ui/dialog/DialogContent.vue` | Glass + 20px radius + aurora backdrop overlay |
| `frontend/components/ui/popover/PopoverContent.vue` | Glass popover |
| `frontend/components/ui/tooltip/TooltipContent.vue` | Inverted dark glass for legibility over any light surface |
| `frontend/components/ui/sonner/Sonner.vue` | Glass cards + spring slide-in |
| `frontend/components/ui/skeleton/Skeleton.vue` | Soft shimmer `from-slate-100 via-white to-slate-100` |
| `frontend/components/ui/avatar/Avatar.vue` | Ring + gradient fallback |
| `frontend/components/ui/scroll-area/ScrollArea*.vue` | Thumbnail thumb color token bump |
| `frontend/components/ui/collapsible/` | Minimal — mostly inherits from container |

### Files Deleted
None. All components are restyled in-place.

---

## Execution Rules

- **Each UI component restyle lives in its own commit** — fine-grained, reviewable, revertable.
- **Every commit ends with `cd frontend && npm run build` passing**. The build test is the primary regression check at this stage.
- **Before `git add <file>`, run `git status --short` on that file** to confirm no pre-existing WIP is being bundled. Remember: `frontend/package.json` and `frontend/package-lock.json` carry uncommitted vitest WIP — never stage those.
- **Don't push.** User will push when Phase ② concludes.
- **No `Co-Authored-By:` lines. No `--no-verify`.**
- **Dev server**: running on `:9999` (Kiro holds `:3000`). For visual inspection, use `http://localhost:9999/_style-demo`. If dev server needs a restart: `pkill -9 -f 'nuxt dev'; cd frontend && rm -rf .nuxt node_modules/.vite && PORT=9999 npm run dev &`
- **Subagent note**: each task is bite-sized; Sonnet for mechanical rewrites, Opus for Badge (the important one) and the final verification.

---

## Task 1 · Create internal style-demo page + baseline screenshots

**Files:**
- Create: `frontend/pages/_style-demo.vue`
- Create: `e2e/phase2-components.spec.ts`

This is the visual regression harness for the whole phase. Do this FIRST so subsequent tasks can immediately see their impact.

- [ ] **Step 1.1: Write the demo page**

```vue
<!-- frontend/pages/_style-demo.vue -->
<script setup lang="ts">
// Internal reference of every UI component + variant. NOT linked from nav.
// Used for visual regression during Phase ② component restyle.
definePageMeta({ layout: false })
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle, CardDescription, CardFooter } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from '@/components/ui/select'
import { Checkbox } from '@/components/ui/checkbox'
import { Switch } from '@/components/ui/switch'
import { Separator } from '@/components/ui/separator'
import { Skeleton } from '@/components/ui/skeleton'
import { Avatar } from '@/components/ui/avatar'

const inputVal = ref('')
const selectVal = ref('a')
const checkboxVal = ref(true)
const switchVal = ref(true)
</script>

<template>
  <div class="aurora-bg min-h-screen p-10 space-y-10 text-slate-900">
    <header>
      <h1 class="text-2xl font-semibold tracking-tight">UI Component Reference</h1>
      <p class="text-sm text-slate-500 mt-1">Sky & Lavender · Phase ② · every variant on one page</p>
    </header>

    <section>
      <h2 class="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Buttons</h2>
      <div class="flex flex-wrap gap-2">
        <Button>Default</Button>
        <Button variant="secondary">Secondary</Button>
        <Button variant="outline">Outline</Button>
        <Button variant="ghost">Ghost</Button>
        <Button variant="link">Link</Button>
        <Button variant="destructive">Destructive</Button>
        <Button variant="success">Success</Button>
        <Button disabled>Disabled</Button>
        <Button size="sm">Small</Button>
        <Button size="lg">Large</Button>
      </div>
    </section>

    <section>
      <h2 class="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Badges</h2>
      <div class="flex flex-wrap gap-2">
        <Badge>Default</Badge>
        <Badge variant="secondary">Secondary</Badge>
        <Badge variant="destructive">Critical</Badge>
        <Badge variant="warning">Warning</Badge>
        <Badge variant="success">Success</Badge>
        <Badge variant="info">Info</Badge>
        <Badge variant="outline">Outline</Badge>
      </div>
    </section>

    <section>
      <h2 class="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Cards</h2>
      <div class="grid grid-cols-3 gap-4">
        <Card>
          <CardHeader>
            <CardTitle>Card title</CardTitle>
            <CardDescription>Supporting description text</CardDescription>
          </CardHeader>
          <CardContent>
            <p class="text-sm text-slate-600">Body content sits here.</p>
          </CardContent>
          <CardFooter>
            <Button size="sm">Action</Button>
          </CardFooter>
        </Card>
        <Card class="glass-hover-lift">
          <CardHeader><CardTitle>With hover lift</CardTitle></CardHeader>
          <CardContent><p class="text-sm text-slate-600">Hover me</p></CardContent>
        </Card>
      </div>
    </section>

    <section>
      <h2 class="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Form inputs</h2>
      <div class="grid grid-cols-2 gap-4 max-w-3xl">
        <Input v-model="inputVal" placeholder="Text input" />
        <Textarea v-model="inputVal" placeholder="Textarea" rows="3" />
        <Select v-model="selectVal">
          <SelectTrigger><SelectValue placeholder="Pick one" /></SelectTrigger>
          <SelectContent>
            <SelectItem value="a">Option A</SelectItem>
            <SelectItem value="b">Option B</SelectItem>
            <SelectItem value="c">Option C</SelectItem>
          </SelectContent>
        </Select>
        <div class="flex items-center gap-4">
          <Checkbox v-model:checked="checkboxVal" />
          <Switch v-model:checked="switchVal" />
        </div>
      </div>
    </section>

    <section>
      <h2 class="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Feedback</h2>
      <div class="space-y-3 max-w-3xl">
        <Separator />
        <div class="flex items-center gap-3">
          <Avatar fallback="AB" />
          <div class="flex-1 space-y-2">
            <Skeleton class="h-3 w-1/2" />
            <Skeleton class="h-3 w-2/3" />
          </div>
        </div>
      </div>
    </section>

    <section>
      <h2 class="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Glass primitives (Phase ② new)</h2>
      <!-- GlassPanel + CountUp will be rendered here after Task 2. For now empty placeholder. -->
      <div class="text-sm text-slate-400 italic">See after Task 2</div>
    </section>
  </div>
</template>
```

- [ ] **Step 1.2: Write the Phase ② screenshot spec**

```ts
// e2e/phase2-components.spec.ts
import { test } from '@playwright/test'

const LABEL = process.env.PHASE2_LABEL || 'baseline'
const OUT = `screenshots/${LABEL}-phase2`

test.describe('Phase 2 Components snapshots', () => {
  test.use({ viewport: { width: 1280, height: 2400 } })

  test('capture style-demo full page', async ({ page }) => {
    await page.goto('http://localhost:9999/_style-demo')
    await page.waitForLoadState('networkidle')
    await page.waitForTimeout(800)
    await page.screenshot({ path: `${OUT}/style-demo.png`, fullPage: true })
  })
})
```

- [ ] **Step 1.3: Start dev server + verify demo page loads**

```bash
pkill -9 -f 'nuxt dev' 2>/dev/null; sleep 2
cd /Users/ychchen/warren_ws/opsagent/frontend && rm -rf .nuxt node_modules/.vite
PORT=9999 nohup npm run dev > /tmp/ops-9999.log 2>&1 &
sleep 25
curl -sI http://localhost:9999/_style-demo | head -1
```
Expected: `HTTP/1.1 200 OK`.

- [ ] **Step 1.4: Capture baseline**

```bash
cd /Users/ychchen/warren_ws/opsagent/e2e
PHASE2_LABEL=baseline npx playwright test phase2-components.spec.ts --project=chromium
ls -la screenshots/baseline-phase2/style-demo.png
```
Expected: a single PNG exists, shows current component styles (which are correct Phase ①-era state — Badge still purple-explodes, button gradient is already sky/violet thanks to Phase 1.5, etc.).

- [ ] **Step 1.5: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/pages/_style-demo.vue e2e/phase2-components.spec.ts
git commit -m "test(ui): style-demo page and phase 2 screenshot spec"
```

Note: baseline PNG is NOT committed yet — we'll stage both baseline and after together in the final task.

---

## Task 2 · Create GlassPanel + CountUp primitives

**Files:**
- Create: `frontend/components/ui/glass-panel/GlassPanel.vue`
- Create: `frontend/components/ui/glass-panel/index.ts`
- Create: `frontend/components/ui/count-up/CountUp.vue`
- Create: `frontend/components/ui/count-up/index.ts`
- Modify: `frontend/pages/_style-demo.vue` (replace the placeholder at the bottom with real glass/countup usage)

### GlassPanel

- [ ] **Step 2.1: Write `GlassPanel.vue`**

```vue
<!-- frontend/components/ui/glass-panel/GlassPanel.vue -->
<script setup lang="ts">
import { type HTMLAttributes } from 'vue'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

const glassPanelVariants = cva(
  'border transition-shadow',
  {
    variants: {
      variant: {
        subtle:  'glass-panel-subtle',   // utilities defined in Phase ① tailwind.css
        default: 'glass-panel',
        strong:  'glass-panel shadow-[0_12px_40px_rgba(100,140,200,0.16)]',
      },
      hover: {
        none: '',
        lift: 'glass-hover-lift',
      },
    },
    defaultVariants: {
      variant: 'default',
      hover: 'none',
    },
  },
)

type GlassPanelVariants = VariantProps<typeof glassPanelVariants>

defineProps<{
  variant?: NonNullable<GlassPanelVariants['variant']>
  hover?: NonNullable<GlassPanelVariants['hover']>
  as?: string
  class?: HTMLAttributes['class']
}>()
</script>

<template>
  <component
    :is="as || 'div'"
    :class="cn(glassPanelVariants({ variant, hover }), $props.class)"
  >
    <slot />
  </component>
</template>
```

- [ ] **Step 2.2: Write `glass-panel/index.ts`**

```ts
// frontend/components/ui/glass-panel/index.ts
export { default as GlassPanel } from './GlassPanel.vue'
```

### CountUp

- [ ] **Step 2.3: Write `CountUp.vue`**

```vue
<!-- frontend/components/ui/count-up/CountUp.vue -->
<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from 'vue'

const props = withDefaults(defineProps<{
  to: number
  duration?: number      // ms
  decimals?: number
  format?: (n: number) => string
}>(), {
  duration: 800,
  decimals: 0,
})

const displayed = ref(0)
let raf: number | null = null
let start = 0
let startTs = 0

function animate(to: number) {
  if (raf !== null) cancelAnimationFrame(raf)
  start = displayed.value
  startTs = performance.now()
  const frame = (now: number) => {
    const t = Math.min(1, (now - startTs) / props.duration)
    const eased = 1 - Math.pow(1 - t, 3) // easeOutCubic
    displayed.value = start + (to - start) * eased
    if (t < 1) raf = requestAnimationFrame(frame)
    else raf = null
  }
  raf = requestAnimationFrame(frame)
}

function formatted(n: number) {
  if (props.format) return props.format(n)
  return n.toLocaleString(undefined, {
    minimumFractionDigits: props.decimals,
    maximumFractionDigits: props.decimals,
  })
}

onMounted(() => animate(props.to))
watch(() => props.to, newVal => animate(newVal))
onUnmounted(() => { if (raf !== null) cancelAnimationFrame(raf) })
</script>

<template>
  <span class="tabular-nums">{{ formatted(displayed) }}</span>
</template>
```

- [ ] **Step 2.4: Write `count-up/index.ts`**

```ts
// frontend/components/ui/count-up/index.ts
export { default as CountUp } from './CountUp.vue'
```

- [ ] **Step 2.5: Wire demo section for new primitives**

Edit `frontend/pages/_style-demo.vue`. Find the placeholder section `<!-- GlassPanel + CountUp will be rendered here after Task 2. -->` and replace with:

```vue
    <section>
      <h2 class="text-xs font-semibold text-slate-500 uppercase tracking-wider mb-3">Glass primitives (Phase ② new)</h2>
      <div class="grid grid-cols-3 gap-4">
        <GlassPanel class="p-4">
          <div class="text-[10px] uppercase tracking-widest text-slate-500 font-semibold">Subtle default</div>
          <div class="text-2xl font-light mt-1"><CountUp :to="1247" /></div>
        </GlassPanel>
        <GlassPanel variant="strong" hover="lift" class="p-4">
          <div class="text-[10px] uppercase tracking-widest text-slate-500 font-semibold">Strong + lift</div>
          <div class="text-2xl font-light mt-1 text-gradient-primary"><CountUp :to="42" /></div>
        </GlassPanel>
        <GlassPanel variant="subtle" class="p-4">
          <div class="text-[10px] uppercase tracking-widest text-slate-500 font-semibold">Subtle (nested-safe)</div>
          <div class="text-2xl font-light mt-1"><CountUp :to="98.4" :decimals="1" /></div>
        </GlassPanel>
      </div>
    </section>
```

Add `import { GlassPanel } from '@/components/ui/glass-panel'` and `import { CountUp } from '@/components/ui/count-up'` to the script block.

- [ ] **Step 2.6: Verify build**

```bash
cd frontend && npm run build
```
Expected: PASS. `@/components/ui/glass-panel` and `@/components/ui/count-up` resolve; no Vue compile errors.

- [ ] **Step 2.7: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/glass-panel/ frontend/components/ui/count-up/ frontend/pages/_style-demo.vue
git commit -m "feat(ui): add GlassPanel and CountUp primitives"
```

---

## Task 3 · Restyle Button

**File:**
- Modify: `frontend/components/ui/button/Button.vue`

Current state: default variant uses solid `bg-primary` (sky-500). Good, but lacks "wow" for CTAs. Secondary variant uses `bg-secondary` (now slate-100) which is too washed out. Success/destructive are fine.

- [ ] **Step 3.1: Rewrite the `cva` variants**

Replace the `variant` block of `buttonVariants` (the whole object literal in `variants: { variant: { ... } }`) with:

```ts
      variant: {
        default: 'text-white bg-gradient-to-r from-sky-500 to-violet-500 hover:brightness-110 shadow-md shadow-sky-500/25 hover:shadow-sky-500/40',
        destructive: 'text-white bg-gradient-to-r from-rose-500 to-red-500 hover:brightness-110 shadow-md shadow-rose-500/25',
        success: 'text-white bg-gradient-to-r from-emerald-500 to-teal-500 hover:brightness-110 shadow-md shadow-emerald-500/25',
        outline: 'border border-slate-200 bg-white/70 backdrop-blur-sm text-slate-700 hover:bg-white hover:border-slate-300 hover:text-slate-900',
        secondary: 'bg-white/60 border border-white/80 backdrop-blur-sm text-slate-700 hover:bg-white hover:text-slate-900',
        ghost: 'text-slate-600 hover:bg-slate-100 hover:text-slate-900',
        link: 'text-sky-600 underline-offset-4 hover:underline',
      },
```

Also update the base string (first arg to `cva(...)`). Change the ring class from `focus-visible:ring-ring` to `focus-visible:ring-sky-500/50`.

- [ ] **Step 3.2: Verify build**

```bash
cd frontend && npm run build
```
Expected: PASS.

- [ ] **Step 3.3: Refresh dev + eyeball demo page**

Dev server should HMR-reload automatically. Open or refresh `http://localhost:9999/_style-demo` in a browser — all 10 button variants should render correctly. Default + destructive + success should have gradients. Secondary/outline should be glass. Ghost/link should be text-only.

- [ ] **Step 3.4: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/button/Button.vue
git commit -m "feat(ui): gradient default + glass secondary button variants"
```

---

## Task 4 · Restyle Card

**File:**
- Modify: `frontend/components/ui/card/Card.vue`

Current state: uses `bg-card` (white), solid border, subtle shadow. Works but boring.

- [ ] **Step 4.1: Update the root card class**

Edit `frontend/components/ui/card/Card.vue`. Find the existing class string on the root `<div>` (probably something like `rounded-lg border bg-card text-card-foreground shadow-sm`). Replace with:

```
rounded-[14px] border border-white/85 bg-white/62 backdrop-blur-[18px] shadow-[0_4px_16px_rgba(100,140,200,0.08)] text-card-foreground transition-shadow duration-200 hover:shadow-[0_8px_28px_rgba(100,140,200,0.14)]
```

Other card sub-components (`CardHeader`, `CardContent`, `CardTitle`, `CardDescription`, `CardFooter`) should NOT be modified — they're structural only, no color responsibility.

- [ ] **Step 4.2: Verify build + visual**

```bash
cd frontend && npm run build
```
Refresh `/_style-demo` — 2 cards in the Cards section should float on the aurora background. Hover on `glass-hover-lift` card should rise 2px.

- [ ] **Step 4.3: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/card/Card.vue
git commit -m "feat(ui): card becomes floating glass surface"
```

---

## Task 5 · Restyle Badge (CRITICAL — fixes chip explosion)

**File:**
- Modify: `frontend/components/ui/badge/Badge.vue`

Current state: all variants use `/15` tinted bg. The `secondary` variant uses `bg-secondary` which is now slate-100 (fine), but all other variants use the raw semantic tint which is overpowering on a white surface.

- [ ] **Step 5.1: Rewrite `badgeVariants`**

Replace the entire `variants.variant` object in `frontend/components/ui/badge/Badge.vue` with:

```ts
      variant: {
        default: 'border-sky-200 bg-sky-50 text-sky-700',
        secondary: 'border-slate-200 bg-slate-50 text-slate-700',
        destructive: 'border-red-200 bg-red-50 text-red-700',
        outline: 'border-slate-200 bg-transparent text-slate-600',
        success: 'border-emerald-200 bg-emerald-50 text-emerald-700',
        warning: 'border-amber-200 bg-amber-50 text-amber-700',
        info: 'border-sky-200 bg-sky-50 text-sky-700',
      },
```

Also update the base class (first arg to `cva`). Change `rounded-sm` → `rounded-full` for a pill shape that matches the glass aesthetic, and keep the rest (`inline-flex`, padding, text size).

Final base string:
```
inline-flex items-center rounded-full border px-2 py-0.5 text-[10px] font-medium transition-colors
```

- [ ] **Step 5.2: Verify build + visual**

```bash
cd frontend && npm run build
```
Refresh `/_style-demo`. The 7 badges should read as soft tinted pills — no more bright purple/saturated colors. Check that:
- `default` and `info` look like sky pills (intentional overlap; they're semantically similar)
- `secondary` reads as neutral slate
- `destructive` is clearly red, but light
- `warning`/`success` are amber/emerald

- [ ] **Step 5.3: Spot-check on a real page**

Open `http://localhost:9999/accounts` in a browser. The region/mode/source chips should now be soft-tinted, NOT bright lavender. This is the regression the user saw at end of Phase ①.

- [ ] **Step 5.4: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/badge/Badge.vue
git commit -m "feat(ui): soft-tint badge variants with pill shape"
```

---

## Task 6 · Restyle Input + Textarea

**Files:**
- Modify: `frontend/components/ui/input/Input.vue`
- Modify: `frontend/components/ui/textarea/Textarea.vue`

Current state: uses `bg-background border-input`. Token change makes this white-on-white, barely visible.

- [ ] **Step 6.1: Update `Input.vue` class string**

Find the class string on the `<input>` element. Replace with:

```
flex h-8 w-full rounded-lg border border-slate-200 bg-white/70 backdrop-blur-sm px-3 py-2 text-xs text-slate-900 placeholder:text-slate-400 ring-offset-white file:border-0 file:bg-transparent file:text-sm file:font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sky-500/40 focus-visible:ring-offset-0 focus-visible:border-sky-400 focus-visible:bg-white transition-colors disabled:cursor-not-allowed disabled:opacity-50
```

- [ ] **Step 6.2: Update `Textarea.vue` class string**

Apply the same treatment to `Textarea.vue` root class. The `h-8` should become the default textarea height — remove `h-8` and leave height to the component consumer (or set `min-h-[80px]` if the current class has one).

Final textarea class:
```
flex min-h-[80px] w-full rounded-lg border border-slate-200 bg-white/70 backdrop-blur-sm px-3 py-2 text-xs text-slate-900 placeholder:text-slate-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sky-500/40 focus-visible:border-sky-400 focus-visible:bg-white transition-colors disabled:cursor-not-allowed disabled:opacity-50 resize-none
```

- [ ] **Step 6.3: Verify build + visual**

```bash
cd frontend && npm run build
```
Refresh `/_style-demo`. Input + Textarea in the Form inputs section should have subtle glass look, with sky ring on focus.

- [ ] **Step 6.4: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/input/Input.vue frontend/components/ui/textarea/Textarea.vue
git commit -m "feat(ui): glass-effect input and textarea with sky focus ring"
```

---

## Task 7 · Restyle Select

**Files:**
- Modify: `frontend/components/ui/select/SelectTrigger.vue`
- Modify: `frontend/components/ui/select/SelectContent.vue`
- Modify: `frontend/components/ui/select/SelectItem.vue`

- [ ] **Step 7.1: SelectTrigger — match Input**

Find the class string on SelectTrigger's root. Ensure it uses the same glass pattern as Input:
```
flex h-8 w-full items-center justify-between rounded-lg border border-slate-200 bg-white/70 backdrop-blur-sm px-3 py-2 text-xs text-slate-900 placeholder:text-slate-400 focus:outline-none focus:ring-2 focus:ring-sky-500/40 focus:border-sky-400 focus:bg-white transition-colors disabled:cursor-not-allowed disabled:opacity-50 [&>span]:line-clamp-1
```

- [ ] **Step 7.2: SelectContent — glass popover**

Find SelectContent's class string. Replace with:
```
relative z-50 max-h-96 min-w-[8rem] overflow-hidden rounded-xl border border-white/85 bg-white/85 backdrop-blur-xl shadow-[0_12px_40px_rgba(100,140,200,0.18)] text-slate-900 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95
```

Keep the position-related classes (side=... etc.) that may be appended — if the existing file has complex class composition, only replace the first (base) section and leave transforms intact.

- [ ] **Step 7.3: SelectItem — light hover + sky active**

Find SelectItem's class string. Replace with:
```
relative flex w-full cursor-default select-none items-center rounded-md py-1.5 pl-8 pr-2 text-xs text-slate-700 outline-none hover:bg-sky-50 hover:text-sky-700 focus:bg-sky-50 focus:text-sky-700 data-[state=checked]:bg-sky-100 data-[state=checked]:text-sky-700 data-[disabled]:pointer-events-none data-[disabled]:opacity-50
```

- [ ] **Step 7.4: Verify build + visual**

```bash
cd frontend && npm run build
```
Refresh `/_style-demo`. Click the Select — it should open a glass popover with 3 options, each hovering sky. Check keyboard nav (↑/↓ + Enter).

- [ ] **Step 7.5: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/select/
git commit -m "feat(ui): glass select trigger + glass content popover"
```

---

## Task 8 · Restyle Checkbox + Switch

**Files:**
- Modify: `frontend/components/ui/checkbox/Checkbox.vue`
- Modify: `frontend/components/ui/switch/Switch.vue`

- [ ] **Step 8.1: Checkbox — gradient on, glass off**

Find the Checkbox root class. Replace with (adjust if current has different structure):
```
peer h-4 w-4 shrink-0 rounded border border-slate-300 bg-white/70 backdrop-blur-sm ring-offset-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sky-500/40 focus-visible:ring-offset-0 disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:border-transparent data-[state=checked]:bg-gradient-to-br data-[state=checked]:from-sky-500 data-[state=checked]:to-violet-500 data-[state=checked]:text-white transition-colors
```

The CheckIcon inside should have `class="h-3 w-3"` (or its existing size).

- [ ] **Step 8.2: Switch — gradient on, glass off**

Find the Switch root class. Replace track class with:
```
peer inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sky-500/40 focus-visible:ring-offset-2 focus-visible:ring-offset-white disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-gradient-to-r data-[state=checked]:from-sky-500 data-[state=checked]:to-violet-500 data-[state=unchecked]:bg-slate-200
```

Thumb class — if separate, use:
```
pointer-events-none block h-4 w-4 rounded-full bg-white shadow-md ring-0 transition-transform data-[state=checked]:translate-x-4 data-[state=unchecked]:translate-x-0
```

- [ ] **Step 8.3: Verify build + visual**

```bash
cd frontend && npm run build
```
Refresh `/_style-demo`. Toggle both — the "on" state should have the sky→violet gradient, "off" is glass/slate.

- [ ] **Step 8.4: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/checkbox/Checkbox.vue frontend/components/ui/switch/Switch.vue
git commit -m "feat(ui): gradient-on / glass-off toggle states for checkbox and switch"
```

---

## Task 9 · Restyle Separator

**File:**
- Modify: `frontend/components/ui/separator/Separator.vue`

- [ ] **Step 9.1: Replace flat color with fading gradient**

Find the class string. If the separator currently uses `bg-border` / `bg-slate-200`, replace with:
```
shrink-0 bg-gradient-to-r from-transparent via-slate-200 to-transparent data-[orientation=horizontal]:h-px data-[orientation=horizontal]:w-full data-[orientation=vertical]:h-full data-[orientation=vertical]:w-px data-[orientation=vertical]:bg-gradient-to-b data-[orientation=vertical]:from-transparent data-[orientation=vertical]:via-slate-200 data-[orientation=vertical]:to-transparent
```

- [ ] **Step 9.2: Verify build + visual**

Refresh demo. The separator in the Feedback section should show as a faded line (darker in middle, fading at edges).

- [ ] **Step 9.3: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/separator/Separator.vue
git commit -m "feat(ui): fading gradient separator replaces flat line"
```

---

## Task 10 · Restyle Dialog + Popover

**Files:**
- Modify: `frontend/components/ui/dialog/DialogContent.vue`
- Modify: `frontend/components/ui/popover/PopoverContent.vue`

- [ ] **Step 10.1: DialogContent — glass + aurora-subtle backdrop**

Find DialogOverlay class (line ~19 per Phase 1.5 state). Confirm it reads `bg-slate-900/40` — that's fine, leave it.

Find DialogContent class. Replace the surface/size portion with:
```
fixed left-[50%] top-[50%] z-50 grid w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 rounded-[20px] border border-white/85 bg-white/90 backdrop-blur-2xl p-6 shadow-[0_20px_60px_rgba(100,140,200,0.25)] duration-200 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[state=closed]:slide-out-to-left-1/2 data-[state=closed]:slide-out-to-top-[48%] data-[state=open]:slide-in-from-left-1/2 data-[state=open]:slide-in-from-top-[48%]
```

- [ ] **Step 10.2: PopoverContent — glass popover**

Apply similar glass pattern to PopoverContent:
```
z-50 w-72 rounded-xl border border-white/85 bg-white/85 backdrop-blur-xl p-4 text-slate-900 shadow-[0_12px_40px_rgba(100,140,200,0.18)] outline-none data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2
```

- [ ] **Step 10.3: Verify build + visual**

```bash
cd frontend && npm run build
```
Demo page doesn't render dialog/popover by default — but if you trigger the Select (Task 7) dropdown, you'll see the PopoverContent pattern. For Dialog, briefly add a Button + Dialog trigger to the demo page temporarily or just visually inspect one of the real pages that opens a dialog.

- [ ] **Step 10.4: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/dialog/DialogContent.vue frontend/components/ui/popover/PopoverContent.vue
git commit -m "feat(ui): glass surfaces for dialog and popover content"
```

---

## Task 11 · Restyle Tooltip (inverted — dark glass)

**File:**
- Modify: `frontend/components/ui/tooltip/TooltipContent.vue`

Tooltips deliberately go dark — they sit on top of any surface and need to be universally legible. This is the ONE component that stays dark in Phase ②.

- [ ] **Step 11.1: Update TooltipContent class**

Replace the class string with:
```
z-50 overflow-hidden rounded-md bg-slate-900/90 backdrop-blur-md px-2.5 py-1 text-[11px] text-white shadow-lg animate-in fade-in-0 zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2
```

- [ ] **Step 11.2: Verify build**

```bash
cd frontend && npm run build
```

- [ ] **Step 11.3: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/tooltip/TooltipContent.vue
git commit -m "feat(ui): dark-glass tooltip for universal legibility"
```

---

## Task 12 · Restyle Sonner + Skeleton

**Files:**
- Modify: `frontend/components/ui/sonner/Sonner.vue`
- Modify: `frontend/components/ui/skeleton/Skeleton.vue`

- [ ] **Step 12.1: Sonner toasts**

Find the `toastOptions` prop (or equivalent) in `Sonner.vue`. If it passes a `style` object / classNames, update the `toast` class to:
```
group toast flex gap-3 items-center rounded-xl border border-white/85 bg-white/90 backdrop-blur-xl p-4 shadow-[0_8px_32px_rgba(100,140,200,0.18)] text-slate-900 data-[type=success]:border-emerald-200 data-[type=error]:border-red-200 data-[type=warning]:border-amber-200
```
Exact DOM structure varies with the Sonner library version — adapt whatever wrapping strategy the component currently uses.

- [ ] **Step 12.2: Skeleton shimmer**

Replace Skeleton class string with:
```
animate-pulse rounded-md bg-gradient-to-r from-slate-100 via-white to-slate-100 bg-[length:200%_100%]
```

Optionally add a custom shimmer keyframe. For Phase ② we'll use Tailwind's built-in `animate-pulse` for simplicity — if it looks bland, we can add a custom shimmer in a later polish.

- [ ] **Step 12.3: Verify build + visual**

```bash
cd frontend && npm run build
```
Refresh demo. Skeleton section should show 2 soft gradient bars pulsing. Sonner is only visible when a toast fires — trigger one by going to `/settings` and saving (or test via an admin action).

- [ ] **Step 12.4: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/sonner/Sonner.vue frontend/components/ui/skeleton/Skeleton.vue
git commit -m "feat(ui): glass toasts and gradient skeleton shimmer"
```

---

## Task 13 · Light-touch polish: Avatar, Scroll-area, Collapsible

**Files:**
- Modify: `frontend/components/ui/avatar/Avatar.vue`
- Modify: `frontend/components/ui/scroll-area/ScrollAreaRoot.vue` (if separate; might just be `ScrollArea.vue`)
- Modify: `frontend/components/ui/scroll-area/ScrollAreaScrollbar.vue` (if separate)

- [ ] **Step 13.1: Avatar — gradient fallback**

Find Avatar root class. If it uses `bg-muted` for the fallback, update the fallback variant to use gradient:
```
relative flex h-8 w-8 shrink-0 overflow-hidden rounded-full bg-gradient-to-br from-sky-400 to-violet-500 text-white ring-1 ring-white/80
```

The `fallback` letter inside should have `text-[11px] font-semibold`.

- [ ] **Step 13.2: Scroll-area thumb**

Find the scrollbar thumb class. Usually something like `bg-border`. Replace with:
```
relative flex-1 rounded-full bg-slate-300/70 hover:bg-slate-400/80 transition-colors
```

- [ ] **Step 13.3: Collapsible**

Inspect `frontend/components/ui/collapsible/`. It's likely just a Radix wrapper with no visual style of its own. If you find any hardcoded colors, replace with tokens; otherwise leave it alone.

- [ ] **Step 13.4: Verify build + visual**

```bash
cd frontend && npm run build
```
Demo page Avatar should show "AB" in white on sky→violet gradient. Scroll-area only manifests when content overflows — test by scrolling on the demo page itself.

- [ ] **Step 13.5: Commit**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add frontend/components/ui/avatar/ frontend/components/ui/scroll-area/ frontend/components/ui/collapsible/
git commit -m "polish(ui): gradient avatar, soft scrollbar, collapsible cleanup"
```

---

## Task 14 · Final verify — build, lint, tests, after-screenshots

**Files:**
- No code change.

- [ ] **Step 14.1: Clean build**

```bash
cd /Users/ychchen/warren_ws/opsagent/frontend && rm -rf .nuxt && npm run build
```
Expected: PASS.

- [ ] **Step 14.2: Lint + unit tests**

```bash
cd frontend && npm run lint && npm run test
```
Expected: both PASS (zero new errors, existing warnings OK).

- [ ] **Step 14.3: Restart dev server for clean CSS**

```bash
pkill -9 -f 'nuxt dev' 2>/dev/null; sleep 2
cd frontend && rm -rf .nuxt node_modules/.vite
PORT=9999 nohup npm run dev > /tmp/ops-9999.log 2>&1 &
sleep 25
curl -sI http://localhost:9999/_style-demo | head -1
```
Expected: `HTTP/1.1 200 OK`.

- [ ] **Step 14.4: Capture after screenshots**

```bash
cd /Users/ychchen/warren_ws/opsagent/e2e
PHASE2_LABEL=after npx playwright test phase2-components.spec.ts --project=chromium
```
Expected: `e2e/screenshots/after-phase2/style-demo.png` exists.

Also re-run the full audit to verify real pages (accounts especially — badge regression check):
```bash
rm -rf screenshots/audit
npx playwright test full-audit.spec.ts --project=chromium
```
Expected: all 20 routes captured, 0 errors (the Vite HMR aborts are allowed per Phase 1.5 findings).

- [ ] **Step 14.5: Visual diff review**

Open `screenshots/baseline-phase2/style-demo.png` vs `screenshots/after-phase2/style-demo.png` side by side. Expected:
- Buttons: default is now a gradient, no longer flat blue
- Badges: all 7 variants are soft pills, NOT bright
- Cards: float with subtle glass + shadow
- Inputs/Selects: glass surface + sky ring on focus
- Checkbox/Switch: gradient when on
- Separator: faded line
- Skeletons: soft gradient
- GlassPanel + CountUp: new section at the bottom renders

Also inspect `screenshots/audit/accounts.png` — the chips that were purple-exploding at end of Phase ① should now be soft slate pills.

- [ ] **Step 14.6: Kill dev server**

```bash
pkill -9 -f 'nuxt dev' || true
```

Actually, DON'T kill it — user may want to keep browsing. Only kill if explicitly asked.

- [ ] **Step 14.7: Commit screenshots**

```bash
cd /Users/ychchen/warren_ws/opsagent
git add e2e/screenshots/baseline-phase2/ e2e/screenshots/after-phase2/
git commit -m "test(e2e): phase 2 before/after component snapshots"
```

---

## Phase ② Completion Criteria

All must be true before declaring Phase ② done:

1. ✅ All 14 tasks have every checkbox ticked
2. ✅ `cd frontend && npm run build` exits 0
3. ✅ `cd frontend && npm run lint` exits 0
4. ✅ `cd frontend && npm run test` exits 0
5. ✅ `frontend/components/ui/` contains two new component dirs: `glass-panel/` and `count-up/`, each with a `.vue` and `index.ts`
6. ✅ `grep -rn "bg-primary/15\|bg-secondary text-secondary-foreground" frontend/components/ui/` returns 0 matches (old chip pattern gone)
7. ✅ Visual diff review completed — demo page + accounts page both show new light theme
8. ✅ Git log shows ~14 atomic Conventional Commits, no pushes
9. ✅ `frontend/package.json` / `frontend/package-lock.json` vitest WIP still uncommitted (user's responsibility)

Once all pass, Phase ③ (Layout — Floating Glass Islands) plan can be written.

---

## What Phase ② does NOT do

Deliberately deferred to later phases, do NOT attempt in Phase ②:

- ❌ Touching `layouts/default.vue` or any layout file (Phase ③)
- ❌ Creating `AuroraBackground.vue` or `CursorGlow.vue` layout components (Phase ③)
- ❌ Rewriting any `pages/*.vue` file beyond the temporary demo page (Phase ④ / ⑤)
- ❌ Modifying `tailwind.config.ts` or `assets/css/tailwind.css` (Phase ① is done)
- ❌ Adding new npm dependencies (CountUp is a ~40-line hand-rolled animation; no library needed)
- ❌ Deleting `frontend/pages/_style-demo.vue` — keep it as a perma-reference for Phase ③/④ contributors (it's prefixed with `_` so Nuxt may or may not route it publicly; leave that to runtime check)

**End of Phase ② plan.**
