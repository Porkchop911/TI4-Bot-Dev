import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { galleryPlayers } from "../_shared/fixtures";
import { actor } from "../_shared/players";
import type { BoardView } from "../../../src/protocol/types";
import { SIZES } from "./scene";

// Letnev (fleet pool 3, Armada +2): the movement tray and the production drawer show the engine's
// limit of 5 and mark Armada's two. The choice details are what the engine sends for a Letnev seat.
const tile = (system_id: string, label: string, q: number, r: number) => ({ system_id, label, q, r });
const board: BoardView = {
  active_system: "18",
  map_tiles: [tile("20", "#20", 0, 0), tile("18", "Mecatol Rex", 1, 0)],
  systems: {
    "20": {
      system_id: "20",
      command_tokens: [],
      planets: {},
      units: [
        { owner: actor, unit_type: "cruiser", damaged: false },
        { owner: actor, unit_type: "cruiser", damaged: false },
      ],
    },
    "18": {
      system_id: "18",
      command_tokens: [],
      planets: {},
      units: [
        { owner: actor, unit_type: "dreadnought", damaged: false },
        { owner: actor, unit_type: "dreadnought", damaged: false },
        { owner: actor, unit_type: "dreadnought", damaged: false },
      ],
    },
  },
};
const players = [{ ...galleryPlayers[0], faction: "letnev", fleet_tokens: 3 }, ...galleryPlayers.slice(1)];

const movement = {
  prompt: "Move ships into Mecatol Rex",
  context: { subtype: "movement_step", target: { System: "18" } },
  details: { fleet_supply: { used: 3, limit: 5, armada: 2 } },
  options: [
    { id: "move|20|0", label: "Cruiser", kind: "move", payload: { origin: "20", unit: "cruiser", capacity: 0 } },
    { id: "done_moving", label: "Finish Movement", kind: "decline" },
  ],
};

const production = {
  prompt: "Produce units in system 18",
  context: { subtype: "produce_unit", target: { System: "18" }, outstanding: [{ kind: "production_capacity", amount: 4, paid: 0 }] },
  details: { fleet_supply: { used: 3, limit: 5, armada: 2 } },
  options: [
    {
      id: "build|cruiser|1",
      label: "produce cruiser for 2",
      kind: "produce",
      payload: { unit: "cruiser", cost: 2, printed_cost: 2, count: 1, production_spent: 1, available_resources: 8 },
    },
    { id: "done_producing", label: "Done producing", kind: "decline", payload: {} },
  ],
};

for (const [name, size] of Object.entries(SIZES)) {
  test(`movement shows Armada ${name}`, async ({ page }, testInfo) => {
    await page.setViewportSize(size);
    await openMockedGame(page, { board, players, choice: movement });
    await page.getByTestId("tactical-movement-tray").waitFor();
    await page.getByTestId("rally-inc-20-cruiser").click();
    await expect(page.getByTestId("fleet-supply-gauge")).toContainText("4 / 5 Ships");
    await expect(page.getByTestId("movement-armada")).toHaveText("incl. +2 Armada");
    const box = (await page.getByTestId("movement-armada").boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(size.width);
    await page.getByTestId("fleet-supply-gauge").scrollIntoViewIfNeeded();
    await shot(page, testInfo, `movement-${name}`, { of: page.getByTestId("tactical-movement-tray"), pad: 8 });
  });

  test(`production shows Armada ${name}`, async ({ page }, testInfo) => {
    await page.setViewportSize(size);
    await openMockedGame(page, { board, players, choice: production });
    await expect(page.getByTestId("fleet-supply-counter")).toHaveText("3 / 5");
    await expect(page.getByTestId("production-armada")).toHaveText("incl. +2 Armada");
    const box = (await page.getByTestId("production-armada").boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(size.width);
    await page.waitForTimeout(500);
    await shot(page, testInfo, `production-${name}`);
  });
}
