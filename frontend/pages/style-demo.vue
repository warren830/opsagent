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
import { GlassPanel } from '@/components/ui/glass-panel'
import { CountUp } from '@/components/ui/count-up'

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
  </div>
</template>
