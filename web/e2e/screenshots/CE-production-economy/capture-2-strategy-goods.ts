import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { shot } from "../_shared/shot";
import { draftOptions } from "../AI-strategy-card-set/_draft";

const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

// The strategy draft with trade goods lying on three cards: each card's title row carries the
// count, so it is in view however long the card text is (the note under the text can scroll away).
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`strategy draft trade goods badge ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openMockedGame(page, {
      phase: "strategy",
      players: [playerWithHand({ strategy_cards: [] }), opponent],
      table: { strategy_card_goods: { pok5trade: 3, pok3politics: 1, pok8imperial: 2 } },
      choice: {
        prompt: "Select a strategy card",
        context: { subtype: "draft_strategy_card" },
        options: draftOptions("te"),
      },
    });
    await page.getByTestId("choice-option").first().waitFor();
    const trade = page.locator('[data-option-id="pok5trade"]');
    await expect(trade.getByTestId("strategy-goods-badge")).toHaveText("3 TG");
    await expect(page.locator('[data-option-id="pok1leadership"]').getByTestId("strategy-goods-badge")).toHaveText("0 TG");
    // The badge sits in the title row, at the top of its card, right of the name.
    const [card, badge] = await Promise.all([
      trade.evaluate((el) => el.getBoundingClientRect().toJSON()),
      trade.getByTestId("strategy-goods-badge").evaluate((el) => el.getBoundingClientRect().toJSON()),
    ]);
    expect(badge.top - card.top, "badge near the card's top").toBeLessThan(40);
    expect(badge.right, "badge inside the card").toBeLessThanOrEqual(card.right);
    await shot(page, testInfo, `2-strategy-goods-${size}`);
  });
}
