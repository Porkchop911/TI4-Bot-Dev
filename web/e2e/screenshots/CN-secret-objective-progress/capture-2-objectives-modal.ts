import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openScene, SIZES, type SizeName } from "./scene";

// The Objectives dialog outside a scoring window now lists the viewer's own secrets under the
// public ones, with the same progress line.
for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`objectives dialog ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size);
    await page
      .getByTestId(size === "desktop" ? "objectives-modal-button" : "objectives-mobile-button")
      .click();
    await page.getByTestId("objectives-modal").waitFor();
    await expect(page.getByTestId("secret-objectives-header")).toBeVisible();
    await expect(page.getByTestId("objectives-modal").getByTestId("secret-progress-ctr")).toContainText("4 / 6");
    await expect(page.getByTestId("secret-held-held-pe")).toContainText("In your hand");
    await page.getByTestId("secret-objectives-header").scrollIntoViewIfNeeded();
    await shot(page, testInfo, `2-objectives-${size}`);
  });
}

// In the status scoring window: Establish a Perimeter is offered; the rest of the hand shows its
// progress beside "not scoreable now".
for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`scoring window ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size, {
      prompt: "score an objective",
      context: { subtype: "score_objective", source: { Rule: "61.6" } },
      options: [
        { id: "lead", label: "lead", kind: "score" },
        { id: "eap", label: "eap", kind: "score" },
        { id: "decline", label: "decline", kind: "decline" },
      ],
    });
    await page.getByTestId("objectives-modal").waitFor();
    await expect(page.getByTestId("secret-held-held-ctr")).toContainText("not scoreable now");
    await expect(page.getByTestId("objectives-modal").getByTestId("secret-progress-ctr")).toContainText("4 / 6");
    await page.getByTestId("secret-row-held-hrm").scrollIntoViewIfNeeded();
    await shot(page, testInfo, `3-scoring-${size}`);
  });
}
