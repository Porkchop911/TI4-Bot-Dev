import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { actor, galleryBoard } from "../_shared/fixtures";
import { opponent, playerWithHand, withPlayer } from "../_shared/players";
import { shot } from "../_shared/shot";

// After the viewer wins a space combat the Salvage window opens: the card now says how many
// commodities it would take from the beaten opponent (here Bob, holding 3).
const board = {
  ...galleryBoard,
  combat: { system_id: "18", round: 2, phase: "complete", attacker: opponent.id, defender: actor },
};

for (const [size, viewport] of [["desktop", { width: 1440, height: 900 }], ["390x844", { width: 390, height: 844 }]] as const) {
  test(`salvage ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openMockedGame(page, {
      players: [playerWithHand({ commodities: 1 }), withPlayer(opponent, { commodities: 3 })],
      board: board as never,
      choice: {
        prompt: "after SPACE_COMBAT_WON",
        context: {
          subtype: "reaction_after_SPACE_COMBAT_WON",
          optional: true,
          source: { Reaction: "SPACE_COMBAT_WON" },
          trigger: {
            kind: "space_combat_won",
            event_type: "SPACE_COMBAT_WON",
            event_id: 7,
            relation: "after",
            actor,
            system: "18",
          },
        },
        options: [
          {
            id: "reaction:Sol:SPACE_COMBAT_WON:after",
            label: "Play Salvage",
            kind: "ability",
            payload: { card: "salvage", card_name: "Salvage" },
          },
          { id: "decline", label: "Pass", kind: "decline" },
        ],
      },
    });
    // With the combat still on the board the combat dialog carries the window.
    await expect(page.locator('[data-testid="combat-salvage-effect"], [data-testid="reaction-row-effect"]')).toContainText("You would gain 3 commodities");
    await page.waitForTimeout(500);
    await shot(page, testInfo, `after-salvage-${size}`);
  });
}
