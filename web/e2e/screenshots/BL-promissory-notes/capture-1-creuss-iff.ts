import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openAbility } from "./_note";

// A faction promissory note offered as a reaction: named and explained, not the raw id.
test("reaction: Creuss Iff", async ({ page }, testInfo) => {
  await openAbility(page, "TURN_BEGAN", "after", "promissory:empyrean:iff:TURN_BEGAN:after");
  await shot(page, testInfo, "1-creuss-iff");
});
