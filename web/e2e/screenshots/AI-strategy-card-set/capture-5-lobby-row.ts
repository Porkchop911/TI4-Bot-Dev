import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// Once created, the lobby names the table's set next to the map row.
test("strategy card set: the lobby row", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 6, alias: "6pStandard", strategyCardSet: "te" });
  await expect(page.getByTestId("lobby-card-set-row")).toContainText("Thunder's Edge");
  await shot(page, testInfo, "5-lobby-row");
});
