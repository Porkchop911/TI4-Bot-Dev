import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { BUILDS, openScene, prefix, pushChoice, warfareScript, windowChoice } from "./scene";
import { engineProduction } from "../../../src/test/secondaryPrepFixtures";

// Arrival (Review mode): the real production question opens. After: the real builder shows the
// prepared builds already staged and the bar says what Confirm does; one click sends them the way
// the builder's own "Confirm builds" does. Before: nothing was prepared beyond following, so the
// builder opens empty.
test("secondary prep: Warfare production on arrival", async ({ page }, testInfo) => {
  const game = await openScene(page, { name: "Warfare", card: "pok6warfare", script: prefix === "before" ? null : warfareScript });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  if (prefix !== "before") {
    await page.getByTestId("production-builder-drawer").waitFor();
    await page.getByTestId("produce-unit-btn-build|infantry|2").click();
    await page.getByTestId("produce-unit-btn-build|carrier|1").click();
    await page.getByRole("button", { name: "Save these builds" }).click();
    // The first build's payment, as the engine would ask for it: Jord.
    await page.getByTestId("payment-drawer").waitFor();
    await page.getByTestId("planet-card-exhaust|jord").click();
    await page.getByTestId("confirm-payment-btn").click();
  }
  await page.getByTestId("secondary-prep-chip").waitFor();
  // The window opens (nothing is sent in this mocked server), then the production question.
  pushChoice(game, 42, windowChoice("pok6warfare"));
  await page.getByTestId("secondary-prepared-bar").waitFor();
  pushChoice(game, 43, engineProduction(BUILDS));
  await page.getByTestId("production-builder-drawer").waitFor();
  await page.waitForTimeout(300);
  // The build list is longer than the screen: bring the staged ground forces into view with the ships.
  await page.getByTestId("produce-option-build|infantry|2").scrollIntoViewIfNeeded();
  await page.waitForTimeout(150);
  await shot(page, testInfo, `${prefix}-3-warfare-arrival`);
});
