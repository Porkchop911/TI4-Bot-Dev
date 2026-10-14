import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { productionGame } from "../Y-produce-units-inline-stats/fixtures";
import { assertOneColumn, buttonInViewport, inViewport, isPhone, newPage, prefix, SIZES } from "./scene";

// The first dialog (the builder) with 2 cruisers + 1 carrier staged: 7 of 9 resources. After: a
// total in the sticky footer (visible without scrolling on every phone size), the cost on the confirm
// button, and the unit cards in one column on phones.
for (const size of SIZES) {
  for (const single of [true, false]) {
    if (!single && size.label !== "390x844" && size.label !== "desktop") continue;
    test(`${prefix} builder total ${size.label} ${single ? "single" : "ask-each"}`, async ({ browser, baseURL }, testInfo) => {
      const { context, page } = await newPage(browser, size, baseURL);
      await page.addInitScript((value) => {
        try {
          localStorage.setItem("player_single_production_payment", value);
        } catch {
          // unavailable: the default applies
        }
      }, single ? "true" : "false");
      await openMockedGame(page, productionGame("sol", [], 9));
      await page.getByTestId("production-builder-drawer").waitFor();
      await page.getByTestId("produce-unit-btn-build|cruiser|1").click();
      await page.getByTestId("produce-unit-btn-build|cruiser|1").click();
      await page.getByTestId("produce-unit-btn-build|carrier|1").click();
      await expect(page.getByTestId("produce-count-build|cruiser|1")).toHaveText("2");
      if (prefix === "after") {
        await expect(page.getByTestId("production-total-sum")).toHaveText("Total: 7 resources — 3 builds");
        await expect(page.getByTestId("production-total-available")).toHaveText("You have 9 available · 2 left");
        await expect(page.getByRole("button", { name: "Confirm builds" })).toHaveText("Build (7)");
        await expect(page.getByTestId("production-total")).toContainText(single ? "You pay once" : "own payment");
        // The total and the confirm button are on screen without scrolling the dialog.
        expect(await inViewport(page, "production-total"), "total in viewport").toBe(true);
        expect(await buttonInViewport(page, "Confirm builds"), "confirm in viewport").toBe(true);
        if (isPhone(size)) {
          await assertOneColumn(page, '[data-testid^="produce-option-"]', 5);
          // 44 px targets for the +/- buttons.
          const small = await page.locator(".production-drawer__unit .button--icon").evaluateAll((els) =>
            els.filter((el) => el.getBoundingClientRect().height < 43.5).length,
          );
          expect(small, "+/- buttons under 44 px").toBe(0);
        }
        // Nothing reaches past the right edge.
        const outside = await page.evaluate(() =>
          Array.from(document.querySelectorAll('[data-testid="production-builder-drawer"] *')).filter(
            (el) => el.getBoundingClientRect().right > window.innerWidth + 1,
          ).length,
        );
        expect(outside, "elements past the right edge").toBe(0);
      }
      await shot(page, testInfo, `${prefix}-1-builder-${size.label}${single ? "" : "-ask-each"}`);
      await context.close();
    });
  }
}
