import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { openLogFully } from "../_shared/eventLog";
import { playerWithHand, opponent } from "../_shared/players";

// Bots keep playing while the host redoes: the server answers 409 every time. After the retries the
// player reads one plain sentence instead of the raw "History change failed (409): ..." text.
test("redo: the game moved on", async ({ page }, testInfo) => {
  test.setTimeout(60_000);
  await page.route("**/api/games/*/history", (route) =>
    route.fulfill({ status: 409, body: "Game advanced during rewind" }),
  );
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    events: actionCardEventLog(),
    history: { cursor: 5, redo_count: 3 },
  });
  await openLogFully(page);
  await page.getByRole("button", { name: "Redo one" }).click();
  await page.getByText("The game moved on while redoing — try again").waitFor({ timeout: 30_000 });
  await shot(page, testInfo, "1-game-moved-on");
});
