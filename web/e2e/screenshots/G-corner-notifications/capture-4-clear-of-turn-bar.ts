import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, publicEntry, pushEntry } from "./corner";

// Where the box sits: lifted above the turn action bar and the event log, clear of the map
// overlays in the other corners. The whole window.
test("notifications: position above the turn action bar", async ({ page }, testInfo) => {
  const tokens = { tactic: 3, fleet: 3, strategy: 2 };
  const game = await openCornerGame(page, {
    players: [playerWithHand(), opponent],
    choice: {
      prompt: "action phase",
      options: [
        { id: "tactical", label: "Tactical action", kind: "action" },
        { id: "pass", label: "Pass", kind: "action" },
      ],
      details: { kind: "turn_menu", closing: false, tokens, strategy_cards: [], partners: [] },
    },
  });
  await page.getByTestId("turn-action-bar").waitFor();
  pushEntry(game, publicEntry("c1", "other_seat", "played Sabotage"));
  pushEntry(game, publicEntry("c2", "other_seat", "researched Neural Motivator"));
  await page.getByTestId("corner-toast").nth(1).waitFor();
  await page.waitForTimeout(700); // the layer re-measures the bars twice a second
  await shot(page, testInfo, "4-above-turn-bar");
});
