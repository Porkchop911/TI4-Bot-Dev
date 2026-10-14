import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openAbility } from "./_note";

// A leader (the Argent commander) is shown by its card name.
test("reaction: Trrakan Aun Zulok", async ({ page }, testInfo) => {
  await openAbility(page, "UNIT_ABILITY_ROLLED", "when", "leader:argent:argentcommander:UNIT_ABILITY_ROLLED:when");
  await shot(page, testInfo, "4-commander");
});
