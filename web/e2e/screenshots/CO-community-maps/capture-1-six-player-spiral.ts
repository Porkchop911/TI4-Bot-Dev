import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPickerOn, realMaps } from "./real";

// 6 players: the community layouts sit beside the upstream templates; Balanced Spiral chosen
test("community maps: 6 players, Balanced Spiral", async ({ page }, testInfo) => {
  const names = realMaps(6).templates.map((t) => t.name).filter(Boolean);
  expect(names).toEqual(expect.arrayContaining(["Balanced Spiral", "SCPT 2021 Final", "Won't You Be My Neighbor?"]));
  await openPickerOn(page, 6, "6pBalancedSpiral", "Balanced Spiral");
  await expect(page.getByTestId("map-card-6pStandard")).toContainText("Recommended");
  await shot(page, testInfo, "1-six-balanced-spiral");
});
