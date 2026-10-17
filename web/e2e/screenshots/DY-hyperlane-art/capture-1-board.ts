import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test, expect, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { galleryPlayers, actor } from "../_shared/fixtures";
import type { BoardView, BoardTileView, MapPreviewDto, PlacedUnitView } from "../../../src/protocol/types";

const here = dirname(fileURLToPath(import.meta.url));
const unit = (unit_type: string, extra: Partial<PlacedUnitView> = {}): PlacedUnitView => ({
  owner: actor, unit_type, damaged: false, ...extra,
});

const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

const boardOf = (tiles: Partial<BoardTileView>[], systems: BoardView["systems"] = {}): BoardView =>
  ({ active_system: null, map_tiles: tiles as BoardTileView[], systems }) as BoardView;

/** The real Learn to Play (PoK) layout: hyperlanes beside gravity rift, nebula, supernova, asteroids. */
function ltpBoard(): BoardView {
  const preview = JSON.parse(
    readFileSync(join(here, "../CO-community-maps/server-snapshot/5p.json"), "utf8"),
  ).previews["5pLearnToPlayPoK"] as MapPreviewDto;
  const tiles = preview.tiles.filter((t) => !t.special_area);
  expect(tiles.filter((t) => t.hyperlane).length).toBeGreaterThanOrEqual(6);
  return boardOf(tiles as Partial<BoardTileView>[], {
    "18": { system_id: "18", command_tokens: [], planets: {}, units: [unit("infantry", { planet: "mr" }), unit("infantry", { planet: "mr" })] },
    "41": { system_id: "41", command_tokens: [actor], planets: {}, units: [unit("destroyer"), unit("fighter")] },
    "68": { system_id: "68", command_tokens: [], planets: {}, units: [unit("carrier"), unit("fighter"), unit("fighter")] },
    "75": { system_id: "75", command_tokens: [], planets: {}, units: [unit("cruiser")] },
  });
}

const FACES = [83, 84, 85, 86, 87, 88, 89, 90, 91].flatMap((n) => [`${n}a`, `${n}b`]);
/** `per` hexes per row, rows offset so that they line up as a rectangle. */
const grid = (ids: string[], per: number): Partial<BoardTileView>[] =>
  ids.map((system_id, i) => {
    const r = Math.floor(i / per);
    return { system_id, label: "Hyperlane", hyperlane: true, q: (i % per) - Math.floor(r / 2), r, planets: [] };
  });

async function open(page: Page, board: BoardView, size: { width: number; height: number }, wait = "system-hex-18") {
  await page.setViewportSize(size);
  await openMockedGame(page, { board, players: galleryPlayers, choice: null });
  await page.getByTestId(wait).waitFor();
}

const zoom = async (page: Page, title: "Zoom In" | "Zoom Out", n: number) => {
  // The phone has no zoom buttons (pinch only); its default zoom already fits the map.
  const btn = page.getByTitle(title).first();
  if (!(await btn.isVisible().catch(() => false))) return;
  for (let i = 0; i < n; i++) await btn.click();
};

test("every hyperlane tile face", async ({ page }, testInfo) => {
  for (const [name, size] of Object.entries(SIZES)) {
    await open(page, boardOf(grid(FACES, 6)), size, "system-hex-83a");
    await expect(page.getByTestId("hyperlane-network")).toHaveCount(1);
    await shot(page, testInfo, `1-faces-${name}`);
  }
});

test("turned copies of three tiles", async ({ page }, testInfo) => {
  const turned = ["83b", "88a", "90a"].flatMap((f) => [f, ...[60, 120, 180, 240, 300].map((d) => `${f}${d}`)]);
  await open(page, boardOf(grid(turned, 6)), SIZES.desktop, "system-hex-83b");
  await shot(page, testInfo, "2-turned-desktop");
});

for (const [name, size] of Object.entries(SIZES)) {
  test(`full map ${name}`, async ({ page }, testInfo) => {
    await open(page, ltpBoard(), size);
    await shot(page, testInfo, `3-map-${name}`);
  });
  test(`small hexes ${name}`, async ({ page }, testInfo) => {
    await open(page, ltpBoard(), size);
    await zoom(page, "Zoom Out", name === "desktop" ? 3 : 1);
    await shot(page, testInfo, `4-small-${name}`);
  });
}

test("zoomed in beside an anomaly", async ({ page }, testInfo) => {
  await open(page, ltpBoard(), SIZES.desktop);
  await zoom(page, "Zoom In", 3);
  await shot(page, testInfo, "5-large-desktop");
});

test("inspector", async ({ page }, testInfo) => {
  await open(page, ltpBoard(), SIZES.desktop);
  await page.getByTestId("system-hex-88a").click();
  await expect(page.getByTestId("inspector-hyperlane")).toBeVisible();
  await shot(page, testInfo, "6-inspector-desktop");
});
