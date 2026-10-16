import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { galleryPlayers } from "../_shared/fixtures";
import { actor } from "../_shared/players";
import type { BoardView } from "../../../src/protocol/types";

// One carrier and one sol_carrier in system 20. The engine offers a move per distinct type, so each
// row holds exactly one ship. Before the fix the sol_carrier row counted both (base type "carrier"),
// showed "Available: 2" and let a second sol_carrier be staged, which the server then rejected.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

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
        { owner: actor, unit_type: "carrier", damaged: false },
        { owner: actor, unit_type: "sol_carrier", damaged: false },
      ],
    },
    "18": { system_id: "18", command_tokens: [], planets: {}, units: [] },
  },
};
const choice = {
  prompt: "Move ships into Mecatol Rex",
  context: { subtype: "movement_step", target: { System: "18" } },
  options: [
    { id: "move|20|0", label: "Carrier", kind: "move", payload: { origin: "20", unit: "carrier", capacity: 4 } },
    { id: "move|20|1", label: "Sol Carrier", kind: "move", payload: { origin: "20", unit: "sol_carrier", capacity: 6 } },
    { id: "done_moving", label: "Finish Movement", kind: "decline" },
  ],
};

for (const [name, size] of Object.entries(SIZES)) {
  test(`carrier and sol_carrier ${name}`, async ({ page }, testInfo) => {
    await page.setViewportSize(size);
    await openMockedGame(page, { board, players: galleryPlayers, choice });
    await page.getByTestId("tactical-movement-tray").waitFor();
    await page.getByTestId("rally-inc-20-sol_carrier").click();
    if (prefix !== "before") {
      await expect(page.getByTestId("rally-count-20-sol_carrier")).toHaveText("1");
      await expect(page.getByTestId("rally-inc-20-sol_carrier")).toBeDisabled();
      await expect(page.getByTestId("rally-inc-20-carrier")).toBeEnabled();
      await expect(page.getByTestId("origin-group-20")).toContainText("Available: 1");
      await expect(page.getByTestId("origin-group-20")).not.toContainText("Available: 2");
    }
    await page.getByTestId("origin-group-20").scrollIntoViewIfNeeded();
    await shot(page, testInfo, `${prefix}-${name}`, { of: page.getByTestId("tactical-movement-tray"), pad: 8 });
  });
}
