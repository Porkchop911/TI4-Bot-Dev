import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { actor, board, exploration_log, players } from "./_fixtures";

// The exploration markers on the bigger planet tiles, zoomed in (3x): they must stay inside the hex
// and clear of the planet discs, value pills, units badge and command tokens. System 10 has two
// planets (Arc Prime, Wren Terra) and is given an attachment, an exploration and a frontier token
// so all three markers stack on a tile with discs out to the right edge. The new records carry the
// engine's structured `gains` / `placed` fields.
test.use({ deviceScaleFactor: 3 });

const log = [
  ...exploration_log,
  { round: 2, player: actor, deck: "INDUSTRIAL", card: "mw", planet: "arc_prime", outcome: "attached:miningworld", gains: {}, placed: ["attachment:miningworld:arc_prime"] },
  { round: 3, player: actor, deck: "HAZARDOUS", card: "cm2", planet: "wren_terra", outcome: "resolved", result: "", gains: { trade_goods: 2 }, placed: [] },
];

const withTwoPlanetMarkers = (system: string) => ({
  board: {
    ...board,
    systems: {
      ...board.systems,
      "10": {
        ...board.systems["10"],
        planets: {
          ...board.systems["10"]?.planets,
          arc_prime: { planet_id: "arc_prime", controlled_by: actor, exhausted: false, attachments: ["miningworld"] },
        },
      },
    },
  },
  frontier: [system, "41"],
});

for (const id of ["26", "24", "19", "10"]) {
  test(`tile ${id} with exploration markers`, async ({ page }, testInfo) => {
    const { list, state } = players();
    const extra = withTwoPlanetMarkers("10");
    await openMockedGame(page, {
      board: extra.board,
      players: list,
      state: { ...state, exploration_log: log, frontier_tokens: extra.frontier },
    });
    const tile = page.getByTestId(`system-hex-${id}`);
    await expect(tile).toBeVisible();
    await shot(page, testInfo, `tile-fit-${id}`, { of: tile, pad: 6 });
  });
}
