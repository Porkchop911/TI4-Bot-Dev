import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { advanceTo, agendaVisibility, newDevicePage, prefix, substateAgenda, substateOptions, type Substate } from "./scene";

// The dialog minimized to its pill: the map is free and the agenda must still be named on screen.
const states: Substate[] = ["vote-cast", "vote-planets", "predict", "reveal-reaction"];
for (const state of states) {
  test(`${prefix} minimized ${state}`, async ({ browser, baseURL }, testInfo) => {
    const { context, page } = await newDevicePage(browser, "mobile", baseURL);
    const game = await openMockedGame(page, substateOptions(state === "predict" ? "reveal-reaction" : state));
    await page.getByTestId("ti4-board-svg").waitFor();
    if (state === "predict") await advanceTo(game, "predict", 41);
    await page.getByRole("button", { name: "Minimize decision" }).click();
    await page.waitForTimeout(500);
    const v = await agendaVisibility(page, substateAgenda(state).name);
    console.log(`VISIBLE ${prefix} minimized-${state} ${JSON.stringify(v)}`);
    await shot(page, testInfo, `${prefix}-minimized-${state}`);
    if (prefix === "after") {
      expect(v.inViewport).toBe(true);
      expect(v.uncovered).toBe(true);
      // The pill names the agenda too.
      if (state !== "reveal-reaction") await expect(page.getByText(/^(Voting|Predict): /)).toBeVisible();
    }
    await context.close();
  });
}
