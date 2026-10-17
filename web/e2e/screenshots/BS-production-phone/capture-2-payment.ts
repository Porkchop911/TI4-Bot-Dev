import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { assertOneColumn, buttonInViewport, confirmBuilds, isPhone, newPage, openProduction, prefix, SIZES, stageAndConfirm, waitForPanel } from "./scene";

// The second dialog: three cruisers (2 each) paid with one payment. After: the planet rows are one
// column on phones, the total repeats the builder's, and Pay is reachable.
for (const size of SIZES) {
  for (const single of [true, false]) {
    if (!single && size.label !== "390x844" && size.label !== "desktop") continue;
    test(`${prefix} payment ${size.label} ${single ? "single" : "ask-each"}`, async ({ browser, baseURL }, testInfo) => {
      const { context, page } = await newPage(browser, size, baseURL);
      await openProduction(page, { single });
      await stageAndConfirm(page, 3);
      if (prefix === "after") {
        await expect(page.getByTestId("production-total-sum")).toHaveText("Total: 6 resources — 3 builds");
        await expect(page.getByRole("button", { name: "Confirm builds" })).toHaveText("Build (6)");
      }
      await shot(page, testInfo, `${prefix}-2-builder-staged-${size.label}${single ? "" : "-ask-each"}`);
      await confirmBuilds(page);
      if (single) await waitForPanel(page);
      else {
        await page.getByTestId("payment-drawer").waitFor();
        await page.getByTestId("auto-pay-btn").click();
        await expect(page.getByTestId("confirm-payment-btn")).toBeEnabled();
      }
      if (prefix === "after") {
        if (single) {
          // The second dialog repeats the first dialog's total.
          await expect(page.getByTestId("payment-total-line")).toHaveText("Total: 6 resources — 3 builds");
        } else await expect(page.getByTestId("payment-total-line")).toHaveCount(0);
        if (isPhone(size)) {
          await assertOneColumn(page, 'label[data-testid^="planet-card-"]', 3);
          const rows = await page.locator('label[data-testid^="planet-card-"]').evaluateAll((els) =>
            els.map((el) => Math.round(el.getBoundingClientRect().height)),
          );
          expect(Math.min(...rows), "planet rows are at least 44 px").toBeGreaterThanOrEqual(44);
          // The last row, scrolled into view, is not covered by the sticky footer.
          const last = page.locator('label[data-testid^="planet-card-"]').last();
          await last.scrollIntoViewIfNeeded();
          const covered = await last.evaluate((el) => {
            const r = el.getBoundingClientRect();
            const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
            return !el.contains(top);
          });
          expect(covered, "last planet row covered").toBe(false);
          await page.getByTestId("confirm-payment-btn").scrollIntoViewIfNeeded();
          expect(await buttonInViewport(page, "Pay"), "Pay reachable").toBe(true);
        }
      }
      await page.getByTestId("payment-drawer").evaluate((el) => el.scrollTo?.(0, 0));
      await shot(page, testInfo, `${prefix}-3-payment-${size.label}${single ? "" : "-ask-each"}`);
      await context.close();
    });
  }
}

// Builds already sent: the total on the builder is marked "so far" (it covers the new builds only).
test(`${prefix} builder total so far, 390x844`, async ({ browser, baseURL }, testInfo) => {
  test.skip(prefix === "before", "new");
  const { context, page } = await newPage(browser, SIZES[0], baseURL);
  const { ledger } = await openProduction(page, { single: true });
  await stageAndConfirm(page, 2);
  await confirmBuilds(page);
  await waitForPanel(page);
  await page.getByTestId("confirm-payment-btn").click();
  await page.getByTestId("production-builder-drawer").waitFor();
  await expect.poll(() => ledger.batches.filter((b) => b.kind === "payment").length).toBe(2);
  await expect(page.getByTestId("production-capacity-counter")).toContainText("2 / 6");
  await page.getByTestId("produce-unit-btn-build|cruiser|1").click();
  await expect(page.getByTestId("production-total-sum")).toHaveText("Total so far: 2 resources — 1 build");
  await shot(page, testInfo, "after-4-so-far-390x844");
  await context.close();
});
