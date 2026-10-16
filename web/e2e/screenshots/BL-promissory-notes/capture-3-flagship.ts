import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openAbility } from "./_note";

// A unit card's ability (the Arborec flagship) reads as the ship's name with its printed text.
test("reaction: Duha Menaimon", async ({ page }, testInfo) => {
  await openAbility(page, "SYSTEM_ACTIVATED", "after", "unit:arborec:arborec_flagship:SYSTEM_ACTIVATED:after");
  await shot(page, testInfo, "3-flagship");
});
