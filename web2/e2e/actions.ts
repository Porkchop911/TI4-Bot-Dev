import { expect, type Locator, type Page } from "@playwright/test";

// What several suites of the real game do the same way.

/** Collects the errors of the page, and with `console` the errors that it logs: a test of the real game expects none. */
export function pageErrors(page: Page, { console = false } = {}): string[] {
  const errors: string[] = [];
  if (console) {
    page.on("console", (message) => message.type() === "error" && errors.push(message.text()));
  }
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

/** The row of a seat in the player table ("Seat A"). */
export const seatRow = (page: Page, seat: string) =>
  page.getByRole("region", { name: "Players" }).getByRole("row").filter({ hasText: seat });

/** A planet of the map, by its name. */
export const mapPlanet = (page: Page, name: string) =>
  page.locator("g.planet").filter({ hasText: name });

/**
 * Takes a tactical action in Live, as a player does: the picker opens it as the private draft,
 * the system is chosen on the map and recorded there, and "Apply to Live" sends the action and
 * the system. The steps after the activation are then open in Live. On a phone the picker opens
 * the Map pane, where the system is chosen.
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
  if (phone) {
    // The activation is on the board: the choice of the player opens the map.
    await expect(panes.getByRole("button", { name: "Map" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  } else {
    await expect(heading).toHaveText("Activation");
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
