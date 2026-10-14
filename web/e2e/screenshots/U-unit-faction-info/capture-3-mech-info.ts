import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { productionGame } from "./fixtures";

// The viewing seat's mech, named above the build options, with its printed ability.
test("unit info: mech", async ({ page }, testInfo) => {
  await openMockedGame(page, productionGame());
  await page.getByTestId("mech-info-row").waitFor();
  await hoverShot(page, testInfo, "4-mech-info", page.getByTestId("mech-info"), {
    tooltip: page.getByTestId("mech-info-card"),
  });
});
