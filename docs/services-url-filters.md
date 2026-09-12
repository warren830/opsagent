# Services page URL filters

The Services grid (`/services`) keeps its filter state in the route query, so a
refresh, a bookmark or a pasted link reproduces the exact same view. Nothing is
stored in component memory only.

## Query parameters

| Param       | Values                                                      | Default    |
|-------------|-------------------------------------------------------------|------------|
| `q`         | free text (matches name, display name, description, tags)   | *(empty)*  |
| `system`    | `all` · `none` (no system) · a system uuid                  | `all`      |
| `lifecycle` | `all` · `production` · `experimental` · `deprecated` · `retired` | `all`  |
| `runtime`   | `all` · `eks` · `ec2` · `rds` · `lambda` · `external` · `generic` | `all` |
| `health`    | `all` · `critical` · `warning` · `unknown` · `healthy`      | `all`      |
| `sort`      | `health` · `name` · `incidents`                             | `health`   |

Example — critical Lambda services in the payments system, sorted by incident
count:

```
/services?system=8f1c…&runtime=lambda&health=critical&sort=incidents
```

Filters intersect (logical AND), and the per-system group counts always reflect
the cards actually visible.

## Behaviour worth knowing

- **Defaults are omitted.** A pristine view has a clean URL; selecting a filter
  and then setting it back to its default removes the param again.
- **Unsupported values degrade silently.** A missing key, a valueless key
  (`?health`), a repeated key (`?health=critical&health=warning`, which arrives
  as an array) or a typo (`?sort=owner`) all fall back to the documented
  default. Values are matched case-sensitively; `?runtime=EKS` is not accepted.
  An invalid URL is rewritten to its canonical form on load.
- **`system` survives loading.** The id is data-dependent, so it cannot be
  validated until the overview payload arrives. Until then the selection is kept
  as-is; once the systems are known, an id that no longer exists resets to `all`.
- **Search does not flood history.** Typing is debounced (300 ms) and written
  with `replace`, so it collapses into the current history entry. Picking a value
  from a dropdown uses `push`, so browser back/forward walks the filter history
  and updates both the controls and the grid.
- **Other params and the hash are preserved.** Anything the page does not own
  (`?tab=`, `?ref=`, …) and the `#fragment` are carried through untouched.

## Code layout

| File | Role |
|------|------|
| `frontend/composables/serviceFilterQuery.ts` | Pure filter ⇄ query codec: parsing, validation, default stripping, merging. No Vue imports. |
| `frontend/composables/useServiceFilterQuery.ts` | Binds that codec to `vue-router`: restore on load, write on change, react to back/forward. |
| `frontend/components/services/ServiceCardGrid.vue` | Consumes `filters` / `setFilters` and feeds the existing filter → sort → group pipeline. |
| `frontend/components/services/cardRegistry.ts` | Unchanged filtering/sorting/grouping helpers. |

Reusing the filters elsewhere (for example to build a deep link in an alert
notification) only needs the pure module:

```ts
import { DEFAULT_SERVICE_FILTERS, serviceFiltersToQuery } from '@/composables/serviceFilterQuery'

const query = serviceFiltersToQuery({ ...DEFAULT_SERVICE_FILTERS, health: 'critical' })
// → { health: 'critical' }
```

The URL carries view state only. It never widens the data request: the page
still calls `/api/services/overview` (or its client-side fallback) with the
caller's own credentials, and all filtering happens on the records the backend
already authorised.
