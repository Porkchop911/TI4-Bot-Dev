import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { DEVICES, newDevicePage, openScene, prefix, probe, type DeviceName, type Situation } from "./scene";

// The same game on each device. "before" forces the desktop layout, which is what a phone in
// desktop-site mode got before the detection; "after" is the automatic decision.
const cases: Array<[DeviceName, Situation, boolean]> = [
  ["mobile", "your-turn", true],
  ["mobile-landscape", "your-turn", true],
  ["desktop-site", "your-turn", true],
  ["desktop-site", "waiting", true],
  ["desktop-site", "decision", true],
  ["ipad", "your-turn", false],
  ["desktop", "your-turn", false],
];

for (const [device, situation, compact] of cases) {
  test(`${prefix} ${device} ${situation}`, async ({ browser, baseURL }, testInfo) => {
    test.skip(prefix === "before" && device !== "desktop-site", "only the desktop-site case differs");
    const { context, page } = await newDevicePage(browser, device, baseURL);
    await openScene(page, situation, prefix === "before" ? "desktop" : undefined);
    const p = await probe(page);
    console.log(`PROBE ${prefix} ${device} ${situation} ${JSON.stringify(p)}`);
    if (prefix === "after") {
      expect(p.layout).toBe(compact ? "compact" : "desktop");
      expect(p.topMenuButton).toBe(compact);
      if (compact) expect(p.headerH).toBeLessThanOrEqual(41);
    }
    await shot(page, testInfo, `${prefix}-${device}-${situation}`);
    await context.close();
  });
}
