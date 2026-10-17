import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { combatStartFleets, completedCombatBoard } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

/**
 * Opens a space combat before the first roll. The odds are the engine's (the page's own engine:
 * a fixed seed, so the same fleets give the same numbers and the same pixels).
 */
export async function openCombatWithOdds(page: Page) {
  const ended = completedCombatBoard({ phase: "pre_roll", round: 1, dice_rolls: [], attacker_hits: 0, defender_hits: 0 });
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    board: { ...ended, systems: { ...ended.systems, "18": { ...ended.systems["18"], units: combatStartFleets } } },
  });
  await page.getByTestId("combat-odds-card").waitFor();
  // The tag changes from the rough estimate to the simulated result once the engine answered.
  await page.getByTestId("combat-odds-tag").getByText(/Simulated/).waitFor();
}
