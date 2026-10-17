import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test, expect, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { galleryPlayers } from "../_shared/fixtures";
import type { BoardView, BoardTileView, MapPreviewDto } from "../../../src/protocol/types";

const here = dirname(fileURLToPath(import.meta.url));
const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

/** The real Learn to Play (PoK) layout on the game board. */
function ltpBoard(): BoardView {
  const preview = JSON.parse(
    readFileSync(join(here, "../CO-community-maps/server-snapshot/5p.json"), "utf8"),
  ).previews["5pLearnToPlayPoK"] as MapPreviewDto;
  const tiles = preview.tiles.filter((t) => !t.special_area);
  return { active_system: null, map_tiles: tiles as unknown as BoardTileView[], systems: {} } as BoardView;
}

async function open(page: Page, size: { width: number; height: number }) {
  await page.setViewportSize(size);
  await openMockedGame(page, { board: ltpBoard(), players: galleryPlayers, choice: null });
  await page.getByTestId("system-hex-18").waitFor();
}

for (const [name, size] of Object.entries(SIZES)) {
  test(`game board ${name}`, async ({ page }, testInfo) => {
    await open(page, size);
    await shot(page, testInfo, `8-board-${name}`);
  });
}

