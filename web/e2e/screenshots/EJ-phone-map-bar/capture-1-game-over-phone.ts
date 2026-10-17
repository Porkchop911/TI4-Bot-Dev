import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame, GAME_ID } from "../_shared/mockGame";
import { me, opponent } from "../_shared/players";

/** `before` shots come from the commit before the fix (the assertions are skipped then). */
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

// On a phone the board column had no definite height, so the board fell back to the map's aspect
// ratio (367 px of a 503 px column at 412x915): the rest was an empty dark band under the map and
// a zoomed map was cut off by a straight edge above it. The board must now fill the column down to
// the bottom of the screen.
for (const [w, h] of [[412, 915], [390, 844], [360, 740]]) {
  test(`phone map fills the screen after game over ${w}x${h}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: w, height: h });
    const game = await openMockedGame(page, { players: [me, opponent], phase: "status", round: 5 });
    await page.getByTestId("turn-status-banner").waitFor();
    game.send({
      type: "game_over",
      protocol_version: game.snapshot.protocol_version,
      game_id: GAME_ID,
      game_version: game.snapshot.game_version + 1,
      winner: me.id,
      final_scores: { [me.id]: 10, [opponent.id]: 9 },
    });
    await page.getByTestId("turn-status-banner").getByText("Game Over").waitFor();
    await shot(page, testInfo, `${prefix}-${w}x${h}`);
    if (prefix !== "after") return;
    const view = (await page.getByTestId("board-viewport").boundingBox())!;
    const svg = (await page.getByTestId("ti4-board-svg").boundingBox())!;
    const column = (await page.locator(".app-shell__board").boundingBox())!;
    // The map container is the whole column, and the column reaches the bottom of the screen.
    expect(view.height).toBeGreaterThanOrEqual(column.height - 1);
    expect(svg.height).toBeGreaterThanOrEqual(view.height - 1);
    expect(view.y + view.height).toBeGreaterThanOrEqual(h - 2);
  });
}
