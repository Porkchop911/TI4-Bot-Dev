import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPickerOn } from "./real";

// 5 players: the official Prophecy of Kings learn-to-play layout with its hyperlanes
test("community maps: 5 players, Learn to Play (PoK)", async ({ page }, testInfo) => {
  const card = await openPickerOn(page, 5, "5pLearnToPlayPoK", "Learn to Play (PoK)");
  await expect(card).toContainText("Hyperlanes");
  await expect(page.getByTestId("map-picker-summary")).toContainText("with hyperlanes");
  await shot(page, testInfo, "2-five-learn-to-play");
});
