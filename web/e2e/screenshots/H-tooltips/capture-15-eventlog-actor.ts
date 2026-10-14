import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";

// The coloured symbol in the log names the player who acted.
// The status phase has no turn action bar, which would otherwise sit over the event log.
test("tooltip: actor symbol in the event log", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "status" });
  await openLogFully(page);
  await hoverShot(page, testInfo, "15-eventlog-actor", page.locator(".event-log__actor").first(), { native: true, pad: 90 });
});
