import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";
import { openPayment, pickOnMap } from "../D-planet-selection/payment";

/** `before` is taken on the commit without the change. */
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

/** The gallery board with a third planet in system 20 (Vefun), to show a 3-planet tile. */
const threePlanetBoard = {
  ...galleryBoard,
  map_tiles: galleryBoard.map_tiles.map((t) =>
    t.system_id === "20"
      ? {
          ...t,
          planets: [
            ...t.planets,
            { id: "ordos", label: "Ordos", resources: 3, influence: 1 },
            { id: "kyrr", label: "Kyrr", resources: 0, influence: 2 },
          ],
        }
      : t,
  ),
};

const open = (page: import("@playwright/test").Page) =>
  openMockedGame(page, { players: [playerWithHand(), opponent], board: threePlanetBoard });

const tile = (page: import("@playwright/test").Page, id: string) => page.getByTestId(`system-hex-${id}`);

test("map on desktop", async ({ page }, testInfo) => {
  await open(page);
  await tile(page, "18").waitFor();
  await shot(page, testInfo, `${prefix}-map-desktop`);
});

test("map on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await open(page);
  await tile(page, "18").waitFor();
  await shot(page, testInfo, `${prefix}-map-phone`);
});

for (const [name, id] of [
  ["tile-two-planets", "18"],
  ["tile-three-planets", "20"],
  ["tile-legendary", "65"],
  ["tile-wormhole", "26"],
  ["tile-units-and-tokens", "19"],
] as const) {
  test(`${name} (desktop)`, async ({ page }, testInfo) => {
    await open(page);
    await shot(page, testInfo, `${prefix}-${name}`, { of: tile(page, id), pad: 6 });
  });
}

test("payment on the map", async ({ page }, testInfo) => {
  await openPayment(page);
  await pickOnMap(page);
  await shot(page, testInfo, `${prefix}-payment-desktop`);
});

test("payment on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openPayment(page);
  await pickOnMap(page);
  await shot(page, testInfo, `${prefix}-payment-phone`);
});
