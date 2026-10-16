import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPrimary, prefix, SIZES, type SizeName } from "./scene";

// Technology primary with 4 or 6 resources: the first pick is free, a second costs 6. The `before`
// run (commit without the fix) only takes the pictures; the after run also asserts the behaviour.
test.describe.configure({ retries: 2 });

for (const size of Object.keys(SIZES) as SizeName[]) {
  test(`four resources, one pick ${size}`, async ({ page }, testInfo) => {
    await openPrimary(page, size, 4);
    await expect(page.getByTestId("tech-available-resources")).toContainText("4");
    await page.getByTestId("tech-card-amd").click();
    if (prefix === "before") {
      // Before the fix a second card could be picked; show that state.
      await page.getByTestId("tech-card-ps").click();
      await shot(page, testInfo, `${prefix}-four-${size}`);
      return;
    }
    await expect(page.getByTestId("tech-card-ps")).toHaveAttribute("data-selectable", "false");
    await expect(page.getByTestId("tech-card-ps")).toHaveAttribute(
      "data-blocked-reason",
      "Second technology costs 6 resources, you have 4",
    );
    await expect(page.getByTestId("tech-second-pick-hint")).toContainText("A second technology costs");
    await expect(page.getByTestId("confirm-research-btn")).toBeEnabled();
    // Clicking a dimmed card changes nothing.
    await page.getByTestId("tech-card-ps").dispatchEvent("click");
    await expect(page.getByTestId("tech-card-amd")).toHaveAttribute("data-selected", "true");
    await expect(page.getByTestId("tech-card-ps")).toHaveAttribute("data-selected", "false");
    const { width, height } = SIZES[size];
    for (const id of ["confirm-research-btn", "tech-second-pick-hint"]) {
      const box = (await page.getByTestId(id).boundingBox())!;
      expect(box.x + box.width, id).toBeLessThanOrEqual(width);
      expect(box.y + box.height, id).toBeLessThanOrEqual(height);
    }
    // Phone targets are 44 px; the desktop buttons keep their 36 px.
    if (width < 720) expect((await page.getByTestId("confirm-research-btn").boundingBox())!.height).toBeGreaterThanOrEqual(44);
    await shot(page, testInfo, `${prefix}-four-${size}`);
  });

  test(`six resources, two picks ${size}`, async ({ page }, testInfo) => {
    await openPrimary(page, size, 6);
    await page.getByTestId("tech-card-amd").click();
    await expect(page.getByTestId("tech-card-ps")).toHaveAttribute("data-selectable", "true");
    await page.getByTestId("tech-card-ps").click();
    await expect(page.getByTestId("confirm-research-btn")).toBeEnabled();
    if (prefix === "before") {
      await shot(page, testInfo, `${prefix}-six-${size}`);
      return;
    }
    await expect(page.getByTestId("tech-second-pick-hint")).toHaveCount(0);
    const clear = page.getByTestId("tech-clear-second");
    expect((await clear.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    await shot(page, testInfo, `${prefix}-six-${size}`);
    await clear.click();
    await expect(page.getByTestId("tech-card-ps")).toHaveAttribute("data-selected", "false");
    await expect(page.getByTestId("tech-card-amd")).toHaveAttribute("data-selected", "true");
  });
}
