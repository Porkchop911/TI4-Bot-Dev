import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES } from "./mapSelect";
import { openOffline } from "../DN-offline-shell/offline";

// The picker's data states, with the in-memory launcher (the engine answers slowly, fails, or
// knows no layout for the table).
for (const [state, shotName] of [
  ["loading", "8-picker-loading"],
  ["error", "9-picker-error"],
  ["empty", "10-picker-empty"],
] as const) {
  for (const [size, viewport] of Object.entries(SIZES)) {
    test(`picker ${state} ${size}`, async ({ page }, testInfo) => {
      await page.setViewportSize(viewport);
      await openOffline(page, "/offline", { mapsState: state });
      await page.getByTestId("offline-nickname").fill("Ada");
      await page.getByTestId("offline-map-button").click();
      await expect(
        page.getByTestId(state === "loading" ? "map-picker-loading" : state === "error" ? "map-picker-error" : "map-picker-empty"),
      ).toBeVisible();
      await shot(page, testInfo, `${shotName}-${size}`);
    });
  }
}
