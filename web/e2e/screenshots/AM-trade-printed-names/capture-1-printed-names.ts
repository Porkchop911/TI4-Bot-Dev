import { test, type Page, type TestInfo } from "@playwright/test";
import type { ChoiceOptionDto } from "../../../src/protocol/types";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent, withPlayer } from "../_shared/players";

// Before/after of promissory-note ids in the trade desk (Fix 7 of the 2026-10-07 morning analysis).
// TI4_SHOT_PREFIX=before is how the committed "before" shots were taken (on the base commit,
// unified-2026-10-08); the default "after" regenerates the current layout.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const PARTNER = "other_seat";

const players = [
  playerWithHand({ faction: "hacan", trade_goods: 3, commodities: 4 }),
  withPlayer(opponent, { id: PARTNER, faction: "jolnar", trade_goods: 5, commodities: 2, action_cards_count: 3 }),
];

const offer = (id: string, label: string, payload: Record<string, unknown> = {}): ChoiceOptionDto => ({
  id,
  label,
  kind: "offer",
  payload,
});

// Ids and labels exactly as transactions.rs writes them: bare note ids (cf:generic, ra:jolnar,
// support_for_throne), action card ids (sabo1) and an id no table knows (mystery_pact:generic).
const proposeChoice = {
  prompt: "transaction with Jolnar",
  context: { subtype: "propose_transaction", target: { Player: PARTNER } } as Record<string, unknown>,
  options: [
    offer("cc2", "swap 2 commodities each -- both gain", { net: 2, their_net: 2 }),
    offer("pncf:generic:2", "sell cf:generic for 2 trade goods", { note: "cf:generic", net: 0, their_net: 1 }),
    offer("pnra:hacan:0", "give ra:hacan", { note: "ra:hacan", gift: true, net: -1 }),
    offer("npra:jolnar:4", "pay 4 trade goods for the note ra:jolnar", { received_promissory: "ra:jolnar", net: 0 }),
    offer("pcmystery_pact:generic:2", "give the note mystery_pact:generic for 2 commodity", {
      promissory: "mystery_pact:generic",
      net: 1,
    }),
    offer("nncf:generic>ra:jolnar", "give the note cf:generic for the note ra:jolnar", {
      promissory: "cf:generic",
      received_promissory: "ra:jolnar",
    }),
    offer("cnsabo1>ra:jolnar", "give the action card sabo1 for the note ra:jolnar", {
      action_card: "sabo1",
      received_promissory: "ra:jolnar",
    }),
    { id: "decline", label: "Offer nothing", kind: "decline" },
  ] as ChoiceOptionDto[],
  nonce: "names-propose",
};

const answerChoice = {
  prompt: "Jolnar gives ra:jolnar, the action card sabo1 for 3 commodities, cf:generic -- accept?",
  context: { subtype: "answer_transaction", target: { Player: PARTNER } } as Record<string, unknown>,
  options: [
    { id: "accept", label: "accept", kind: "answer", payload: { net: 1 } },
    { id: "refuse", label: "refuse", kind: "decline" },
    { id: "counter", label: "counter-offer", kind: "answer" },
  ] as ChoiceOptionDto[],
  nonce: "names-answer",
};

async function open(page: Page, choice: unknown, width: number, height: number) {
  await page.setViewportSize({ width, height });
  await openMockedGame(page, { players, choice: choice as never });
  await page.getByTestId("trade-desk-modal").waitFor();
}

const shotModal = (page: Page, testInfo: TestInfo, name: string) =>
  shot(page, testInfo, `${prefix}-${name}`, {
    of: page.getByTestId("trade-desk-modal").locator(".choice-workflow-modal"),
    pad: 8,
  });

test(`${prefix} staging summary with notes`, async ({ page }, testInfo) => {
  await open(page, proposeChoice, 1100, 1250);
  await page.getByTestId("stage-give-note-cf:generic").click();
  await page.getByTestId("stage-receive-note-ra:jolnar").click();
  await page.getByTestId("stage-status-valid").waitFor();
  await shotModal(page, testInfo, "staging-summary");
});

test(`${prefix} quick deals with notes`, async ({ page }, testInfo) => {
  await open(page, proposeChoice, 1100, 1250);
  await page.getByTestId("quick-deals").locator("summary").click();
  await page.getByTestId("trade-tab-promissory").click();
  await page.getByTestId("trade-opt-pncf:generic:2").waitFor();
  await shotModal(page, testInfo, "quick-deals");
});

test(`${prefix} answer screen with notes`, async ({ page }, testInfo) => {
  await open(page, answerChoice, 1100, 1100);
  await page.getByTestId("offer-net").waitFor();
  await shotModal(page, testInfo, "answer-screen");
});

test(`${prefix} phone answer screen with notes`, async ({ page }, testInfo) => {
  await open(page, answerChoice, 390, 844);
  await page.getByTestId("offer-net").waitFor();
  await shot(page, testInfo, `${prefix}-phone-answer`);
});
