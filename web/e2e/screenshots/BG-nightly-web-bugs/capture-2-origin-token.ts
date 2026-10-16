import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { galleryPlayers } from "../_shared/fixtures";
import { actor } from "../_shared/players";
import type { BoardView } from "../../../src/protocol/types";

// 95.5: fighters and ground forces cannot be picked up from a system that holds one of the player's
// own command tokens, unless it is the active system. The Dominus Orb lets a ship leave such a system
// (58.4c) and the engine still refuses to load its garrison; the planner used to offer it anyway and
// the batch was refused. Both boards: carrier + fighter + infantry on Home at #72, active system #18.
const tile = (system_id: string, label: string, q: number, r: number) => ({ system_id, label, q, r });
const sys = (id: string, units: BoardView["systems"][string]["units"], tokens: string[] = []) => ({
  system_id: id,
  command_tokens: tokens,
  planets: {},
  units,
});
const board = (originTokens: string[]): BoardView => ({
  active_system: "18",
  map_tiles: [tile("72", "#72", 0, 0), tile("18", "Mecatol Rex", 1, 0)],
  systems: {
    "72": sys(
      "72",
      [
        { owner: actor, unit_type: "carrier", damaged: false },
        { owner: actor, unit_type: "fighter", damaged: false },
        { owner: actor, unit_type: "infantry", planet: "Home", damaged: false },
      ],
      originTokens,
    ),
    "18": sys("18", []),
  },
});
const choice = {
  prompt: "Move ships into Mecatol Rex",
  context: { subtype: "movement_step", target: { System: "18" } },
  options: [
    { id: "move|72|0", label: "Carrier", kind: "move", payload: { origin: "72", unit: "carrier", capacity: 4 } },
    { id: "done_moving", label: "Finish Movement", kind: "decline" },
  ],
};

for (const [name, tokens] of [["1-no-token-offers-cargo", []], ["2-own-token-no-cargo", [actor]]] as const) {
  test(name, async ({ page }, testInfo) => {
    await openMockedGame(page, { board: board([...tokens]), players: galleryPlayers, choice });
    await page.getByTestId("tactical-movement-tray").waitFor();
    await page.getByTestId("rally-inc-72-carrier").click();
    const fighter = page.getByTestId("rally-row-cargo-72-fighter-space");
    if (tokens.length) await expect(fighter).toHaveCount(0);
    else await expect(fighter).toBeVisible();
    await shot(page, testInfo, `5-${name}`, { of: page.getByTestId("tactical-movement-tray"), pad: 8 });
  });
}
