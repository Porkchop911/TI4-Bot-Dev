import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { openLogFully } from "../_shared/eventLog";
import { showTitleTooltip } from "../_shared/tooltip";
import { playerWithHand, opponent } from "../_shared/players";

// While a history change is running the undo and redo buttons are disabled and say why.
test("undo/redo buttons: disabled while a change runs", async ({ page }, testInfo) => {
  await page.route("**/api/games/*/history", () => new Promise(() => {}));
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    events: actionCardEventLog(),
    history: { cursor: 5, redo_count: 3 },
  });
  await openLogFully(page);
  await page.getByRole("button", { name: "Redo one" }).click();
  const redo = page.getByRole("button", { name: "Redo batch" });
  await redo.waitFor();
  await showTitleTooltip(page, redo);
  await shot(page, testInfo, "2-buttons-busy");
});
