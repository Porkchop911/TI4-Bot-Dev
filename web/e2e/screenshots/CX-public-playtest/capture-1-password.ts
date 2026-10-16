import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPasswordScreen, type SizeName } from "./scene";

for (const size of ["desktop", "phone"] as SizeName[]) {
  test(`password screen ${size}`, async ({ page }, testInfo) => {
    await openPasswordScreen(page, size);
    const input = page.getByTestId("password-input");
    await expect(page.getByRole("heading", { name: "Enter the playtest password" })).toBeVisible();
    await expect(input).toBeFocused();
    // 16px text so iOS does not zoom, and a 44px+ target for the button.
    expect(await input.evaluate((el) => parseFloat(getComputedStyle(el).fontSize))).toBeGreaterThanOrEqual(16);
    const submit = (await page.getByTestId("password-submit").boundingBox())!;
    expect(submit.height).toBeGreaterThanOrEqual(44);
    const vw = page.viewportSize()!.width;
    expect(submit.x).toBeGreaterThanOrEqual(0);
    expect(submit.x + submit.width).toBeLessThanOrEqual(vw);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await shot(page, testInfo, `password-${size}`);
  });
}

test("wrong password phone", async ({ page }, testInfo) => {
  await openPasswordScreen(page, "phone");
  await page.getByTestId("password-input").fill("not-it");
  await page.getByTestId("password-submit").click();
  await expect(page.getByTestId("password-error")).toContainText("not right");
  await shot(page, testInfo, "password-wrong-phone");
});
