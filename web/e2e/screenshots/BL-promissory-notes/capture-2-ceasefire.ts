import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openAbility } from "./_note";

// A generic note (every seat holds its own Ceasefire) uses the printed name too.
test("reaction: Ceasefire", async ({ page }, testInfo) => {
  await openAbility(page, "SYSTEM_ACTIVATED", "after", "promissory:xxcha:cf:SYSTEM_ACTIVATED:after");
  await shot(page, testInfo, "2-ceasefire");
});
