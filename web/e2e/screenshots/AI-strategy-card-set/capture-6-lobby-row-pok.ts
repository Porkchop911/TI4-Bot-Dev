import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// The same lobby row for a Prophecy of Kings table (also what a game saved before the option shows).
test("strategy card set: the lobby row, Prophecy of Kings", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 6, alias: "6pStandard", strategyCardSet: "pok" });
  await expect(page.getByTestId("lobby-card-set-row")).toContainText("Prophecy of Kings");
  await shot(page, testInfo, "6-lobby-row-pok");
});
