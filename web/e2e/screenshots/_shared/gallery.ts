import { expect, type Page } from "@playwright/test";

/** Opens one decision gallery case (synthetic previews rendered by the real GameShell). */
export async function openGalleryCase(page: Page, title: string | RegExp) {
  await page.goto("/dev/decisions");
  const name = typeof title === "string" ? new RegExp(`^${title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`) : title;
  await page.getByRole("button", { name }).first().click();
  await expect(page.getByTestId("game-container")).toBeAttached();
}
