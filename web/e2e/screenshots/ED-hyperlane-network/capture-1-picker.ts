import { test, expect, type Page } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPicker } from "./open";
import { deriveHexGeometry } from "../../../src/presentation/boardPresentation";

// Hyperlane maps from the picker: the large preview, the strip thumbnail, and close-ups.
// The same specs ran on the commit before the change to make before/*.png (names match).
const MAPS: [players: number, alias: string][] = [
  [5, "5pRaceForMecatol"],
  [3, "3pHyperlanes"],
  [4, "4pHyperlanes"],
  [5, "5pLearnToPlayPoK"],
  [3, "3pHurtsDonut"],
];

const open = (page: Page, players: number, alias: string) => openPicker(page, players, alias);

/** Zooms the large preview to the hexes around the given centre (same svg, no re-layout). */
const closeUp = (page: Page, box: [number, number, number, number]) =>
  page.getByTestId("map-preview-board").evaluate((svg, b) => svg.setAttribute("viewBox", b.join(" ")), box);

for (const [players, alias] of MAPS) {
  test(`preview ${alias} desktop`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await open(page, players, alias);
    await shot(page, testInfo, `1-preview-${alias}-desktop`, { of: page.getByTestId("map-preview-board") });
  });
}

for (const [players, alias] of MAPS.slice(0, 3)) {
  test(`preview ${alias} phone`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await open(page, players, alias);
    await shot(page, testInfo, `2-preview-${alias}-390x844`);
  });
}

for (const [players, alias] of MAPS.slice(0, 4)) {
  test(`thumbnail ${alias}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const card = await open(page, players, alias);
    await shot(page, testInfo, `3-thumbnail-${alias}`, { of: card, pad: 4 });
  });
}

test("close-up: a long chain (Race for Mecatol, five lane tiles)", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await open(page, 5, "5pRaceForMecatol");
  await closeUp(page, [-300, -230, 600, 460]);
  await shot(page, testInfo, "4-closeup-chain", { of: page.getByTestId("map-preview-board") });
});

test("close-up: a junction (3 players, hyperlanes)", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await open(page, 3, "3pHyperlanes");
  await closeUp(page, [-460, -90, 480, 440]);
  await shot(page, testInfo, "5-closeup-junction", { of: page.getByTestId("map-preview-board") });
});

test("close-up: Learn to Play, three lane tiles in a row", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await open(page, 5, "5pLearnToPlayPoK");
  await closeUp(page, [-120, -500, 540, 440]);
  await shot(page, testInfo, "6-closeup-ltp", { of: page.getByTestId("map-preview-board") });
});

void deriveHexGeometry;
void expect;
