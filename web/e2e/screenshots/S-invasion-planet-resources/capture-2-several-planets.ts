import { test } from "@playwright/test";
import { openLanding, capture } from "./fixture";

test("2-several-planets", async ({ page }, testInfo) => {
  await openLanding(page, [
    { id: "jord", label: "Jord", resources: 2, influence: 2, traits: ["cultural"] },
    { id: "exhausted", label: "Exhausted World", resources: 1, influence: 3, traits: ["industrial"], attachments: ["dmz"] },
    { id: "quann", label: "Quann", resources: 2, influence: 1 },
  ]);
  await capture(page, testInfo, "2-several-planets");
});
