import { test } from "@playwright/test";
import { openInspector, capture, mine, theirs, ground } from "./scene";

// An opponent's PDS shoots at the viewer's landing on the same planet: it shows with its own colour and seat badge.
test("3-opponent", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 1100 });
  await openInspector(page, [
    ground("jord"),
    mine("jord", "spacedock"),
    theirs("exhausted", "pds"),
    theirs("exhausted", "infantry"),
  ]);
  await capture(page, testInfo, "3-opponent");
});
