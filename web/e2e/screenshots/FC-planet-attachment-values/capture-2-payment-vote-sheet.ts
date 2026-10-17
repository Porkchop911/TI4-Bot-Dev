import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { open, payChoice, voteChoice, SIZES } from "./scene";

// Paying: each planet row shows resources and influence, the boosted ones marked like on the map.
for (const s of SIZES) {
  test(`payment drawer ${s.label}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: s.width, height: s.height });
    await open(page, { choice: payChoice });
    const drawer = page.getByTestId("payment-drawer");
    await drawer.waitFor();
    await expect(drawer.locator('[data-boosted="true"]').first()).toBeVisible();
    await shot(page, testInfo, `4-payment-${s.label}`, s.label === "desktop" ? { of: drawer, pad: 8 } : {});
  });
}

test("payment bar and map", async ({ page }, testInfo) => {
  await open(page, { choice: payChoice });
  await page.getByTestId("payment-drawer").waitFor();
  await page.getByTestId("pick-on-map-btn").click();
  await page.getByTestId("payment-bar").waitFor();
  await shot(page, testInfo, "5-payment-map-desktop");
});

// Voting: exhaust a planet for its influence. Quann's 2 votes include Dyson Sphere's +1.
for (const s of SIZES) {
  test(`vote exhaust list ${s.label}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: s.width, height: s.height });
    await open(page, { phase: "agenda", round: 3, choice: voteChoice });
    const modal = page.getByTestId("agenda-ballot-modal");
    await modal.waitFor();
    await modal.getByTestId("planet-card-quann").waitFor();
    await expect(modal.locator('[data-boosted="true"]').first()).toBeVisible();
    await shot(page, testInfo, `6-vote-${s.label}`, s.label === "desktop" ? { of: modal, pad: 8 } : {});
  });
}

// The player sheet's ready / total resources and influence count the raised values.
test("player sheet totals", async ({ page }, testInfo) => {
  await open(page);
  await shot(page, testInfo, "7-player-stats-desktop", { of: page.getByTestId("player-stats-row").first(), pad: 10 });
});
