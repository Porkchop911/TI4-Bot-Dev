import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReaction, other, trigger, outerOffer, pass } from "./_reaction";

// A player who knows the card shrinks it: the text collapses to its first sentence, remembered for next time.
test("reaction: a card shrunk to its first sentence", async ({ page }, testInfo) => {
  await openReaction(
    page,
    "reaction_when_ACTION_CARD_PLAYED",
    [outerOffer("sabo1", "Sabotage", "ACTION_CARD_PLAYED", "when"), pass],
    trigger("action_card_played", "ACTION_CARD_PLAYED", "when", { actor: other, card: "plague" }),
    { Reaction: "ACTION_CARD_PLAYED" },
    "when ACTION_CARD_PLAYED",
  );
  await page.getByTestId("reaction-inspect-text-Plague").click();
  await shot(page, testInfo, "7-shrunk");
});
