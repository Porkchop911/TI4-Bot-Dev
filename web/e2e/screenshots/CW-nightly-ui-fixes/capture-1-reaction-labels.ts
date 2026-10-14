import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReaction, other, trigger, pass } from "../E-reaction-offer/_reaction";

// Faction-ability and leader reactions used to show the engine's raw ids as their labels. The
// engine now labels them with the printed name and the window; the ids stay as they were.
const ABILITY = "ability:empyrean:aetherpassage:SYSTEM_ACTIVATED:after";
const AGENT = "leader:l1z1x:l1z1xagent:SYSTEM_ACTIVATED:after";

for (const variant of ["before", "after"] as const) {
  test(`reaction labels ${variant}`, async ({ page }, testInfo) => {
    const label = (id: string, named: string) => (variant === "before" ? id : named);
    await openReaction(
      page,
      "reaction_after_SYSTEM_ACTIVATED",
      [
        { id: ABILITY, label: label(ABILITY, "Aetherpassage (after system activated)"), kind: "ability" },
        { id: AGENT, label: label(AGENT, "Agent I48S (after system activated)"), kind: "ability" },
        pass,
      ],
      trigger("system_activated", "SYSTEM_ACTIVATED", "after", { actor: other, system: "18" }),
      { Reaction: "SYSTEM_ACTIVATED" },
      "after SYSTEM_ACTIVATED",
    );
    await shot(page, testInfo, `1-reaction-labels-${variant}`);
  });
}
