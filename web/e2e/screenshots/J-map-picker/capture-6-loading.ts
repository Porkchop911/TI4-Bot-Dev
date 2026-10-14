import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// loading state: the catalog and the preview have not answered
test("map picker: loading state", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 6, catalog: "loading", preview: "loading" });
  await page.getByTestId("lobby-map-button").click();
  await expect(page.getByTestId("map-picker-loading")).toBeVisible();
  await expect(page.getByText("Loading the map…")).toBeVisible();
  await shot(page, testInfo, "6-loading");
});
