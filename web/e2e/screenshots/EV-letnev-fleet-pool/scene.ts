import { expect, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

/**
 * Letnev's status-phase redistribution of 9 held tokens. `floor` is the fewest fleet tokens the
 * engine offers: 2 is what it offered before the fix, 0 after (Letnev only, rules version 4).
 */
export function redistributeChoice(floor: number, armada = false) {
  const total = 9;
  const options: { id: string; label: string; kind: string }[] = [];
  for (let tactic = 0; tactic <= total; tactic++) {
    for (let fleet = floor; fleet <= total - tactic; fleet++) {
      const strategic = total - tactic - fleet;
      options.push({
        id: `${tactic}|${fleet}|${strategic}`,
        label: `tactic ${tactic} / fleet ${fleet} / strategy ${strategic}`,
        kind: "redistribute",
      });
    }
  }
  return {
    prompt: "redistribute your command tokens",
    nonce: `shots-ev-${floor}`,
    context: { subtype: "status_redistribute_tokens" },
    options,
    details: {
      kind: "command_tokens",
      mode: "redistribute",
      pools: { tactic: 3, fleet: 4, strategic: 2, ...(armada ? { fleet_supply_bonus: 2 } : {}) },
      reinforcements: 7,
      total,
    },
  };
}

export async function openRedistribution(page: Page, size: SizeName, floor: number, armada = false) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    players: [playerWithHand(armada ? { faction: "letnev" } : {}), opponent],
    board: galleryBoard,
    choice: redistributeChoice(floor, armada),
  });
  await page.getByTestId("command-token-panel").waitFor();
  await expect(page.getByTestId("command-token-panel")).toBeVisible();
  await page.waitForTimeout(500);
}

/** Move the fleet pool down to `fleet` and put the freed tokens on the strategy pool. */
export async function lowerFleetTo(page: Page, fleet: number) {
  for (let held = 4; held > fleet; held--) {
    await page.getByTestId("token-minus-fleet").click();
    await page.getByTestId("token-plus-strategic").click();
  }
}
