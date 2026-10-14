import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { measureTop, openScene, pickOverlay, prefix, type Situation, type SizeName } from "./scene";

// The top of the screen with the menu closed: every size and situation. MEASURE lines give the
// heights (status header, board toolbar row, where the map starts).
const cases: Array<[SizeName, Situation]> = [
  ["390x844", "waiting"],
  ["390x844", "your-turn"],
  ["390x844", "decision"],
  ["360x740", "waiting"],
  ["360x740", "your-turn"],
  ["844x390", "waiting"],
  ["844x390", "your-turn"],
  ["980x740", "waiting"],
  ["1440x900", "your-turn"],
];

test.describe.configure({ retries: 2 });

for (const [size, situation] of cases) {
  test(`top ${situation} ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size, situation);
    const m = await measureTop(page);
    console.log(`MEASURE ${prefix} ${situation} ${size} ${JSON.stringify(m)}`);
    await shot(page, testInfo, `${prefix}-${situation}-${size}`);
  });
}

test("view economy 390x844", async ({ page }, testInfo) => {
  await openScene(page, "390x844", "waiting");
  await pickOverlay(page, "economy");
  await shot(page, testInfo, `${prefix}-view-economy-390x844`);
});
