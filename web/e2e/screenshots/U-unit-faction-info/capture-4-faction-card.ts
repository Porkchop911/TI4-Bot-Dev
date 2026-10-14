import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { solSeat, opponent } from "./fixtures";

// The Faction button on a player sheet: abilities, promissory note, flagship, mech, techs, leaders.
test("faction info: own faction and another seat's", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [solSeat(), { ...opponent, leaders: { hacanagent: "Readied", hacancommander: "Unlocked", hacanhero: "Locked" } }] });
  const buttons = page.getByTestId("faction-info-button");
  await buttons.first().click();
  await page.getByTestId("faction-info-card").waitFor();
  await shot(page, testInfo, "5-faction-card-own");
  // The card scrolls; the rest of it is technologies and the three leaders.
  await page.getByTestId("faction-info-card").evaluate((el) => {
    const card = el.closest<HTMLElement>("[role=dialog]")!;
    card.scrollTop = card.scrollHeight;
  });
  await shot(page, testInfo, "5b-faction-card-leaders");
  await page.keyboard.press("Escape");
  await page.getByTestId("faction-info-card").waitFor({ state: "detached" });

  await buttons.nth(1).click();
  await page.getByTestId("faction-info-card").waitFor();
  await shot(page, testInfo, "6-faction-card-other-seat");
});
