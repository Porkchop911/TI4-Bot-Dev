import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openTurn } from "./scene";

/** An AudioContext that counts the beeps instead of playing them. */
const COUNT_BEEPS = () => {
  (window as unknown as { __beeps: number }).__beeps = 0;
  class Fake {
    state = "running";
    currentTime = 0;
    destination = {};
    resume() {}
    createOscillator() {
      (window as unknown as { __beeps: number }).__beeps++;
      return { connect() {}, start() {}, stop() {}, frequency: {}, type: "sine" };
    }
    createGain() {
      return { connect() {}, gain: { setValueAtTime() {}, exponentialRampToValueAtTime() {} } };
    }
  }
  (window as unknown as { AudioContext: unknown }).AudioContext = Fake;
};
const beeps = (page: import("@playwright/test").Page) =>
  page.evaluate(() => (window as unknown as { __beeps: number }).__beeps);

test("one beep when the decision reaches you, none for the next ones of the turn; mute is a labelled button", async ({ page }, testInfo) => {
  await page.addInitScript(COUNT_BEEPS);
  const game = await openTurn(page, { width: 1280, height: 700 });
  await expect.poll(() => beeps(page)).toBe(1);
  const { events: _e, ...base } = game.snapshot;
  for (const [n, version] of [["next-1", 41], ["next-2", 42]] as const) {
    game.send({
      ...base,
      type: "state_update",
      game_version: version,
      pending_choice: { ...base.pending_choice!, nonce: n },
    });
    await page.waitForTimeout(300);
  }
  expect(await beeps(page)).toBe(1);
  const button = page.getByTestId("turn-sound-btn");
  await expect(button).toHaveText(/Sound on/);
  await button.click();
  await expect(button).toHaveText(/Sound off/);
  await shot(page, testInfo, "after-sound-button", { of: page.getByTestId("player-sheet-panel").locator("h2").locator(".."), pad: 6 });
});
