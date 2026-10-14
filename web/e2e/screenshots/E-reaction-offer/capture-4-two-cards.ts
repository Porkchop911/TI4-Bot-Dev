import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReaction, other, trigger, pass } from "./_reaction";

// Two differently named cards can answer the same window: the inner step repeats the trigger and
// lists each card with its printed window and full text.
test("reaction: two different cards offered for one window", async ({ page }, testInfo) => {
  await openReaction(
    page,
    "play_reaction_after_SYSTEM_ACTIVATED",
    [
      { id: "decoy", label: "play Decoy Operation", kind: "action_card", payload: { card: "decoy", card_name: "Decoy Operation" } },
      { id: "flank_speed", label: "play Flank Speed", kind: "action_card", payload: { card: "fs1", card_name: "Flank Speed" } },
      pass,
    ],
    trigger("system_activated", "SYSTEM_ACTIVATED", "after", { actor: other, system: "18" }),
    { Rule: "22.1" },
    "play an action card (after SYSTEM_ACTIVATED)",
  );
  await shot(page, testInfo, "4-two-cards");
});
