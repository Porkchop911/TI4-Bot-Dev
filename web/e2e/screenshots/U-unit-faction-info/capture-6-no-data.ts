import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { shot } from "../_shared/shot";
import { productionGame, unknownUnit, solSeat, opponent } from "./fixtures";

// Content the catalog does not know: a plain "no information" card, never an invented one.
test("unit info: no data", async ({ page }, testInfo) => {
  await openMockedGame(page, productionGame([unknownUnit]));
  await page.getByTestId("production-builder-drawer").waitFor();
  await hoverShot(page, testInfo, "8-no-data-unit", page.getByTestId("unit-info-mystery_unit"), {
    tooltip: page.getByTestId("unit-info-mystery_unit-card"),
  });
});

test("faction info: no data", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [solSeat(), { ...opponent, faction: "homebrew_faction" }] });
  await page.getByTestId("faction-info-button").nth(1).click();
  await page.getByTestId("faction-info-empty").waitFor();
  await shot(page, testInfo, "9-no-data-faction");
});
