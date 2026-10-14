import { test } from "@playwright/test";
import { hoverShot } from "../_shared/hover";
import { openCombatWithOdds } from "./odds";

// The defender's share of the bar says its win percentage on hover.
test("tooltip: combat odds, defender win", async ({ page }, testInfo) => {
  await openCombatWithOdds(page);
  await hoverShot(page, testInfo, "26-odds-defender-win", page.locator(".combat-odds-bar__def").first(), { native: true, pad: 70 });
});
