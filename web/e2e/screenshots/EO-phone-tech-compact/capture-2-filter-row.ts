import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openResearch, prefix } from "./scene";

// Phone: the colour / type filter chips are one row that scrolls sideways and keeps the pressed chip in view.
test("filter row scrolled, last chip pressed 390x844", async ({ page }, testInfo) => {
  await openResearch(page, "390x844", { card: "primary", planets: false, tradeGoods: 3 });
  const row = page.getByTestId("tech-filter");
  const last = page.getByTestId("tech-filter-available");
  if (prefix !== "before") {
    const d = await row.evaluate((el) => ({ sw: el.scrollWidth, cw: el.clientWidth, h: el.getBoundingClientRect().height }));
    expect(d.sw).toBeGreaterThan(d.cw); // it scrolls instead of wrapping
    expect(d.h).toBeLessThan(60); // one row of 44 px chips
    expect(await row.evaluate((el) => getComputedStyle(el).scrollbarWidth)).toBe("none");
  }
  await last.scrollIntoViewIfNeeded();
  await last.click();
  await page.waitForTimeout(200);
  if (prefix !== "before") await expect(last).toBeInViewport({ ratio: 1 });
  await shot(page, testInfo, `${prefix}-filter-scrolled-390x844`);
});

test("pressed chip is scrolled into view on its own 390x844", async ({ page }, testInfo) => {
  test.skip(prefix === "before", "after only");
  await openResearch(page, "390x844", { card: "primary", planets: false, tradeGoods: 3 });
  await page.getByTestId("tech-filter").evaluate((el) => (el.scrollLeft = 0));
  // A click that bypasses scrolling (as a restored or programmatic filter would): the chip must still come into view.
  await page.getByTestId("tech-filter-available").dispatchEvent("click");
  await page.waitForTimeout(200);
  await expect(page.getByTestId("tech-filter-available")).toBeInViewport({ ratio: 1 });
  await shot(page, testInfo, `${prefix}-filter-selected-visible-390x844`);
});
