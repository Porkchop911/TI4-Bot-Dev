import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReaction, other, trigger, outerOffer, pass } from "./_reaction";

// The same dialog at a phone width: one scrolling body and a sticky Play / Pass row.
test("reaction: phone width", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openReaction(
    page,
    "reaction_when_ACTION_CARD_PLAYED",
    [outerOffer("sabo1", "Sabotage", "ACTION_CARD_PLAYED", "when"), pass],
    trigger("action_card_played", "ACTION_CARD_PLAYED", "when", { actor: other, card: "uprising" }),
    { Reaction: "ACTION_CARD_PLAYED" },
    "when ACTION_CARD_PLAYED",
  );
  await shot(page, testInfo, "5-phone");
});
