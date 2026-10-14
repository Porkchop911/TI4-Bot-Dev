import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReaction, other, trigger, outerOffer, pass } from "./_reaction";

// The engine's SHIP_MOVED now names the destination and the arriving ships.
test("reaction: ships moved into a system", async ({ page }, testInfo) => {
  await openReaction(
    page,
    "reaction_after_SHIP_MOVED",
    [outerOffer("rescue", "Rescue", "SHIP_MOVED", "after"), pass],
    trigger("ship_moved", "SHIP_MOVED", "after", {
      actor: other,
      system: "26",
      units: [
        { owner: other, unit_type: "cruiser", count: 2 },
        { owner: other, unit_type: "dreadnought", count: 1 },
      ],
    }),
    { Reaction: "SHIP_MOVED" },
    "after SHIP_MOVED",
  );
  await shot(page, testInfo, "3-ships-moved");
});
