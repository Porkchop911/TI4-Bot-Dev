import { expect, type Locator, type Page } from "@playwright/test";

// What several suites of the real game do the same way.

/**
 * Takes a tactical action in Live, as a player does: the picker opens it as the private draft,
 * the system is chosen on the map and recorded there, and "Apply to Live" sends the action and
 * the system. The steps after the activation are then open in Live. On a phone the system is
 * chosen on the Map pane, which stays in view.
 */
export async function activateInLive(page: Page, target: Locator) {
  const panel = page.locator("#step-panel");
  const heading = panel.getByRole("heading").first();
  const panes = page.getByRole("navigation", { name: "Panes" });
  // The phone layout: the shell shows one pane at a time.
  const phone = (page.viewportSize()?.width ?? 1920) < 600;
  if (phone) {
    await panes.getByRole("button", { name: "Action" }).click();
  }
  await panel.getByRole("button", { name: /^(Activate a system|Open your draft)$/ }).click();
  await expect(heading).toHaveText("Activation");
  if (phone) {
    await panes.getByRole("button", { name: "Map" }).click();
  }
  // The systems are the choice once the engine answered the draft.
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);
  await target.click();
  await page.locator('footer button[aria-keyshortcuts="Enter"]').click();
  // The heading of the step is on the Action pane.
  if (phone) {
    await panes.getByRole("button", { name: "Action" }).click();
  }
  await expect(heading).toHaveText("Movement");
  await page.getByRole("button", { name: "Apply to Live" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Apply to Live" }).click();
  await expect(
    page.getByRole("navigation", { name: "Workspace" }).getByRole("button", { name: "Live" }),
  ).toHaveAttribute("aria-pressed", "true");
  if (phone) {
    await panes.getByRole("button", { name: "Map" }).click();
  }
}
