import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPickerOn } from "./real";

// 3 players: Three's Company, homes on the second ring and hyperlanes outside
test("community maps: 3 players, Three's Company", async ({ page }, testInfo) => {
  const card = await openPickerOn(page, 3, "3pThreesCompany", "Three's Company");
  await expect(card).toContainText("Hyperlanes");
  await shot(page, testInfo, "4-three-threes-company");
});
