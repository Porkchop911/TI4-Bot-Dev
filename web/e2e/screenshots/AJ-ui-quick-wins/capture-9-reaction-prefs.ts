import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReaction, other, trigger, outerOffer, pass } from "../E-reaction-offer/_reaction";

// The reaction bar after the type-size floor (12 px) and with larger, clearly clickable checkboxes.
test("reaction dialog: text size and checkboxes", async ({ page }, testInfo) => {
  await openReaction(
    page,
    "reaction_when_ACTION_CARD_PLAYED",
    [outerOffer("sabo1", "Sabotage", "ACTION_CARD_PLAYED", "when"), pass],
    trigger("action_card_played", "ACTION_CARD_PLAYED", "when", { actor: other, card: "uprising" }),
    { Reaction: "ACTION_CARD_PLAYED" },
    "when ACTION_CARD_PLAYED",
  );
  await shot(page, testInfo, "9-reaction-prefs");
});
