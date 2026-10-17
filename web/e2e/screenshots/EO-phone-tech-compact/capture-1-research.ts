import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openResearch, prefix, SIZES, type SizeName, type Variant } from "./scene";

// The Select Research picker on phones. `before` runs (TI4_SHOT_PREFIX=before, on the commit without
// the change) only take the pictures; the after run also asserts the compact layout.
test.describe.configure({ retries: 2 });

const cases: Array<{ key: string; v: Variant }> = [
  { key: "free-skips", v: { card: "primary", planets: true, tradeGoods: 0 } },
  { key: "free-noskips", v: { card: "primary", planets: false, tradeGoods: 3 } },
  { key: "paid-skips", v: { card: "secondary", planets: true, tradeGoods: 2 } },
  { key: "paid-noskips", v: { card: "secondary", planets: false, tradeGoods: 5 } },
];

for (const size of Object.keys(SIZES) as SizeName[]) {
  const phone = SIZES[size].width < 720;
  for (const { key, v } of cases) {
    if (!phone && key !== "free-skips" && key !== "paid-noskips") continue;
    test(`${key} ${size}`, async ({ page }, testInfo) => {
      await openResearch(page, size, v);
      const box = async (id: string) => (await page.getByTestId(id).boundingBox())!;
      if (key.endsWith("-skips")) console.log(`${key}: skip pills = ${await page.locator('[data-testid^="tech-skip-"]').count()}`);
      if (prefix !== "before") {
        const roster = page.getByTestId("tech-modal-roster");
        const effect = page.getByTestId("tech-effect-amd");
        if (phone) {
          await expect(roster).toBeHidden();
          await expect(effect).toBeVisible(); // EX-tech-picker-text-space: the technology text is back (clamped)
          // Available and Selected share a line, the button follows, then the filter row.
          const avail = await box("tech-available-resources");
          const btn = await box("confirm-research-btn");
          const filter = await box("tech-filter");
          expect(btn.y - (avail.y + avail.height)).toBeLessThan(60);
          expect(filter.y).toBeGreaterThanOrEqual(btn.y + btn.height - 1);
          expect(filter.y + filter.height).toBeLessThan(SIZES[size].height * 0.5);
          const sameLine = await page.getByTestId("tech-available-resources").evaluate((el) => {
            const next = el.nextElementSibling as HTMLElement;
            return Math.abs(next.getBoundingClientRect().top - el.getBoundingClientRect().top) < 12;
          });
          expect(sameLine).toBe(true);
        } else {
          await expect(roster).toBeVisible();
          await expect(effect).toBeVisible();
        }
      }
      await shot(page, testInfo, `${prefix}-${key}-${size}`);
    });
  }
}
