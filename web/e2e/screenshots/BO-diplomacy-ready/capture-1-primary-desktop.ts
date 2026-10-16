import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReady, prefix } from "./scene";

// Diplomacy primary or secondary on desktop: "ready which planet" as it opens (nothing picked).
// Before: a one-planet bar (the engine asks again for the second). After: every exhausted planet at once.
test("diplomacy ready: bar as opened", async ({ page }, testInfo) => {
  await openReady(page);
  await shot(page, testInfo, `${prefix}-1-desktop-opened`);
});
