import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, open, type Scenario } from "./scene";

// "Who can go here" tab of the system overview: the engine's answer split into the viewer's own
// ships and everybody else's, with arrows on the map (green dashed = yours, red dotted = others').
const SCENARIOS: Scenario[] = ["own", "enemy", "mixed", "empty"];

for (const [name, size] of Object.entries(SIZES)) {
  for (const scenario of SCENARIOS) {
    test(`reach ${scenario} ${name}`, async ({ page }, testInfo) => {
      const asked = await open(page, size, scenario);
      // Nothing is asked until the tab is opened.
      expect(asked).toEqual([]);
      await page.getByTestId("inspector-tab-reach").click();
      await expect(page.getByTestId("reach-assumption")).toContainText("activated now");
      await expect(page.getByTestId("reach-legend")).toBeVisible();
      if (scenario === "empty") await expect(page.getByTestId("reach-empty")).toBeVisible();
      else await expect(page.getByTestId("reach-tab")).toBeVisible();
      expect(asked).toEqual(["18"]);
      const own = scenario === "own" || scenario === "mixed" ? 1 : 0;
      const enemy = scenario === "enemy" ? 2 : scenario === "mixed" ? 2 : 0;
      await expect(page.getByTestId("reach-vector-own")).toHaveCount(own);
      await expect(page.getByTestId("reach-vector-enemy")).toHaveCount(enemy);
      if (own) await expect(page.getByTestId("reach-own")).toContainText("You");
      if (enemy) {
        await expect(page.getByTestId("reach-enemy")).toContainText("Hacan Fox");
        await expect(page.getByTestId("reach-enemy")).toContainText("Letnev Lee");
      }
      if (name === "390x844") {
        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
        for (const tab of ["details", "reach"])
          expect((await page.getByTestId(`inspector-tab-${tab}`).boundingBox())!.height).toBeGreaterThanOrEqual(44);
      }
      await shot(page, testInfo, `1-${scenario}-${name}`);
      if (scenario === "mixed") {
        // On a phone the open turn menu covers the lower part of the panel (as it does for the
        // plain overview); minimise it to reach the entries.
        if (name === "390x844") await page.getByTestId("turn-bar-minimize").click();
        // Picking one entry fades the other arrows.
        await page.getByTestId("reach-entry").nth(1).click();
        await expect(page.getByTestId("reach-entry").nth(1)).toHaveAttribute("aria-pressed", "true");
        await shot(page, testInfo, `2-picked-${name}`);
        if (name === "390x844") {
          // Fold the panel to see the whole map with the arrows on a phone.
          await page.getByTestId("system-inspector-fold").click();
          await shot(page, testInfo, `3-folded-${name}`);
        }
      }
    });
  }

  test(`units per owner ${name}`, async ({ page }, testInfo) => {
    await open(page, size, "mixed");
    const blocks = page.locator('[data-testid^="inspector-owner-"]:not([data-testid*="structures"])');
    await expect(blocks).toHaveCount(3);
    // The viewer first, then the table order, each with its own ships and ground forces.
    await expect(blocks.nth(0)).toContainText("You");
    await expect(blocks.nth(1)).toContainText("Hacan Fox");
    await expect(blocks.nth(2)).toContainText("Letnev Lee");
    await expect(blocks.nth(0)).toContainText("2 × Infantry");
    await expect(blocks.nth(1)).not.toContainText("Dreadnought");
    await page.getByTestId("inspector-owner-other_seat").scrollIntoViewIfNeeded();
    await shot(page, testInfo, `4-units-per-owner-${name}`);
  });
}
