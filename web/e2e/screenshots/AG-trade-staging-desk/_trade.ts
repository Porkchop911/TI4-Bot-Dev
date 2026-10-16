import type { Page, TestInfo } from "@playwright/test";
import type { ChoiceOptionDto } from "../../../src/protocol/types";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent, withPlayer } from "../_shared/players";

export const PARTNER = "other_seat";

/** The viewer (Hacan-style purse) and a partner whose trade goods and commodities are public. */
export const tradePlayers = [
  playerWithHand({ faction: "hacan", trade_goods: 3, commodities: 4, held_action_cards: ["sabo1", "direct_hit", "decoy"] }),
  withPlayer(opponent, { id: PARTNER, faction: "jolnar", trade_goods: 5, commodities: 2, action_cards_count: 3 }),
];

const offer = (id: string, label: string, payload: Record<string, unknown> = {}): ChoiceOptionDto => ({
  id,
  label,
  kind: "offer",
  payload,
});

/** A catalogue shaped like the engine's offer menu for these two seats (ids as in transactions.rs). */
export const proposeOptions: ChoiceOptionDto[] = [
  offer("cc2", "swap 2 commodities each -- both gain", { net: 2, their_net: 2 }),
  offer("cc1", "swap 1 commodity each", { net: 1, their_net: 1 }),
  offer("ct3:2", "give 3 commodities for 2 trade goods", { net: 0, their_net: 1 }),
  offer("tc2:2", "give 2 trade goods for 2 commodities", { net: 0, their_net: 0 }),
  offer("c4:0", "gift 4 commodities", { net: -4 }),
  offer("0:1", "give 0 for 1", { net: 1 }),
  offer("1:0", "give 1 for 0", { net: -1 }),
  offer("1:1", "give 1 for 1", { net: 0 }),
  offer("2:3", "give 2 for 3", { net: 1 }),
  offer("3:2", "give 3 for 2", { net: -1 }),
  offer("pntrade_agreement:hacan:2", "sell trade_agreement:hacan for 2 trade goods", { note: "trade_agreement:hacan", net: 0, their_net: 1 }),
  offer("pnceasefire:hacan:0", "give ceasefire:hacan", { note: "ceasefire:hacan", gift: true, net: -1 }),
  offer("npcf:jolnar:2", "pay 2 trade goods for the note cf:jolnar", { received_promissory: "cf:jolnar", net: 0 }),
  offer("cppolitical_secret:jolnar:3", "pay 3 commodities for the note political_secret:jolnar", { received_promissory: "political_secret:jolnar", net: 0 }),
  offer("pcceasefire:hacan:2", "give the note ceasefire:hacan for 2 commodities", { promissory: "ceasefire:hacan", net: 1 }),
  offer("nntrade_agreement:hacan>political_secret:jolnar", "give the note trade_agreement:hacan for the note political_secret:jolnar", {
    promissory: "trade_agreement:hacan",
    received_promissory: "political_secret:jolnar",
  }),
  offer("cnbribery>political_secret:jolnar", "give the action card bribery for the note political_secret:jolnar", {
    action_card: "bribery",
    received_promissory: "political_secret:jolnar",
  }),
  { id: "decline", label: "Offer nothing", kind: "decline" },
];

export const proposeChoice = {
  prompt: "transaction with Jolnar",
  context: { subtype: "propose_transaction", target: { Player: PARTNER } } as Record<string, unknown>,
  options: proposeOptions,
  nonce: "trade-propose",
};

export const answerChoice = {
  prompt: "Jolnar gives 2 trade goods, trade_agreement:jolnar for 3 commodities -- accept?",
  context: { subtype: "answer_transaction", target: { Player: PARTNER } } as Record<string, unknown>,
  options: [
    { id: "accept", label: "accept", kind: "answer", payload: { net: 1 } },
    { id: "refuse", label: "refuse", kind: "decline" },
    { id: "counter", label: "counter-offer", kind: "answer" },
  ] as ChoiceOptionDto[],
  nonce: "trade-answer",
};

export async function openDesk(page: Page, choice = proposeChoice, height = 1250) {
  await page.setViewportSize({ width: 1100, height });
  await openMockedGame(page, { players: tradePlayers, choice });
  await page.getByTestId("trade-desk-modal").waitFor();
}

export const click = (page: Page, testId: string) => page.getByTestId(testId).click();

export async function shotDesk(page: Page, testInfo: TestInfo, name: string) {
  return shot(page, testInfo, name, { of: page.getByTestId("trade-desk-modal").locator(".choice-workflow-modal"), pad: 8 });
}
