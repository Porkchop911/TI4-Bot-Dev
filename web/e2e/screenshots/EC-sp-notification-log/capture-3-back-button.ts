import { expect, test } from "@playwright/test";
import { storedAnswers } from "../../offlineDriver";
import { shot } from "../_shared/shot";
import { VIEWPORTS, checking, playTo, prefix, startSeeded } from "./scene";

// The browser Back button undoes the last decision and says so; it stays on the game screen.
if (checking)
  for (const viewport of VIEWPORTS)
    test(`back button undo at ${viewport.name}`, async ({ page }, testInfo) => {
      test.setTimeout(120_000);
      const gameId = await startSeeded(page, viewport);
      const answers = await playTo(page, gameId, 8);
      await page.waitForTimeout(600);
      const url = page.url();
      await page.goBack();
      await expect.poll(() => storedAnswers(page, gameId), { timeout: 60_000 }).toBe(answers - 1);
      expect(page.url()).toBe(url);
      await expect(page.getByTestId("back-undo-note")).toContainText("Undid your last decision");
      await shot(page, testInfo, `${prefix}-back-undo-${viewport.name}`);
    });
