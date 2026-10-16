import { expect, test, type Page } from "@playwright/test";
import { PROTOCOL_VERSION } from "../../../src/protocol/types";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { answerChoice, proposeChoice, tradePlayers, PARTNER, proposeOptions, click } from "../AG-trade-staging-desk/_trade";

// Before/after of the trade desk's sticky action bar. TI4_SHOT_PREFIX=before is how the committed
// "before" shots were taken (on the base commit, unified-2026-10-08); the default "after"
// regenerates the current layout and asserts that nothing shows below the bar.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
const sizes = [
  { id: "desktop", width: 1280, height: 720 },
  { id: "phone", width: 390, height: 844 },
];

const modalOf = (page: Page) => page.locator(".trade-dialog .choice-workflow-modal");

/** Scrolls the dialog to its end, then checks the bar is flush with the container and nothing sits below it. */
async function expectBarFlush(page: Page, actionsTestId: string) {
  const modal = modalOf(page);
  await modal.evaluate((el) => el.scrollTo(0, el.scrollHeight));
  await page.waitForTimeout(100);
  const m = await modal.evaluate((el) => ({ clientH: el.clientHeight, scrollH: el.scrollHeight, border: parseFloat(getComputedStyle(el).borderBottomWidth) }));
  const box = (await modal.boundingBox())!;
  const bar = (await page.getByTestId(actionsTestId).boundingBox())!;
  const containerBottom = box.y + box.height - m.border;
  expect(m.scrollH).toBeGreaterThan(m.clientH); // the deal list really scrolls
  // The bar's bottom edge is the container's bottom edge: no strip of desk content below it.
  expect(Math.abs(bar.y + bar.height - containerBottom)).toBeLessThanOrEqual(1);
  // The last card above the bar is fully visible: every sibling ends at or above the bar's top.
  const overlap = await page.evaluate((id) => {
    const barEl = document.querySelector(`[data-testid="${id}"]`)!;
    const barTop = barEl.getBoundingClientRect().top;
    let worst = -Infinity;
    for (const child of Array.from(barEl.parentElement!.children)) {
      if (child === barEl) continue;
      const r = child.getBoundingClientRect();
      if (r.height > 0) worst = Math.max(worst, r.bottom - barTop);
    }
    return worst;
  }, actionsTestId);
  expect(overlap).toBeLessThanOrEqual(1);
}

/** Scrolls to just short of the end, where content continues behind (or, before the fix, below) the bar. */
const scrollNearEnd = (page: Page) =>
  modalOf(page).evaluate((el) => el.scrollTo(0, Math.max(0, el.scrollHeight - el.clientHeight - 120)));

async function stageLongDeal(page: Page) {
  for (let i = 0; i < 2; i++) await click(page, "stage-give-tg-inc");
  for (let i = 0; i < 3; i++) await click(page, "stage-receive-tg-inc");
  await click(page, "stage-give-ac-bribery");
  await click(page, "stage-receive-note-political_secret:jolnar");
  await page.locator(".trade-quick > summary").click(); // the long catalogue of listed deals
  await page.getByTestId("stage-status").waitFor();
}

for (const size of sizes) {
  test(`trade desk footer: ${size.id} propose`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: size.width, height: size.height });
    await openMockedGame(page, { players: tradePlayers, choice: proposeChoice });
    await page.getByTestId("trade-desk-modal").waitFor();
    await stageLongDeal(page);
    if (prefix === "after") await expectBarFlush(page, "trade-propose-actions");
    await scrollNearEnd(page);
    await shot(page, testInfo, `${prefix}-${size.id}-propose`);
  });

  test(`trade desk footer: ${size.id} answer`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: size.width, height: size.height });
    await openMockedGame(page, { players: tradePlayers, choice: answerChoice as never });
    await page.getByTestId("offer-net").waitFor();
    const modal = modalOf(page);
    await modal.evaluate((el) => el.scrollTo(0, el.scrollHeight));
    if (prefix === "after" && (await modal.evaluate((el) => el.scrollHeight > el.clientHeight))) {
      await expectBarFlush(page, "trade-answer-actions");
    }
    await scrollNearEnd(page);
    await shot(page, testInfo, `${prefix}-${size.id}-answer`);
  });

  test(`trade desk footer: ${size.id} counter prefill`, async ({ page }) => {
    test.skip(prefix !== "after", "assertion-only check");
    await page.setViewportSize({ width: size.width, height: size.height });
    const game = await openMockedGame(page, { players: tradePlayers, choice: answerChoice as never });
    await click(page, "answer-opt-counter");
    const { events: _events, ...update } = game.snapshot;
    game.send({
      ...update,
      type: "state_update",
      protocol_version: PROTOCOL_VERSION,
      game_version: game.snapshot.game_version + 1,
      pending_choice: {
        nonce: "trade-counter",
        choice: {
          player: game.snapshot.viewer.role === "player" ? game.snapshot.viewer.seat : "",
          prompt: "transaction with Jolnar",
          context: { subtype: "propose_transaction", target: { Player: PARTNER } },
          options: proposeOptions,
        },
      },
    });
    await page.getByTestId("counter-prefill-note").waitFor();
    await page.locator(".trade-quick > summary").click();
    await expectBarFlush(page, "trade-propose-actions");
  });
}
