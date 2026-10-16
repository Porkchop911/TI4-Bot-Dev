import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, actor } from "../_shared/players";
import { shot } from "../_shared/shot";
import { productionChoice } from "../Y-produce-units-inline-stats/fixtures";
import { publicEntry, pushEntry } from "../G-corner-notifications/corner";

const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

// The viewer has 4 trade goods when the page opens; Trade's primary then brings them to 7. The
// production dialog opened afterwards splits its 9 resources into planets and trade goods and says
// where the trade goods came from. Its unit cards are one column wide even on a wide dialog.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`production dialog planets vs trade goods ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    const game = await openMockedGame(page, {
      phase: "action",
      players: [playerWithHand({ trade_goods: 4 }), opponent],
    });
    await page.getByTestId("ti4-board-svg").waitFor();
    pushEntry(game, publicEntry("trade-1", actor, "resolved the Trade strategy card: gained 3 trade goods"));
    const { events: _events, ...update } = game.snapshot;
    const choice = productionChoice(9);
    game.send({
      ...update,
      type: "state_update",
      game_version: 41,
      view: {
        ...update.view,
        players: [playerWithHand({ trade_goods: 7 }), opponent],
      },
      pending_choice: { nonce: "prod-1", choice: { player: actor, ...choice } },
    });
    const drawer = page.getByTestId("production-builder-drawer");
    await drawer.waitFor();
    await expect(page.getByTestId("production-resources-counter")).toContainText("0 / 9");
    await expect(page.getByTestId("production-split-planets")).toHaveText("2");
    await expect(page.getByTestId("production-split-tg")).toHaveText("7");
    const sources = page.getByTestId("trade-good-sources");
    await expect(sources).toContainText("+3");
    await expect(sources).toContainText("Trade strategy card");
    await expect(sources).toContainText("now 7");
    // One column: every unit card has the same left edge.
    const lefts = await page
      .locator('[data-testid^="produce-option-"]')
      .evaluateAll((els) => new Set(els.map((el) => Math.round(el.getBoundingClientRect().left))).size);
    expect(lefts, "distinct left edges of unit cards").toBe(1);
    await shot(page, testInfo, `1-production-split-${size}`);
  });
}
