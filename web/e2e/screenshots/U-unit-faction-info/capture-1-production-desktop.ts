import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { productionGame } from "./fixtures";

// Hovering the info mark next to a build option: Sol owns Dreadnought II, so the card is the II.
test("unit info: production builder, desktop", async ({ page }, testInfo) => {
  await openMockedGame(page, productionGame());
  await page.getByTestId("production-builder-drawer").waitFor();
  const card = page.getByTestId("unit-info-dreadnought-card");
  await hoverShot(page, testInfo, "1-production-dreadnought", page.getByTestId("unit-info-dreadnought"), { tooltip: card });
  // Faction-specific flagship text for the viewing seat.
  await page.mouse.move(0, 0);
  await hoverShot(page, testInfo, "2-production-flagship", page.getByTestId("unit-info-flagship"), {
    tooltip: page.getByTestId("unit-info-flagship-card"),
  });
});
