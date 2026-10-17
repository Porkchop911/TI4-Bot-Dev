import { expect, test, type Page } from "@playwright/test";
import { storedAnswers } from "../../offlineDriver";
import { shot } from "../_shared/shot";
import { VIEWPORTS, checking, playTo, prefix, startSeeded, type Viewport } from "./scene";

/** Opens the event log: the strip under the board on desktop, the Events button on a phone. */
async function openLog(page: Page, viewport: Viewport) {
  const toggle = page.getByTestId(viewport.name === "desktop" ? "event-log-toggle" : "event-log-mobile-toggle");
  await toggle.dispatchEvent("click");
  await expect(page.getByTestId("event-log-list")).toBeVisible();
}

/** The "#N" numbers of the entries on screen, top to bottom. */
const entryNumbers = (page: Page) =>
  page.getByTestId("event-log-entry").evaluateAll((rows) =>
    rows.map((row) => Number(/#(\d+)/.exec(row.querySelector(".event-log__index")?.textContent ?? "")?.[1] ?? NaN)).filter((n) => !Number.isNaN(n)),
  );

/** Folds the decision dialog away (a player minimizes it to read the log), when there is one. */
async function foldDialog(page: Page) {
  const minimize = page.getByTestId("minimize-choice-button");
  if (await minimize.count()) await minimize.first().dispatchEvent("click");
  await page.waitForTimeout(300);
}

// Nothing answered yet: nothing to undo.
if (checking)
  for (const viewport of VIEWPORTS)
    test(`undo disabled at the start at ${viewport.name}`, async ({ page }, testInfo) => {
      await startSeeded(page, viewport);
      await foldDialog(page);
      await openLog(page, viewport);
      await expect(page.getByTestId("event-log-undo-last")).toBeDisabled();
      await expect(page.getByTestId("event-log-undo-status")).toHaveText("Nothing to undo yet.");
      await shot(page, testInfo, `${prefix}-undo-disabled-${viewport.name}`);
    });

// The event log: newest entry first, and one Undo that takes back the player's last decision.
for (const viewport of VIEWPORTS) {
  test(`event log and undo at ${viewport.name}`, async ({ page }, testInfo) => {
    test.setTimeout(180_000);
    const gameId = await startSeeded(page, viewport);

    await playTo(page, gameId, 14);
    await page.waitForTimeout(600);
    await foldDialog(page);
    await openLog(page, viewport);
    const numbers = await entryNumbers(page);
    console.log(`entries top to bottom: ${numbers.slice(0, 6).join(", ")} ... ${numbers.slice(-3).join(", ")}`);
    await shot(page, testInfo, `${prefix}-log-${viewport.name}`);
    if (!checking) return;

    expect(numbers.length).toBeGreaterThan(3);
    expect(numbers[0], "newest entry on top").toBeGreaterThan(numbers[numbers.length - 1]);
    const undo = page.getByTestId("event-log-undo-last");
    await expect(undo).toBeEnabled();
    await expect(page.getByTestId("event-log-list")).toHaveJSProperty("scrollTop", 0);

    const answers = await storedAnswers(page, gameId);
    await undo.dispatchEvent("click");
    await expect.poll(() => storedAnswers(page, gameId), { timeout: 60_000 }).toBe(answers - 1);
    await expect(page.getByTestId("event-log-undo-status")).toContainText("Undid your last decision");
    // The question before the undone answer is asked again; fold its dialog away to see the log.
    await foldDialog(page);
    await shot(page, testInfo, `${prefix}-undone-${viewport.name}`);

    // A reload resumes at the undone position.
    await page.reload();
    await page.waitForSelector('[data-testid="offline-game"]', { timeout: 60_000 });
    expect(await storedAnswers(page, gameId)).toBe(answers - 1);
  });
}
