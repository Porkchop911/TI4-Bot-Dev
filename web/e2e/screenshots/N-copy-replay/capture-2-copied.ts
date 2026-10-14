import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { GAME_ID, openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";

// After a click the replay JSON is on the clipboard and the status line says how big it is.
test("copy replay: copied", async ({ page }, testInfo) => {
  const replay = {
    format: "ti4-replay",
    version: 1,
    game_id: GAME_ID,
    seed: 42,
    player_ids: ["player_a", "player_b"],
    map_template: null,
    history: { decisions: [], redo: [], events: [], revision: 1 },
  };
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"], {
    origin: new URL(testInfo.project.use.baseURL ?? "http://127.0.0.1").origin,
  });
  await page.route(`**/api/games/${GAME_ID}/replay`, (route) => route.fulfill({ json: replay }));
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "status" });
  await openLogFully(page);
  await page.getByTestId("copy-replay-btn").click();
  await expect(page.getByTestId("copy-replay-status")).toHaveAttribute("data-state", "copied");
  const clipboard = await page.evaluate(() => navigator.clipboard.readText());
  expect(JSON.parse(clipboard).format).toBe("ti4-replay");
  await shot(page, testInfo, "2-copied", { of: page.locator("#event-log-drawer"), pad: 8 });
});
