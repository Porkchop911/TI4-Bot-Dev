import { test } from "@playwright/test";
import { openLanding, capture } from "./fixture";

test("4-no-trait", async ({ page }, testInfo) => {
  await openLanding(page, [
    { id: "jord", label: "Jord", resources: 4, influence: 2 },
    { id: "exhausted", label: "Exhausted World", resources: 0, influence: 0 },
  ]);
  await capture(page, testInfo, "4-no-trait");
});
