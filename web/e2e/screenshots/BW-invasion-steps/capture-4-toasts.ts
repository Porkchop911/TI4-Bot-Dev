import { test } from "@playwright/test";
import { PROTOCOL_VERSION } from "../../../src/protocol/types";
import type { BoardView } from "../../../src/protocol/types";
import type { MockedGame } from "../_shared/mockGame";
import {
  actor,
  bombardmentStep,
  defenders,
  defendersAfterBombardment,
  groundStep,
  invasionBoard,
  openInvasion,
  prefix,
  spaceCannonStep,
  take,
  viewports,
} from "./scene";
import type { Page } from "@playwright/test";

function update(game: MockedGame, board: BoardView, n: number) {
  const { events: _events, ...base } = game.snapshot;
  game.send({
    ...base,
    type: "state_update",
    protocol_version: PROTOCOL_VERSION,
    game_version: game.snapshot.game_version + n,
    view: { ...game.snapshot.view, board },
    pending_choice: null,
  });
}

async function toastOrWait(page: Page, text: RegExp) {
  // The base commit has no invasion toasts: wait a moment instead and shoot what is there.
  if (prefix === "before") await page.waitForTimeout(700);
  else await page.locator('[data-toast-kind="invasion"]', { hasText: text }).first().waitFor();
}

// The whole invasion of Jord, step by step, with the toasts it raises.
for (const v of viewports) {
  test(`invasion-toasts-${v.id}`, async ({ page }, testInfo) => {
    // The page is open before the invasion starts, as for a player at the table: a page that loads
    // mid-invasion has no start to summarise and toasts nothing for what already happened.
    const idle = invasionBoard({ phase: "", units: defenders, invasion: false });
    const game = await openInvasion(page, v, idle, null);
    update(game, invasionBoard({ phase: "bombardment", units: defenders }), 1);
    await page.waitForTimeout(300);

    update(game, invasionBoard({ phase: "commit_ground_forces", units: defendersAfterBombardment, step: bombardmentStep }), 2);
    await toastOrWait(page, /Bombardment/);
    await take(page, testInfo, "toast-bombardment", v);

    update(game, invasionBoard({ phase: "ground_battle", units: spaceCannonStep.after, step: spaceCannonStep }), 3);
    await toastOrWait(page, /Space Cannon/);
    await take(page, testInfo, "toast-space-cannon", v);

    update(game, invasionBoard({ phase: "planet_result", units: groundStep.after, step: groundStep }), 4);
    await page.waitForTimeout(400);
    update(game, invasionBoard({ phase: "planet_result", units: groundStep.after, controller: actor, invasion: false }), 5);
    await toastOrWait(page, /Invasion of/);
    await take(page, testInfo, "toast-summary", v);
  });
}
