import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openChoice, prefix, primaryChoice } from "./scene";

// A phone with "Request desktop site": the desktop layout (about 980 css px wide) on a touch screen.
// Before: the bar's Confirm stayed disabled ("Assign 4 more tokens") and the pools could only be
// assigned in the panel, so nothing on the map screen let the player continue.
// After: pick planets on the map, assign the tokens from the bar, Confirm is there and enabled.
test.use({ viewport: { width: 980, height: 760 }, hasTouch: true });
test("leadership primary: desktop site on a phone, pay on the map", async ({ page }, testInfo) => {
  await openChoice(page, primaryChoice());
  if (prefix === "before") {
    await page.getByTestId("token-buy-plus").tap();
    await page.getByTestId("token-pay-on-map").tap();
    await page.getByTestId("token-payment-bar").waitFor();
    await shot(page, testInfo, `${prefix}-6-desktop-site-pay-on-map`);
    return;
  }
  await page.getByTestId("token-pay-on-map").tap();
  await page.getByTestId("token-payment-bar").waitFor();
  await page.getByTestId("planet-dal_bootha").tap();
  await page.getByTestId("planet-rarron").tap();
  await expect(page.getByTestId("token-bar-bought")).toHaveText("1");
  for (let i = 0; i < 4; i += 1) await page.getByTestId("token-bar-pool-tactic").tap();
  const confirm = page.getByTestId("token-bar-confirm");
  await expect(confirm).toBeEnabled();
  // Regression: the confirm control is on screen and not covered by anything else.
  const box = (await confirm.boundingBox())!;
  const viewport = page.viewportSize()!;
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
  expect(box.height).toBeGreaterThanOrEqual(44);
  const hit = await page.evaluate(
    ({ x, y }) => document.elementFromPoint(x, y)?.closest("[data-testid]")?.getAttribute("data-testid"),
    { x: box.x + box.width / 2, y: box.y + box.height / 2 },
  );
  expect(hit).toBe("token-bar-confirm");
  await shot(page, testInfo, `${prefix}-6-desktop-site-pay-on-map`);
});
