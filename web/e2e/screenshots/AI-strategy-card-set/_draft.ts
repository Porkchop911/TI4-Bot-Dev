import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import type { Page } from "@playwright/test";

/** The eight cards a game of the given set deals: only Construction and Warfare differ. */
export function draftOptions(set: "te" | "pok") {
  const te = set === "te";
  return [
    ["pok1leadership", "1. Leadership"],
    ["pok2diplomacy", "2. Diplomacy"],
    ["pok3politics", "3. Politics"],
    [te ? "te4construction" : "pok4construction", "4. Construction"],
    ["pok5trade", "5. Trade"],
    [te ? "te6warfare" : "pok6warfare", "6. Warfare"],
    ["pok7technology", "7. Technology"],
    ["pok8imperial", "8. Imperial"],
  ].map(([id, label]) => ({ id, kind: "strategy_card", label }));
}

/** The strategy phase draft, with the eight cards of the game's set. */
export async function openDraft(page: Page, set: "te" | "pok") {
  await openMockedGame(page, {
    phase: "strategy",
    players: [playerWithHand({ strategy_cards: [] }), opponent],
    choice: {
      prompt: "Select a strategy card",
      context: { subtype: "draft_strategy_card" },
      options: draftOptions(set),
    },
  });
  await page.getByTestId("choice-option").first().waitFor();
}
