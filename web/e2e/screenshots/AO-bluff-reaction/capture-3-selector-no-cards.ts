import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand } from "../_shared/players";
import { opponent } from "../_shared/players";
import { openBluffGame, openSelector, ownCard } from "./_bluff";

// Hand size is public, so a seat with no action cards cannot credibly bluff: the selector is off.
test("selector: disabled with no action cards", async ({ page }, testInfo) => {
  await openBluffGame(page, {
    players: [
      playerWithHand({ action_cards_count: 0, held_action_cards: [] }),
      opponent,
    ],
  });
  const selector = await openSelector(page);
  await expect(selector).toHaveAttribute("data-state", "disabled");
  await expect(page.getByTestId("bluff-trigger-agenda")).toBeDisabled();
  await shot(page, testInfo, "3-selector-no-cards", { of: ownCard(page), pad: 8 });
});
