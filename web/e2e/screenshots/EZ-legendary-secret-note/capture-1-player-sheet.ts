import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openScene, SIZES, type SizeName } from "./scene";

// Seize an Icon in the private hand: with no legendary planet on the map the card says why it is
// stuck; with one on the map it shows only its count.
for (const size of Object.keys(SIZES) as SizeName[]) {
  for (const withLegendary of [false, true]) {
    const name = `${withLegendary ? "2-with" : "1-without"}-legendary-${size}`;
    test(`player sheet ${name}`, async ({ page }, testInfo) => {
      await openScene(page, size, withLegendary);
      if (size !== "desktop") {
        await page.getByTestId("player-sheet-toggle").click();
        await page.waitForTimeout(400);
      }
      const item = page.getByTestId("secret-objective-item-sai");
      await expect(item.getByTestId("secret-progress-sai")).toContainText("0 / 1");
      const note = item.getByTestId("secret-unreachable-sai");
      if (withLegendary) await expect(note).toHaveCount(0);
      else await expect(note).toContainText("No legendary planet is on this map");
      await expect(page.getByTestId("secret-unreachable-ctr")).toHaveCount(0);
      await item.evaluate((el) => el.scrollIntoView({ block: "center" }));
      await page.waitForTimeout(200);
      if (size !== "desktop") {
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        expect(overflow).toBeLessThanOrEqual(0);
      }
      const target = item.locator("xpath=../..");
      await shot(page, testInfo, name, { of: target, pad: 12 });
    });
  }
}
