import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { acceptingScript, chooseThePayment, openScene, prefix } from "./scene";

// Preparing Leadership: one token bought, the payment chosen on the map. Planets that can pay are
// ringed, the bar shows the bill covered by Jord + Quann instead of Auto-pay's Lodor. Before, the
// same screen existed but whatever was chosen here was dropped on Save (only the pools were kept);
// after, the choice is the plan and the engine checks it when it is saved.
test("leadership prep: paying on the map", async ({ page }, testInfo) => {
  await openScene(page, acceptingScript);
  await chooseThePayment(page);
  await shot(page, testInfo, `${prefix}-1-prepare-map-payment`);
});
