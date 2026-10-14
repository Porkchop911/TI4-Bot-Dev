import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { productionGame } from "./fixtures";

// Two resources only: what cannot be added says why, at rest, instead of a bare greyed-out plus.
test("produce units: unaffordable options", async ({ page }, testInfo) => {
  await openMockedGame(page, productionGame("sol", ["dn2", "ac2"], 2));
  await page.getByTestId("production-builder-drawer").waitFor();
  await shot(page, testInfo, "4-unaffordable");
});

// Staged units use up resources and capacity; the options that no longer fit say so.
test("produce units: staged builds", async ({ page }, testInfo) => {
  await openMockedGame(page, productionGame("sol", ["dn2", "ac2"], 9));
  await page.getByTestId("production-builder-drawer").waitFor();
  await page.getByRole("button", { name: "Add 1x dreadnought for 4" }).click();
  await page.getByRole("button", { name: "Add 1x carrier for 3" }).click();
  await shot(page, testInfo, "4b-staged");
});
