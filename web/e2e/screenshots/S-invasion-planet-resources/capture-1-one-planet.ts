import { test } from "@playwright/test";
import { openLanding, capture } from "./fixture";

test("1-one-planet", async ({ page }, testInfo) => {
  await openLanding(page, [{ id: "jord", label: "Jord", resources: 2, influence: 2, traits: ["cultural"] }]);
  await capture(page, testInfo, "1-one-planet");
});
