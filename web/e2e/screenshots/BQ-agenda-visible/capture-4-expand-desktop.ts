import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { newDevicePage, prefix, substateOptions } from "./scene";

test.skip(prefix === "before", "the strip is new");

// Tapping the strip opens the printed text under it; the dialog starts below the opened strip, so
// the vote controls are not covered.
test("after expanded strip over the vote dialog", async ({ browser, baseURL }, testInfo) => {
  const { context, page } = await newDevicePage(browser, "mobile", baseURL);
  await openMockedGame(page, substateOptions("vote-cast"));
  await page.getByTestId("agenda-ballot-modal").waitFor();
  await page.getByTestId("agenda-strip-toggle").click();
  const details = page.getByTestId("agenda-strip-details");
  await expect(details).toContainText("For: The elected player");
  await expect(page.getByTestId("agenda-strip-tally")).toContainText("FOR 3 · AGAINST 1");
  await shot(page, testInfo, "after-expanded-390x844");
  const stripBox = (await page.getByTestId("agenda-strip").boundingBox())!;
  const stripBottom = stripBox.y + stripBox.height;
  for (const id of ["vote-outcome-opt-FOR", "vote-outcome-opt-AGAINST", "abstain-vote-btn", "close-agenda-modal"]) {
    const box = (await page.getByTestId(id).boundingBox())!;
    expect(box.y, `${id} starts below the opened strip`).toBeGreaterThanOrEqual(stripBottom - 1);
    const covered = await page.evaluate(
      ([x, y, testId]) => {
        const target = document.querySelector(`[data-testid="${testId}"]`)!;
        const top = document.elementFromPoint(x as number, y as number);
        return !(top === target || target.contains(top));
      },
      [box.x + box.width / 2, box.y + box.height / 2, id],
    );
    expect(covered, `${id} is covered`).toBe(false);
  }
  await context.close();
});

// A phone asking for the desktop site, ?layout=compact on a desktop window, and the plain desktop.
for (const [name, device, compact] of [
  ["desktop-site", "desktop-site", false],
  ["layout-compact-desktop", "desktop", true],
  ["desktop", "desktop", false],
] as const) {
  test(`after ${name} vote`, async ({ browser, baseURL }, testInfo) => {
    const { context, page } = await newDevicePage(browser, device, baseURL);
    if (compact) await page.addInitScript(() => window.localStorage.setItem("ti4.layout.v1", "compact"));
    await openMockedGame(page, substateOptions("vote-planets"));
    await page.getByTestId("agenda-ballot-modal").waitFor();
    await expect(page.getByTestId("agenda-strip-text")).toHaveText("Agenda 1/2: Minister of Peace · For/Against · Your vote");
    const uncovered = await page.evaluate(() => {
      const el = document.querySelector("[data-testid=agenda-strip-text]")!;
      const r = el.getBoundingClientRect();
      const top = document.elementFromPoint(r.left + 20, r.top + r.height / 2);
      return top === el || el.contains(top);
    });
    expect(uncovered).toBe(true);
    await shot(page, testInfo, `after-${name}-vote`);
    await context.close();
  });
}
