import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";
import { realMaps } from "./real";

// the lobby row names a community map by its display name
test("community maps: lobby row", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 6, alias: "6pScpt2021Final", real: realMaps(6) });
  const row = page.getByTestId("lobby-map-row");
  await expect(row).toContainText("SCPT 2021 Final");
  await expect(row).not.toContainText("6pScpt2021Final");
  await shot(page, testInfo, "6-lobby-row");
});
