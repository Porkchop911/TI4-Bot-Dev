import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";
import { cabalProduction, prefix } from "./scene";

// The Cabal holds captured carriers: the engine offers the paid build AND the Amalgamation exchange
// of the same type in one produce_unit decision. Before, both read as a bare "carrier" row.
const game = { players: [playerWithHand({ faction: "cabal" }), opponent], choice: cabalProduction };

test(`${prefix} production builder with the exchange row, desktop`, async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await openMockedGame(page, game);
  await page.getByTestId("production-builder-drawer").waitFor();
  await page.waitForTimeout(300);
  await shot(page, testInfo, `${prefix}-production-desktop`);
  if (prefix === "before") return;
  await page.getByTestId("produce-unit-btn-exchange|carrier").click();
  await page.getByTestId("produce-unit-btn-build|fighter|1").click();
  await expect(page.getByTestId("production-resources-counter")).toContainText("1 / 6");
  await expect(page.getByTestId("production-capacity-counter")).toContainText("2 / 3");
  await shot(page, testInfo, `${prefix}-production-staged-desktop`);
});

test.describe("phone", () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  test(`${prefix} production builder with the exchange row, phone`, async ({ page }, testInfo) => {
    await openMockedGame(page, game);
    await page.getByTestId("production-builder-drawer").waitFor();
    await page.waitForTimeout(300);
    await page.getByTestId("produce-option-build|carrier|1").scrollIntoViewIfNeeded();
    await shot(page, testInfo, `${prefix}-production-phone`);
  });
});
