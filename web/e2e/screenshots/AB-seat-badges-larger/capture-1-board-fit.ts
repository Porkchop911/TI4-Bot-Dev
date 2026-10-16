import { expect, test, type Page } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame, type MockGameOptions } from "../_shared/mockGame";
import { galleryDecision, systemActivationOptions } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Before/after of the larger seat badges and the reserved toolbar / prompt space around the map.
// TI4_SHOT_PREFIX=before is how the committed "before" shots were taken (on the base commit,
// upstream-pr-2026-10-07); the default "after" regenerates the current layout.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const viewports = [
  { id: "1440x900", width: 1440, height: 900 },
  { id: "1280x720", width: 1280, height: 720 },
  { id: "1024x768", width: 1024, height: 768 },
  { id: "390x844", width: 390, height: 844 },
];

const scenarios: { id: string; options: () => MockGameOptions; wait?: string }[] = [
  { id: "standard", options: () => ({ players: [playerWithHand(), opponent] }) },
  {
    id: "system-selection",
    wait: "system-activation-bar",
    options: () => ({
      players: [playerWithHand(), opponent],
      choice: { prompt: "Choose a system to activate", context: { subtype: "activate_system" }, options: systemActivationOptions },
    }),
  },
  {
    id: "planet-selection",
    wait: "planet-selection-bar",
    options: () => ({ players: [playerWithHand(), opponent], choice: galleryDecision("planet selection") }),
  },
  {
    id: "movement",
    options: () => ({ players: [playerWithHand(), opponent], choice: galleryDecision("Empty movement") }),
  },
];

/** No hex may sit under the toolbar / legend row, under the prompt pill, or outside the board area. */
async function expectMapClear(page: Page, pillTestId?: string) {
  const box = async (selector: string) => page.locator(selector).first().boundingBox();
  const chrome = (await box('[data-testid="board-chrome"]'))!;
  const stage = (await box('[data-testid="board-stage"]'))!;
  const pill = pillTestId ? await box(`[data-testid="${pillTestId}"]`) : null;
  const hexes = await page.locator('[data-testid^="system-hex-"]').evaluateAll((els) =>
    els.map((el) => {
      const r = el.getBoundingClientRect();
      return { top: r.top, bottom: r.bottom };
    }),
  );
  const top = Math.min(...hexes.map((h) => h.top));
  const bottom = Math.max(...hexes.map((h) => h.bottom));
  expect(top, "top tile row below toolbar and legend").toBeGreaterThanOrEqual(chrome.y + chrome.height - 1);
  expect(bottom, "bottom tile row inside the stage").toBeLessThanOrEqual(stage.y + stage.height + 1);
  if (pill) expect(bottom, "bottom tile row above the prompt pill").toBeLessThanOrEqual(pill.y + 1);
}

for (const viewport of viewports) {
  for (const scenario of scenarios) {
    test(`${prefix} ${viewport.id} ${scenario.id}`, async ({ page }, testInfo) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await openMockedGame(page, scenario.options());
      if (scenario.wait) await page.getByTestId(scenario.wait).waitFor();
      await page.getByTestId("ti4-board-svg").waitFor();
      if (prefix === "after") await expectMapClear(page, scenario.wait);
      await shot(page, testInfo, `${prefix}-${viewport.id}-${scenario.id}`);
    });
  }
}

test(`${prefix} legend`, async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await page.getByTestId("ti4-board-svg").waitFor();
  await shot(page, testInfo, `${prefix}-legend`, { of: page.locator(".board-seat-legend"), pad: 12 });
});
