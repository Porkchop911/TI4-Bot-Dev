import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReaction, outerOffer, pass } from "./_reaction";

// An older server sends no trigger: the dialog says only which window opened, in words.
test("reaction: no trigger data", async ({ page }, testInfo) => {
  await openReaction(
    page,
    "reaction_after_SHIP_DESTROYED",
    [outerOffer("sabo1", "Sabotage", "SHIP_DESTROYED", "after"), pass],
    null,
    { Reaction: "SHIP_DESTROYED" },
    "after SHIP_DESTROYED",
  );
  await shot(page, testInfo, "6-fallback");
});
