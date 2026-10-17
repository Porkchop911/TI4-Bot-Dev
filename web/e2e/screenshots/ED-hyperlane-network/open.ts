import { expect, type Page } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { realMaps } from "../CO-community-maps/real";

/** Like CO's openPickerOn, for templates the server names only by alias (e.g. "Hyperlanes"). */
export async function openPicker(page: Page, players: number, alias: string) {
  const real = realMaps(players);
  expect(real.templates.some((t) => t.alias === alias)).toBe(true);
  await openMapLobby(page, { players, alias, real });
  await page.getByTestId("lobby-map-button").click();
  await page.getByRole("dialog").waitFor();
  const card = page.getByTestId(`map-card-${alias}`);
  await card.scrollIntoViewIfNeeded();
  await expect(page.getByTestId("map-preview-board")).toBeVisible();
  return card;
}
