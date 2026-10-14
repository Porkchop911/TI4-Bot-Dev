import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { mockFeedbackEndpoint, openPublicGame, type SizeName } from "./scene";

for (const size of ["desktop", "phone"] as SizeName[]) {
  test(`feedback sheet ${size}`, async ({ page }, testInfo) => {
    const received = await mockFeedbackEndpoint(page);
    await openPublicGame(page, size);
    await page.getByTestId("feedback-button").click();
    const sheet = page.getByTestId("feedback-sheet");
    await expect(sheet).toBeVisible();
    await expect(page.getByTestId("feedback-submit")).toBeDisabled();
    await expect(page.getByTestId("feedback-include-game")).toBeChecked();
    const { width, height } = page.viewportSize()!;
    const box = (await sheet.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(width + 1);
    expect(box.y + box.height).toBeLessThanOrEqual(height + 1);
    for (const id of ["feedback-submit", "feedback-close", "feedback-category-bug"]) {
      expect((await page.getByTestId(id).boundingBox())!.height).toBeGreaterThanOrEqual(44);
    }
    await shot(page, testInfo, `sheet-empty-${size}`);

    await page.getByTestId("feedback-category-confusing").click();
    await page.getByTestId("feedback-text").fill(
      "I did not understand why the Pass button was greyed out after I activated the second system. I expected to be able to end my turn.",
    );
    await expect(page.getByTestId("feedback-submit")).toBeEnabled();
    await shot(page, testInfo, `sheet-filled-${size}`);

    await page.getByTestId("feedback-submit").click();
    await expect(page.getByTestId("feedback-sent")).toBeVisible();
    await expect(page.getByTestId("feedback-id")).toContainText("fb_1760268000_9f3a1c2e");
    expect(received).toHaveLength(1);
    expect(received[0].category).toBe("confusing");
    const context = received[0].context as Record<string, unknown>;
    expect(context).toMatchObject({ build_commit: "51390594", viewer_seat: expect.any(String) });
    expect(JSON.stringify(received[0])).not.toContain("screenshot-session");
    await shot(page, testInfo, `sheet-sent-${size}`);
  });
}
