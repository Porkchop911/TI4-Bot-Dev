import { expect, test, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";

/** `before` is taken on the commit without the compact header (layout assertions are skipped then). */
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const SIZES = {
  "390x844": { width: 390, height: 844 },
  "360x740": { width: 360, height: 740 },
  "844x390": { width: 844, height: 390 },
  "1440x900": { width: 1440, height: 900 },
} as const;
type Size = keyof typeof SIZES;

const OBJECTIVES = ["amass_wealth", "become_legend", "brain_trust", "build_defenses", "centralize_trade", "command_armada", "conquer", "corner"];

async function open(page: Page, size: Size, opts: { imperial: boolean; count: number }) {
  await page.setViewportSize(SIZES[size]);
  const base = galleryDecision("Imperial: draw a secret");
  const objectives = OBJECTIVES.slice(0, opts.count).map((id) => ({ id, label: id, kind: "objective" }));
  const decline = base.options.find((o) => o.id === "decline")!;
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: {
      prompt: base.prompt,
      context: opts.imperial ? (base.context as Record<string, unknown>) : { subtype: "score_objective" },
      options: [...objectives, decline],
      ...(opts.imperial && base.details ? { details: base.details } : {}),
    },
  });
  await page.getByTestId("objectives-modal").waitFor();
  await page.waitForTimeout(300);
}

async function assertLayout(page: Page, size: Size, minRows: number) {
  if (prefix === "before") return;
  const vp = SIZES[size];
  const phone = size !== "1440x900";
  const panel = (await page.locator(".objectives-modal-panel").boundingBox())!;
  const header = (await page.locator(".objectives-modal__header").boundingBox())!;
  const footer = (await page.getByTestId("objectives-scoring-footer").boundingBox())!;
  const horizontalOverflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
  expect(horizontalOverflow).toBe(false);
  expect(panel.x + panel.width).toBeLessThanOrEqual(vp.width + 0.5);
  // Footer (Selected line, Decline, Confirm) stays inside the viewport.
  expect(footer.y + footer.height).toBeLessThanOrEqual(vp.height + 0.5);
  const confirm = (await page.getByTestId("submit-choice-button").boundingBox())!;
  expect(confirm.y + confirm.height).toBeLessThanOrEqual(vp.height + 0.5);
  expect(confirm.x + confirm.width).toBeLessThanOrEqual(vp.width + 0.5);
  if (phone) {
    expect(header.height).toBeLessThanOrEqual(52);
    await expect(page.getByTestId("minimize-choice-button")).toBeVisible();
    const min = (await page.getByTestId("minimize-choice-button").boundingBox())!;
    expect(Math.abs(min.y + min.height / 2 - (header.y + header.height / 2))).toBeLessThan(4);
    // Rows fully visible between the header and the footer.
    const rows = page.locator('[data-testid^="objective-row-"]');
    const n = await rows.count();
    let visible = 0;
    for (let i = 0; i < n; i++) {
      const b = (await rows.nth(i).boundingBox())!;
      if (b.y >= header.y + header.height - 1 && b.y + b.height <= footer.y + 1) visible++;
    }
    expect(visible).toBeGreaterThanOrEqual(Math.min(minRows, n));
  }
}

for (const size of ["390x844", "360x740", "844x390"] as const) {
  for (const imperial of [true, false]) {
    for (const count of [3, 8]) {
      // Landscape phones are short: ask for 2 rows there, 3 on portrait phones.
      const minRows = size === "844x390" ? 2 : 3;
      test(`score public objective ${size} ${imperial ? "imperial" : "plain"} ${count} rows`, async ({ page }, testInfo) => {
        // Every size with Imperial (3 and 8 rows); plain only with 8 rows on portrait phones.
        if (!imperial && !(count === 8 && size !== "844x390")) test.skip(true, "covered by the other variants");
        await open(page, size, { imperial, count });
        await shot(page, testInfo, `${prefix}-${imperial ? "imperial" : "plain"}-${count}rows-${size}`);
        await assertLayout(page, size, minRows);
      });
    }
  }
}

test("imperial details expand on tap 390x844", async ({ page }, testInfo) => {
  test.skip(prefix === "before", "new");
  await open(page, "390x844", { imperial: true, count: 8 });
  const toggle = page.getByTestId("imperial-outcome-toggle");
  await expect(toggle).toBeVisible();
  await expect(page.getByTestId("imperial-outcome-details")).toBeHidden();
  await toggle.click();
  await expect(page.getByTestId("imperial-outcome-details")).toContainText("Scoring an objective is optional");
  await shot(page, testInfo, `${prefix}-imperial-expanded-390x844`);
});

test("desktop unchanged 1440x900", async ({ page }, testInfo) => {
  await open(page, "1440x900", { imperial: true, count: 8 });
  await assertLayout(page, "1440x900", 3);
  if (prefix === "after") {
    await expect(page.locator(".objectives-modal__subtitle")).toBeVisible();
    await expect(page.locator(".objectives-modal__scoring-badge")).toBeVisible();
    await expect(page.getByTestId("imperial-outcome-details")).toBeVisible();
  }
  await shot(page, testInfo, `${prefix}-imperial-8rows-1440x900`);
});
