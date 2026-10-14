import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReaction, other, trigger, outerOffer, pass } from "./_reaction";

// Another player played an action card; the viewer holds Sabotage. The dialog says who played what
// (with the card's full text) and then what the viewer can do now.
test("reaction: Sabotage when an action card is played", async ({ page }, testInfo) => {
  await openReaction(
    page,
    "reaction_when_ACTION_CARD_PLAYED",
    [outerOffer("sabo1", "Sabotage", "ACTION_CARD_PLAYED", "when"), pass],
    trigger("action_card_played", "ACTION_CARD_PLAYED", "when", { actor: other, card: "uprising" }),
    { Reaction: "ACTION_CARD_PLAYED" },
    "when ACTION_CARD_PLAYED",
  );
  await shot(page, testInfo, "1-action-card-played");
});
