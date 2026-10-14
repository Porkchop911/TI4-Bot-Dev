import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";

// An action card named in the log is marked, and shows its effect on hover.
// The status phase has no turn action bar, which would otherwise sit over the event log.
test("tooltip: action card name in the event log", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "status" });
  await openLogFully(page);
  await hoverShot(page, testInfo, "14-eventlog-card-name", page.locator(".event-log__action-card").first(), { native: true, pad: 90 });
});
