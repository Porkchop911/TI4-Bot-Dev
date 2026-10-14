import type { Page } from "@playwright/test";
import type { BattleOddsResponse } from "../../../src/protocol/advisorTypes";
import { openMockedGame } from "../_shared/mockGame";
import { combatStartFleets, completedCombatBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

/** A fixed simulation result, as the odds service (ti4-advisor, POST /advisor/battle) would answer. */
export const fixedOdds: BattleOddsResponse = {
  simulations: 2000,
  attacker_win_rate: 0.62,
  defender_win_rate: 0.28,
  mutual_destruction_rate: 0.1,
  unresolved_rate: 0,
  average_rounds: 2.4,
  attacker_expected_survivors: { dreadnought: 1.6, destroyer: 1.1, fighter: 5.2 },
  defender_expected_survivors: { cruiser: 0.7, destroyer: 0.4 },
  attacker_fielded: { dreadnought: 2, destroyer: 2, fighter: 8, carrier: 2 },
  defender_fielded: { cruiser: 2, destroyer: 2 },
};

/**
 * Opens a space combat before the first roll, with the odds service answered by a fixed response
 * through Playwright route interception (no advisor process needed).
 */
export async function openCombatWithOdds(page: Page) {
  await page.route("**/advisor/battle", (route) => route.fulfill({ json: fixedOdds }));
  const ended = completedCombatBoard({ phase: "pre_roll", round: 1, dice_rolls: [], attacker_hits: 0, defender_hits: 0 });
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    board: { ...ended, systems: { ...ended.systems, "18": { ...ended.systems["18"], units: combatStartFleets } } },
  });
  await page.getByTestId("combat-odds-card").waitFor();
  // The tag changes from the rough estimate to the simulated result once the response arrived.
  await page.getByTestId("combat-odds-tag").getByText(/Simulated/).waitFor();
}
