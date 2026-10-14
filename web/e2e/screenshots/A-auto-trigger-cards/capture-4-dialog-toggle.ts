import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReaction, other, trigger, outerOffer, pass } from "../E-reaction-offer/_reaction";

// The same switch inside the reaction window, under the card it belongs to. Ticking it sends
// set_reaction_mode and passes this window; the box is drawn from the server's answer.
test("action cards: never offer from the reaction window", async ({ page }, testInfo) => {
  await openReaction(
    page,
    "reaction_when_ACTION_CARD_PLAYED",
    [outerOffer("sabo1", "Sabotage", "ACTION_CARD_PLAYED", "when"), pass],
    trigger("action_card_played", "ACTION_CARD_PLAYED", "when", { actor: other, card: "uprising" }),
    { Reaction: "ACTION_CARD_PLAYED" },
    "when ACTION_CARD_PLAYED",
  );
  const box = page.getByTestId("reaction-inspect-never-Sabotage");
  await expect(box).not.toBeChecked();
  await shot(page, testInfo, "4-dialog-switch", { of: page.getByTestId("reaction-status-bar"), pad: 8 });
  await box.check();
  await expect(box).toBeChecked();
  await shot(page, testInfo, "5-dialog-switch-ticked", { of: page.getByTestId("reaction-status-bar"), pad: 8 });
});
