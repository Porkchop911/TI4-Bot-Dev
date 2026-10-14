import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReaction, other, trigger, outerOffer, pass } from "./_reaction";

// Another player activated a system where the viewer has units: the system links to the map and the
// dialog says how many of the viewer's units stand there, then offers Decoy Operation with its window.
test("reaction: a system with your units was activated", async ({ page }, testInfo) => {
  await openReaction(
    page,
    "reaction_after_SYSTEM_ACTIVATED",
    [outerOffer("decoy", "Decoy Operation", "SYSTEM_ACTIVATED", "after"), pass],
    trigger("system_activated", "SYSTEM_ACTIVATED", "after", { actor: other, system: "18" }),
    { Reaction: "SYSTEM_ACTIVATED" },
    "after SYSTEM_ACTIVATED",
  );
  await shot(page, testInfo, "2-system-activated");
});
