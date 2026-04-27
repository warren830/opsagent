# Comprehensive Playwright Suite

End-to-end functional tests for the Ops web app. Targets the **local** dev
server by default (`http://localhost:9999`, Nuxt 3 SSR, backend on `:3080`).

Four spec files covering:

| File | Focus |
|------|------|
| `01-smoke.spec.ts` | Every route loads (22 auth routes + `/login` + 3 callback routes) — no console errors, body has text, screenshot captured. |
| `02-auth-nav.spec.ts` | Login success/failure, session persistence, logout, middleware guard, sidebar navigation, chat fullscreen toggle. |
| `03-crud-dialogs.spec.ts` | Open + close the "new" dialog for 8 resource pages. **No DB writes.** |
| `04-feature-interactions.spec.ts` | Topology (Vue Flow), Issues list, Deployments list + promote/rollback, chat textarea typing, Skills detail, Style demo. |

Shared helpers live in `helpers-local.ts` (do **not** use the repo-level
`e2e/helpers.ts`, which points at CloudFront).

## Prerequisites

1. Backend running: `cargo run` (from `backend/`) on `:3080`
2. Frontend dev server: `npm run dev` (from `frontend/`) on `:9999`
3. Database seeded with the `admin` user (password `admin123`)
4. Playwright browsers installed: `cd e2e && npx playwright install`

## Run

```bash
cd e2e
E2E_BASE_URL=http://localhost:9999 npx playwright test comprehensive/ --project=chromium
```

### Single file

```bash
E2E_BASE_URL=http://localhost:9999 npx playwright test comprehensive/01-smoke.spec.ts --project=chromium
```

### Headed mode (watch it in a real browser)

```bash
E2E_BASE_URL=http://localhost:9999 npx playwright test comprehensive/ --project=chromium --headed
```

### View HTML report

```bash
npx playwright show-report
```

## Screenshots

Screenshots are written to `e2e/screenshots/comprehensive/`:

- `smoke-<route>.png` — one per route
- `dialog-<resource>.png` — CRUD dialog opened
- `skills-detail.png` — skills detail (if present)

## Notes

- Tests log in **once per test suite** via `POST /api/auth/login`. The
  returned HttpOnly cookies are installed on the `BrowserContext`, so
  every page is pre-authenticated without going through the UI form.
- Tests run `serial` within each describe to amortize the login cost.
- No writes: CRUD specs only open and cancel the dialog. Safe to re-run.
- `chromium` is the primary target. The config also ships firefox, webkit,
  and mobile-chrome profiles — try `--project=firefox` for a cross-browser
  pass if something looks suspicious.
