import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";

// The event log drawer of a seated player: "Copy replay" sits above the log.
// The status phase has no turn action bar, which would otherwise sit over the event log.
test("copy replay: idle button in the event log", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "status" });
  await openLogFully(page);
  await page.getByTestId("copy-replay-btn").waitFor();
  await shot(page, testInfo, "1-idle", { of: page.locator("#event-log-drawer"), pad: 8 });
});
