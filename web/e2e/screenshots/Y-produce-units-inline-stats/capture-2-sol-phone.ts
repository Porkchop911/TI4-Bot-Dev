import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { productionGame } from "./fixtures";

test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

test("produce units: Sol, phone width", async ({ page }, testInfo) => {
  await openMockedGame(page, productionGame("sol", ["dn2", "ac2"]));
  const drawer = page.getByTestId("production-builder-drawer");
  await drawer.waitFor();
  await shot(page, testInfo, "2-sol-phone");
  // Nothing in the dialog reaches past the right edge of the screen (so nothing scrolls sideways).
  const outside = await page.evaluate(() => {
    const root = document.querySelector('[data-testid="production-builder-drawer"]')!;
    return [root, ...Array.from(root.querySelectorAll("*"))]
      .filter((el) => el.getBoundingClientRect().right > window.innerWidth + 1)
      .map((el) => `${el.tagName}.${el.className}`.slice(0, 80));
  });
  if (outside.length) throw new Error(`past the right edge: ${outside.join(", ")}`);
  await page.getByTestId("production-group-ground").scrollIntoViewIfNeeded();
  await page.getByTestId("produce-option-build|mech|1").scrollIntoViewIfNeeded();
  await shot(page, testInfo, "2b-sol-phone-lower");
});
