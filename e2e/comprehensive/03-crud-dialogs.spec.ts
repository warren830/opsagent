/**
 * CRUD dialog rendering tests — open the "new / add / create" dialog on
 * each resource page, confirm it has an input, then close it.
 *
 * NO submits — nothing is written to the database.
 *
 * Run: E2E_BASE_URL=http://localhost:9999 npx playwright test \
 *      comprehensive/03-crud-dialogs.spec.ts --project=chromium
 */
import { test, expect } from '@playwright/test';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import {
  LOCAL_BASE,
  setupAuthed,
  waitForHydration,
} from './helpers-local';

// Each resource page + the localized label of the button that opens the
// "new" dialog. See frontend/i18n/zh.json for the canonical strings.
interface CrudCase {
  name: string;
  path: string;
  /** Regex matching the Chinese or English "add" button text. */
  buttonText: RegExp;
}

const CASES: CrudCase[] = [
  { name: 'Accounts',  path: '/accounts',  buttonText: /添加账号|添加|新建|Add account|New/i },
  { name: 'Clusters',  path: '/clusters',  buttonText: /添加集群|添加|新建|Add cluster|New/i },
  { name: 'Channels',  path: '/channels',  buttonText: /添加集成|添加|新建|Add|New/i },
  { name: 'Providers', path: '/providers', buttonText: /添加模型|添加|新建|Add|New/i },
  { name: 'Glossary',  path: '/glossary',  buttonText: /添加术语|添加|新建|Add term|New/i },
  { name: 'Knowledge', path: '/knowledge', buttonText: /新建文件|新建|添加|New file|Add/i },
  { name: 'Users',     path: '/users',     buttonText: /邀请用户|创建用户|添加|新建|Invite|Create user|New/i },
  { name: 'Tenants',   path: '/tenants',   buttonText: /创建租户|添加|新建|Create tenant|New/i },
];

let sharedContext: BrowserContext;

test.describe.configure({ mode: 'serial' });

test.describe('CRUD dialog rendering', () => {
  test.beforeAll(async ({ browser }: { browser: Browser }) => {
    const setup = await setupAuthed(browser);
    sharedContext = setup.context;
    await setup.page.close();
  });

  test.afterAll(async () => {
    await sharedContext?.close();
  });

  for (const c of CASES) {
    test(`${c.name}: open + close create dialog`, async () => {
      const page: Page = await sharedContext.newPage();
      try {
        await page.goto(`${LOCAL_BASE}${c.path}`, { waitUntil: 'domcontentloaded' });
        await waitForHydration(page);

        // Find the "add / new" button. Strategy order:
        //   1) By visible text matching the localized label pattern.
        //   2) Fall back to any <button> containing a lucide `Plus` icon.
        let addBtn = page.locator('button').filter({ hasText: c.buttonText }).first();
        if (!(await addBtn.isVisible({ timeout: 3_000 }).catch(() => false))) {
          // Fallback: lucide-vue-next renders <svg class="lucide-plus"> —
          // match the wrapping <button> via :has().
          addBtn = page.locator('button:has(svg.lucide-plus)').first();
        }

        // If we still can't find it, the page may be locked down (e.g.
        // Users page only shows the invite/create button to super_admin).
        // We're logged in as admin (super_admin) so this should succeed,
        // but handle gracefully regardless.
        const hasButton = await addBtn.isVisible({ timeout: 5_000 }).catch(() => false);
        expect(hasButton, `${c.name}: no "add" button found on ${c.path}`).toBeTruthy();

        await addBtn.click();

        // Dialog should appear.
        const dialog = page.locator('[role="dialog"]').first();
        await expect(dialog, `${c.name}: dialog did not open`).toBeVisible({ timeout: 5_000 });

        // At least one input / textarea / select trigger inside the dialog.
        const inputs = dialog.locator('input, textarea, [role="combobox"], [role="textbox"]');
        const n = await inputs.count();
        expect(n, `${c.name}: dialog has no input fields`).toBeGreaterThan(0);

        // Screenshot the open dialog for human review.
        await page.screenshot({
          path: `screenshots/comprehensive/dialog-${c.name.toLowerCase()}.png`,
          fullPage: false,
        });

        // Close the dialog — prefer the Cancel button, fall back to the
        // DialogClose (top-right X), then Escape.
        const cancelBtn = dialog
          .locator('button')
          .filter({ hasText: /取消|关闭|Cancel|Close/i })
          .first();

        if (await cancelBtn.isVisible({ timeout: 1_500 }).catch(() => false)) {
          await cancelBtn.click();
        } else {
          // The radix-vue DialogClose (X in top-right) doesn't have a
          // visible text label; it's an icon-only button with class
          // `absolute right-4 top-4`. Fall back to Escape if it's awkward.
          const closeX = dialog.locator('button.absolute.right-4.top-4').first();
          if (await closeX.isVisible({ timeout: 1_500 }).catch(() => false)) {
            await closeX.click();
          } else {
            await page.keyboard.press('Escape');
          }
        }

        // Dialog should be dismissed.
        await expect(dialog, `${c.name}: dialog did not close`).toBeHidden({ timeout: 5_000 });
      } finally {
        await page.close();
      }
    });
  }
});
