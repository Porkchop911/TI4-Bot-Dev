import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, type Page } from "@playwright/test";
import type { MapPreview, MapTemplateSummary } from "../../../../src/protocol/types";
import { openMapLobby } from "../_shared/mapLobby";

const here = dirname(fileURLToPath(import.meta.url));

/** The real server's catalog and previews for `players` seats, snapshotted by fetch-real.mjs. */
export function realMaps(players: number): {
  templates: MapTemplateSummary[];
  previews: Record<string, MapPreview>;
} {
  return JSON.parse(readFileSync(join(here, "server-snapshot", `${players}p.json`), "utf8"));
}

/** Opens the host's picker with `alias` chosen, its card scrolled into view and its board drawn. */
export async function openPickerOn(page: Page, players: number, alias: string, name: string) {
  const real = realMaps(players);
  expect(real.templates.find((t) => t.alias === alias)?.name).toBe(name);
  await openMapLobby(page, { players, alias, real });
  await page.getByTestId("lobby-map-button").click();
  await page.getByRole("dialog").waitFor();
  const card = page.getByTestId(`map-card-${alias}`);
  await card.scrollIntoViewIfNeeded();
  await expect(card).toContainText(name);
  await expect(page.getByTestId("map-picker-summary")).toContainText(name);
  await expect(page.getByTestId("map-preview-board")).toBeVisible();
  return card;
}
