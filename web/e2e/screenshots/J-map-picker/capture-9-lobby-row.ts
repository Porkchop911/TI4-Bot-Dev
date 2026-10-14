import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// the lobby row shows the chosen map before Start
test("map picker: the lobby row shows the chosen map before Start", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 6, alias: "6pHyperlanes" });
  const row = page.getByTestId("lobby-map-row");
  await expect(row).toContainText("6pHyperlanes");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await shot(page, testInfo, "9-lobby-row");
});
