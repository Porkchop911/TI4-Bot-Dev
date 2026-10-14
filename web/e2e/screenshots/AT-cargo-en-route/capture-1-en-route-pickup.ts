import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { galleryPlayers } from "../_shared/fixtures";
import { actor } from "../_shared/players";
import type { BoardView } from "../../../src/protocol/types";

// 95.1: a carrier also picks up units from the systems it moves through. The movement tray used to
// list cargo from the carrier's origin only, so infantry standing in a system on the route could
// not be staged. The prefix is "before" on the commit without the fix.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const tile = (system_id: string, label: string, q: number, r: number) => ({ system_id, label, q, r });
const sys = (id: string, units: BoardView["systems"][string]["units"], tokens: string[] = []) => ({
  system_id: id,
  command_tokens: tokens,
  planets: {},
  units,
});
const infantry = { owner: actor, unit_type: "infantry", planet: "Lazar", damaged: false };

// 24 (carrier) - 30 (infantry on Lazar, a fighter in space) - 18 (active system)
const board = (midTokens: string[] = []): BoardView => ({
  active_system: "18",
  map_tiles: [tile("24", "#24", 0, 0), tile("30", "#30", 1, 0), tile("18", "Mecatol Rex", 2, 0)],
  systems: {
    "24": sys("24", [
      { owner: actor, unit_type: "carrier", damaged: false },
      { owner: actor, unit_type: "infantry", planet: "Home", damaged: false },
    ]),
    "30": sys(
      "30",
      [infantry, infantry, { owner: actor, unit_type: "fighter", damaged: false }],
      midTokens,
    ),
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

async function open(page: import("@playwright/test").Page, midTokens: string[] = []) {
  await openMockedGame(page, { board: board(midTokens), players: galleryPlayers, choice });
  await page.getByTestId("tactical-movement-tray").waitFor();
}

test("carrier with infantry staged from the system it passes through", async ({ page }, testInfo) => {
  await open(page);
  await page.getByTestId("rally-inc-24-carrier").click();
  const inc = page.getByTestId("rally-inc-cargo-24-infantry-Lazar-via-30");
  if (prefix === "after") {
    await inc.click();
    await inc.click();
  }
  await page.getByTestId("origin-group-24").scrollIntoViewIfNeeded();
  await shot(page, testInfo, `${prefix}-1-staged`, {
    of: page.getByTestId("tactical-movement-tray"),
    pad: 8,
  });
});

test("a command token on the passed system bars the pickup (95.5)", async ({ page }, testInfo) => {
  await open(page, [actor]);
  await page.getByTestId("rally-inc-24-carrier").click();
  await shot(page, testInfo, `${prefix}-2-token-bars`, {
    of: page.getByTestId("tactical-movement-tray"),
    pad: 8,
  });
});
