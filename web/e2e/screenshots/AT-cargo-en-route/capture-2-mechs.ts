import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { galleryPlayers } from "../_shared/fixtures";
import { actor } from "../_shared/players";
import type { BoardView } from "../../../src/protocol/types";

// Mechs picked up en route (95.1). A mech is a ground force and uses one slot of the hold, except
// the Argent Flight's Aerie Sentinel (`argent_mech`), which the engine carries free
// (MovementHooks::free_cargo): a fifth unit on a hold of four. The "before" prefix is the commit
// that had the en-route pickup but counted every mech as a slot.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const tile = (system_id: string, label: string, q: number, r: number) => ({ system_id, label, q, r });
const sys = (id: string, units: BoardView["systems"][string]["units"]) => ({
  system_id: id,
  command_tokens: [],
  planets: {},
  units,
});
const infantry = { owner: actor, unit_type: "infantry", planet: "Lazar", damaged: false };
const board = (mech: string): BoardView => ({
  active_system: "18",
  map_tiles: [tile("24", "#24", 0, 0), tile("30", "#30", 1, 0), tile("18", "Mecatol Rex", 2, 0)],
  systems: {
    "24": sys("24", [{ owner: actor, unit_type: "carrier", damaged: false }]),
    "30": sys("30", [
      { owner: actor, unit_type: mech, planet: "Lazar", damaged: false },
      infantry,
      infantry,
      infantry,
      infantry,
    ]),
    "18": sys("18", []),
  },
});
const choice = {
  prompt: "Move ships into Mecatol Rex",
  context: { subtype: "movement_step", target: { System: "18" } },
  options: [
    {
      id: "move|24|0",
      label: "Carrier",
      kind: "move",
      payload: { origin: "24", unit: "carrier", capacity: 4 },
    },
    { id: "done_moving", label: "Finish Movement", kind: "decline" },
  ],
};

async function stage(page: import("@playwright/test").Page, mech: string, infantryCount: number) {
  await openMockedGame(page, { board: board(mech), players: galleryPlayers, choice });
  await page.getByTestId("tactical-movement-tray").waitFor();
  await page.getByTestId("rally-inc-24-carrier").click();
  const inf = page.getByTestId("rally-inc-cargo-24-infantry-Lazar-via-30");
  for (let i = 0; i < infantryCount; i++) await inf.click();
  const unit = page.getByTestId(`rally-inc-cargo-24-${mech}-Lazar-via-30`);
  if (!(await unit.isDisabled())) await unit.click();
  await page.getByTestId("origin-group-24").scrollIntoViewIfNeeded();
}

test("a mech and three infantry fill the hold of four", async ({ page }, testInfo) => {
  await stage(page, "mech", 3);
  await shot(page, testInfo, `${prefix}-3-mech-with-infantry`, {
    of: page.getByTestId("tactical-movement-tray"),
    pad: 8,
  });
});

test("an Argent mech rides free beside four infantry", async ({ page }, testInfo) => {
  await stage(page, "argent_mech", 4);
  await shot(page, testInfo, `${prefix}-4-argent-mech-free`, {
    of: page.getByTestId("tactical-movement-tray"),
    pad: 8,
  });
});
