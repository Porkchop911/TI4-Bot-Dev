import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openReady, prefix } from "./scene";
import { opponent } from "../_shared/players";

// The other seat while the Diplomacy player decides: the slim waiting bar, read-only.
test("diplomacy ready: another seat watching", async ({ page }, testInfo) => {
  await openReady(page, { seat: opponent.id });
  await shot(page, testInfo, `${prefix}-6-other-seat`);
});
