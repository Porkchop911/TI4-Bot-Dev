import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openChoice, prefix, primaryChoice } from "./scene";

// Phone (390x844, touch): paying on the map. Before the panel covered the map and the pay bar was a
// tall card over a third of it with a disabled Confirm. After: the compact bar leaves the map
// tappable, shows what is selected and lets the player assign the tokens and confirm from it.
test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
test("leadership primary: phone, pay on the map", async ({ page }, testInfo) => {
  await openChoice(page, primaryChoice());
  if (prefix === "before") {
    await page.getByTestId("token-buy-plus").tap();
    await shot(page, testInfo, `${prefix}-7-phone-bought`);
    await page.getByTestId("token-pay-on-map").tap();
    await page.getByTestId("token-payment-bar").waitFor();
    await shot(page, testInfo, `${prefix}-7-phone-pay-on-map`);
    return;
  }
  await shot(page, testInfo, `${prefix}-7-phone-bought`);
  await page.getByTestId("token-pay-on-map").tap();
  const bar = page.getByTestId("token-payment-bar");
  await bar.waitFor();
  await shot(page, testInfo, `${prefix}-7-phone-pay-on-map-empty`);
  // The map is still tappable: planets that can pay respond to a tap, which is hit-tested first.
  const planet = page.getByTestId("planet-dal_bootha");
  const pbox = (await planet.boundingBox())!;
  const barBox = (await bar.boundingBox())!;
  expect(pbox.y + pbox.height / 2).toBeLessThan(barBox.y);
  await planet.tap();
  await page.getByTestId("planet-rarron").tap();
  await expect(page.getByTestId("token-bar-bought")).toHaveText("1");
  for (let i = 0; i < 4; i += 1) await page.getByTestId("token-bar-pool-tactic").tap();
  const confirm = page.getByTestId("token-bar-confirm");
  await expect(confirm).toBeEnabled();
  const box = (await confirm.boundingBox())!;
  expect(box.y + box.height).toBeLessThanOrEqual(page.viewportSize()!.height);
  expect(box.height).toBeGreaterThanOrEqual(44);
  await shot(page, testInfo, `${prefix}-7-phone-pay-on-map`);
});
