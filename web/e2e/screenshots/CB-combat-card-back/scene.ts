import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, actor } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

/** `before` is the engine and UI as they were: the card step offered no decline. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const trigger = { event_type: "HITS_TO_ASSIGN", relation: "when", payload: { player: "p2", hits: 2 } };

/** Step one: "Choose an action card..." plus Pass (the engine always offered this). */
export const outerChoice = {
  prompt: "when HITS_TO_ASSIGN",
  context: { subtype: "reaction_when_HITS_TO_ASSIGN", optional: true, source: { Reaction: "HITS_TO_ASSIGN" }, trigger },
  options: [
    { id: "reaction:hacan:HITS_TO_ASSIGN:when", label: "Choose an action card…", kind: "ability", payload: { card_names: ["Shields Holding", "Direct Hit"] } },
    { id: "decline", label: "Decline", kind: "decline" },
  ],
};

/** Step two: which card. Before the fix there was no decline here. */
export const innerChoice = {
  prompt: "play an action card (when HITS_TO_ASSIGN)",
  context: { subtype: "play_reaction_when_HITS_TO_ASSIGN", optional: prefix === "after", trigger },
  options: [
    { id: "sh1", label: "play Shields Holding", kind: "action_card", payload: { card: "sh1", card_name: "Shields Holding" } },
    { id: "direct_hit1", label: "play Direct Hit", kind: "action_card", payload: { card: "direct_hit1", card_name: "Direct Hit" } },
    ...(prefix === "after" ? [{ id: "decline", label: "Decline", kind: "decline" }] : []),
  ],
};

export async function openStep(page: Page, choice: typeof outerChoice | typeof innerChoice) {
  await openMockedGame(page, {
    players: [playerWithHand({ held_action_cards: ["sh1", "direct_hit1"], action_cards_count: 2 }), opponent],
    board: galleryBoard,
    choice,
  });
  void actor;
  await page.getByTestId("reaction-status-bar").waitFor();
}
