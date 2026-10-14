import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { advanceIdle, advanceTo, newDevicePage, prefix, substateOptions } from "./scene";

// The whole phase in one game on a phone: reveal, vote, resolution, the second agenda, and the
// strip going away with the phase.
test(`${prefix} flow`, async ({ browser, baseURL }, testInfo) => {
  test.skip(prefix === "before", "the strip is new");
  const { context, page } = await newDevicePage(browser, "mobile", baseURL);
  const strip = page.getByTestId("agenda-strip-text");
  const game = await openMockedGame(page, substateOptions("reveal-reaction"));
  await page.getByTestId("ti4-board-svg").waitFor();
  await expect(strip).toHaveText("Agenda 1/2: Minister of Peace · Your reaction");
  await shot(page, testInfo, "after-flow-1-reveal");

  await advanceTo(game, "vote-cast", 41);
  await expect(strip).toHaveText("Agenda 1/2: Minister of Peace · For/Against · Your vote");
  await advanceTo(game, "others-voting", 42);
  await expect(strip).toHaveText("Agenda 1/2: Minister of Peace · For/Against · Player 2's vote");
  await advanceTo(game, "tiebreak", 43);
  await expect(strip).toHaveText("Agenda 1/2: Minister of Peace · For/Against · You break the tie");

  // Resolution: nothing to decide, the agenda stays until the phase ends.
  await advanceIdle(game, 44);
  await expect(strip).toHaveText("Agenda 1/2: Minister of Peace · For/Against");
  await shot(page, testInfo, "after-flow-2-resolution");

  // The second agenda.
  await advanceTo(game, "elect-vote", 45);
  await expect(strip).toHaveText("Agenda 2/2: Minister of Commerce · Elect Player · Your vote");
  await shot(page, testInfo, "after-flow-3-second-agenda");

  // The phase ends: the strip goes and the page is back to normal.
  await advanceIdle(game, 46, "action");
  await expect(page.getByTestId("agenda-strip")).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.dataset.agendaStrip)).toBeUndefined();
  await shot(page, testInfo, "after-flow-4-phase-over");
  await context.close();
});
