import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";
import { outerOffer, pass, trigger, other } from "../E-reaction-offer/_reaction";

// Bribery's window, "after votes cast", now opens once per voter: here after the opponent named
// their outcome. Before the change the same window opened again after each planet they exhausted.
test("bribery offer after a voter has voted", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openMockedGame(page, {
    phase: "agenda",
    players: [playerWithHand({ held_action_cards: ["bribery"], action_cards_count: 1 }), opponent],
    choice: {
      prompt: "after VOTES_CAST",
      context: {
        subtype: "reaction_after_VOTES_CAST",
        optional: true,
        source: { Reaction: "VOTES_CAST" },
        trigger: trigger("votes_cast", "VOTES_CAST", "after", { actor: other }),
      },
      options: [outerOffer("bribery", "Bribery", "VOTES_CAST", "after"), pass] as never,
    },
  });
  await page.getByTestId("reaction-status-bar").waitFor().catch(() => page.waitForTimeout(800));
  await page.waitForTimeout(500);
  await shot(page, testInfo, "1-offer");
});
