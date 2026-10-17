import { expect, test } from "@playwright/test";
import { openResearch } from "./scene";

// Desktop keeps the roster, the descriptions and the skips hint.
test("desktop keeps roster and descriptions", async ({ page }) => {
  await openResearch(page, "desktop", { card: "primary", planets: true, tradeGoods: 0 });
  await expect(page.getByTestId("tech-modal-roster")).toBeVisible();
  await expect(page.getByTestId("tech-effect-amd")).toBeVisible();
  await expect(page.locator(".technology-modal__skips-hint")).toBeVisible();
});
