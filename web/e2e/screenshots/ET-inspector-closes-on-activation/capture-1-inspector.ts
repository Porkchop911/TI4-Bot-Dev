import { expect, test } from "@playwright/test";
import { PROTOCOL_VERSION } from "../../../src/protocol/types";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actor, systemActivationOptions } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { newPage, SIZES } from "../BS-production-phone/scene";

// A system opened to look at stays open when the next decision is "activate a system" and covers
// the map on a phone. TI4_SHOT_PREFIX=before is how the "before" shots were taken (on the base
// commit); the default "after" regenerates the current behaviour and asserts the inspector closed.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

for (const s of SIZES.filter((s) => s.label === "390x844" || s.label === "desktop")) {
  test(`${prefix} inspector vs activation ${s.label}`, async ({ browser, baseURL }, testInfo) => {
    const { context, page } = await newPage(browser, s, baseURL);
    const game = await openMockedGame(page, {
      players: [playerWithHand(), opponent],
    });
    await page.getByTestId("system-hex-26").waitFor();
    // Tap a system to look at it (one that is on screen).
    await page.getByTestId("system-hex-26").click();
    await page.getByTestId("system-inspector").waitFor();
    await shot(page, testInfo, `${prefix}-1-inspector-open-${s.label}`);

    const { events: _events, ...update } = game.snapshot;
    game.send({
      ...update,
      type: "state_update",
      protocol_version: PROTOCOL_VERSION,
      game_version: game.snapshot.game_version + 1,
      pending_choice: {
        nonce: "activation",
        choice: {
          player: actor,
          prompt: "Choose a system to activate",
          context: { subtype: "activate_system" },
          options: systemActivationOptions,
        },
      },
    });
    await page.getByTestId("system-activation-bar").waitFor();
    await page.waitForTimeout(300);
    if (prefix === "after" && s.device !== "desktop") {
      await expect(page.getByTestId("system-inspector")).toHaveCount(0);
    }
    await shot(page, testInfo, `${prefix}-2-activation-pending-${s.label}`);
    await context.close();
  });
}
