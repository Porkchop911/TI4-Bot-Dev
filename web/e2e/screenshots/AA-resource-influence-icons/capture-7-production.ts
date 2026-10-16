import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";

const choice = {
  prompt: "produce in 18 (3 left)",
  context: {
    subtype: "produce_unit",
    target: { System: "18" },
    outstanding: [{ kind: "production_capacity", amount: 3, paid: 0 }],
  },
  options: [
    { id: "build|fighter|1", kind: "produce", label: "produce 1x fighter for 1", payload: { unit: "fighter", count: 1, cost: 1, available_resources: 5 } },
    { id: "build|carrier|1", kind: "produce", label: "produce 1x carrier for 3", payload: { unit: "carrier", count: 1, cost: 3, available_resources: 5 } },
    { id: "done_producing", kind: "decline", label: "produce nothing further" },
  ],
};

// Production: the resource budget meter reads "staged / available [icon] (n Left)".
test("production builder budget", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice });
  const drawer = page.getByTestId("production-builder-drawer");
  await drawer.waitFor();
  await page.getByTestId("produce-unit-btn-build|carrier|1").click();
  await shot(page, testInfo, "7a-production-budget", { of: drawer, pad: 8 });
});

test("production builder on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice });
  await page.getByTestId("production-builder-drawer").waitFor();
  await page.getByTestId("produce-unit-btn-build|carrier|1").click();
  await shot(page, testInfo, "7b-production-phone");
});
