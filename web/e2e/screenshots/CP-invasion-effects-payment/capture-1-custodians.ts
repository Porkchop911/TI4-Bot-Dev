import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { custodiansOffer, custodiansPayment, openInvasionChoice } from "./invasion";

// First landing on Mecatol Rex: the custodians offer, then the six influence paid with the
// payment panel (both values beside every planet, running total against the cost), not with
// a row of bare "exhaust" buttons inside the invasion card.

test("custodians offer inside the invasion", async ({ page }, testInfo) => {
  await openInvasionChoice(page, "committing", custodiansOffer);
  await expect(page.getByTestId("invasion-effect-card")).toContainText("remove it for a victory point");
  await shot(page, testInfo, "1-custodians-offer-desktop");
});

test("custodians payment list with both values", async ({ page }, testInfo) => {
  await openInvasionChoice(page, "committing", custodiansPayment);
  const drawer = page.getByTestId("payment-drawer");
  await drawer.waitFor();
  await expect(page.getByTestId("invasion-effect-card")).toHaveCount(0);
  // Wellon: 1 resource, 2 influence; Torkan: 0 resources, 3 influence.
  await expect(page.getByTestId("planet-card-exhaust|wellon-values")).toHaveText(/1\s*2/);
  await expect(page.getByTestId("planet-card-exhaust|torkan-values")).toHaveText(/0\s*3/);
  await expect(page.getByTestId("confirm-payment-btn")).toContainText("6");
  await shot(page, testInfo, "2-custodians-payment-desktop", { of: drawer, pad: 8 });

  // Staged: the running total reaches the cost.
  await page.getByTestId("auto-pay-btn").click();
  await expect(page.getByTestId("confirm-payment-btn")).toBeEnabled();
  await shot(page, testInfo, "3-custodians-payment-staged-desktop", { of: drawer, pad: 8 });
});

test("custodians payment on the map", async ({ page }, testInfo) => {
  await openInvasionChoice(page, "committing", custodiansPayment);
  await page.getByTestId("payment-drawer").waitFor();
  await page.getByTestId("pick-on-map-btn").click();
  await page.getByTestId("payment-bar").waitFor();
  await expect(page.getByTestId("payment-mark-torkan")).toHaveAttribute("aria-label", /3 influence/);
  await shot(page, testInfo, "4-custodians-map-desktop");
});

test("custodians payment on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openInvasionChoice(page, "committing", custodiansPayment);
  await page.getByTestId("payment-drawer").waitFor();
  await expect(page.getByTestId("planet-card-exhaust|centauri-values")).toHaveText(/1\s*3/);
  const scroll = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(scroll).toBeLessThanOrEqual(0);
  await shot(page, testInfo, "5-custodians-payment-390x844");
});
