import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, thirdPlayer } from "../G-corner-notifications/corner";

const windowFor = (trigger: Record<string, unknown>, subtype: string) => ({
  prompt: String(trigger.event_type),
  context: { subtype, optional: true, source: { Reaction: trigger.event_type }, trigger },
  options: [
    { id: "reaction:x", kind: "ability", label: "Play Sabotage", payload: { card: "sabo1", card_name: "Sabotage" } },
    { id: "decline", kind: "decline", label: "Pass" },
  ],
});

// The normal reaction window says which card was played or discarded, its text and what it targets.
test("reaction window: Direct Hit with its targets", async ({ page }, testInfo) => {
  await openCornerGame(page, {
    players: [playerWithHand(), opponent, thirdPlayer],
    choice: windowFor(
      { kind: "action_card_played", event_type: "ACTION_CARD_PLAYED", event_id: 41, relation: "when", actor: "other_seat", card: "direct_hit", system: "27", planet: "mecatol_rex", subject: "third_seat" },
      "reaction_when_ACTION_CARD_PLAYED",
    ),
  });
  await expect(page.getByTestId("reaction-trigger-targets")).toBeVisible();
  await shot(page, testInfo, "4a-sabotage-window");
});

test("reaction window: Reverse Engineer", async ({ page }, testInfo) => {
  await openCornerGame(page, {
    players: [playerWithHand(), opponent, thirdPlayer],
    choice: windowFor(
      { kind: "action_card_discarded", event_type: "ACTION_CARD_DISCARDED", event_id: 42, relation: "after", actor: "other_seat", card: "lucky_shot" },
      "reaction_after_ACTION_CARD_DISCARDED",
    ),
  });
  await expect(page.getByTestId("reaction-trigger-card")).toBeVisible();
  await shot(page, testInfo, "4b-reverse-engineer-window");
});
