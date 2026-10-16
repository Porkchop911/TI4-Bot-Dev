import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { advanceTo, agendaVisibility, newDevicePage, prefix, substateAgenda, substateOptions, SUBSTATES, type DeviceName } from "./scene";

// Every agenda sub-state on the phone sizes (portrait, small portrait, landscape, desktop site):
// is the agenda on screen and uncovered? "after" asserts it; "before" only logs it.
const devices: Array<[string, DeviceName, { width: number; height: number } | null]> = [
  ["390x844", "mobile", null],
  ["360x740", "mobile", { width: 360, height: 740 }],
  ["844x390", "mobile-landscape", null],
  ["desktop-site", "desktop-site", null],
];

for (const state of SUBSTATES) {
  for (const [label, device, size] of devices) {
    test(`${prefix} ${state} ${label}`, async ({ browser, baseURL }, testInfo) => {
      const { context, page } = await newDevicePage(browser, device, baseURL);
      if (size) await page.setViewportSize(size);
      // A rider's prediction names no agenda itself: the phase's memory (from the reveal) carries it.
      const game = await openMockedGame(page, substateOptions(state === "predict" ? "reveal-reaction" : state));
      await page.getByTestId("ti4-board-svg").waitFor();
      if (state === "predict") await advanceTo(game, "predict", 41);
      await page.waitForTimeout(600);
      const v = await agendaVisibility(page, substateAgenda(state).name);
      console.log(`VISIBLE ${prefix} ${state} ${label} ${JSON.stringify(v)}`);
      await shot(page, testInfo, `${prefix}-${state}-${label}`);
      if (prefix === "after") {
        expect(v.inViewport, "agenda name inside the viewport").toBe(true);
        expect(v.uncovered, "agenda name not covered").toBe(true);
      }
      await context.close();
    });
  }
}
