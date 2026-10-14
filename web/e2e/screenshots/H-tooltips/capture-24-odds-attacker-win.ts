import { test } from "@playwright/test";
import { hoverShot } from "../_shared/hover";
import { openCombatWithOdds } from "./odds";

// The attacker's share of the win-probability bar says its percentage on hover (a native title).
test("tooltip: combat odds, attacker win", async ({ page }, testInfo) => {
  await openCombatWithOdds(page);
  await hoverShot(page, testInfo, "24-odds-attacker-win", page.locator(".combat-odds-bar__att").first(), { native: true, pad: 70 });
});
