import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPickerOn } from "./real";

// 4 players: Outer Rim from the Map Atlas
test("community maps: 4 players, Outer Rim", async ({ page }, testInfo) => {
  await openPickerOn(page, 4, "4pOuterRim", "Outer Rim");
  await shot(page, testInfo, "3-four-outer-rim");
});
