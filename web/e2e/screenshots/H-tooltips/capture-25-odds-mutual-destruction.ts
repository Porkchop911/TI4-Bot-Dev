import { test } from "@playwright/test";
import { hoverShot } from "../_shared/hover";
import { openCombatWithOdds } from "./odds";

// The thin middle part of the bar is the chance that both fleets are wiped out.
test("tooltip: combat odds, mutual destruction", async ({ page }, testInfo) => {
  await openCombatWithOdds(page);
  await hoverShot(page, testInfo, "25-odds-mutual-destruction", page.locator(".combat-odds-bar__mutual").first(), { native: true, pad: 70 });
});
