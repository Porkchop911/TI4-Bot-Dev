import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { GAME_ID, openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";

// When the server refuses, the reason shows in the status line and the button is usable again.
test("copy replay: server refuses", async ({ page }, testInfo) => {
  await page.route(`**/api/games/${GAME_ID}/replay`, (route) =>
    route.fulfill({ status: 403, body: "the session credential is not valid for this game" }),
  );
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "status" });
  await openLogFully(page);
  await page.getByTestId("copy-replay-btn").click();
  await expect(page.getByTestId("copy-replay-status")).toHaveAttribute("data-state", "error");
  await shot(page, testInfo, "3-error", { of: page.locator("#event-log-drawer"), pad: 8 });
});
