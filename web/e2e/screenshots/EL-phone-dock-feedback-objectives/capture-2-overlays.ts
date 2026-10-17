import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openScene, PORTRAIT, prefix, tappable, type SizeName, type Situation } from "./scene";

// Players opens as a sheet over the map with the dock still on top; Objectives was a modal dialog
// that covered the dock. Now (portrait phone) it is the same kind of sheet, and the buttons switch
// between and toggle Players / Objectives / Events. Landscape phone and desktop keep the dialog.
for (const size of [...PORTRAIT, "844x390", "desktop"] as SizeName[])
  for (const situation of ["turn", "decision"] as Situation[]) {
    test(`${prefix} players and objectives ${situation} ${size}`, async ({ page }, testInfo) => {
      await openScene(page, size, situation);
      const portrait = PORTRAIT.includes(size);
      if (portrait && situation === "turn") {
        await page.getByTestId("player-sheet-toggle").click();
        await expect(page.getByTestId("player-sheet-drawer")).toHaveClass(/app-shell__drawer--open/);
        await shot(page, testInfo, `${prefix}-players-${size}`);
        await page.getByTestId("player-sheet-toggle").click();
      }
      if (portrait) await page.getByTestId("objectives-mobile-button").evaluate((el) => (el as HTMLElement).click());
      else if (size === "desktop") await page.getByTestId("objectives-modal-button").evaluate((el) => (el as HTMLElement).click());
      else {
        await page.getByTestId("top-menu-button").click();
        await page.getByTestId("top-menu-sheet").waitFor();
        await page.getByTestId("objectives-modal-button").click();
      }
      await page.getByTestId("objectives-modal").waitFor();
      if (portrait && prefix === "after") {
        const sheet = (await page.getByTestId("objectives-modal").boundingBox())!;
        expect(sheet.y + sheet.height).toBeGreaterThanOrEqual(page.viewportSize()!.height - 1);
        // The dock stays on top of the sheet and is tappable, including Feedback.
        for (const id of ["player-sheet-toggle", "objectives-mobile-button", "event-log-mobile-toggle", "feedback-button"])
          expect(await tappable(page, id), id).toBe(true);
        await expect(page.getByTestId("objectives-mobile-button")).toHaveAttribute("aria-expanded", "true");
      }
      await shot(page, testInfo, `${prefix}-objectives-${situation}-${size}`);
      if (!portrait || prefix === "before") return;
      // Switch to Players: Objectives closes. Back: Players closes. Objectives again toggles off.
      await page.getByTestId("player-sheet-toggle").click();
      await expect(page.getByTestId("objectives-modal")).toHaveCount(0);
      await expect(page.getByTestId("player-sheet-drawer")).toHaveClass(/app-shell__drawer--open/);
      await page.getByTestId("objectives-mobile-button").click();
      await expect(page.getByTestId("player-sheet-drawer")).not.toHaveClass(/app-shell__drawer--open/);
      await expect(page.getByTestId("objectives-modal")).toBeVisible();
      await page.getByTestId("event-log-mobile-toggle").click();
      await expect(page.getByTestId("objectives-modal")).toHaveCount(0);
      await page.getByTestId("event-log-mobile-toggle").click();
      await page.getByTestId("objectives-mobile-button").click();
      await expect(page.getByTestId("objectives-modal")).toBeVisible();
      await page.getByTestId("objectives-mobile-button").click();
      await expect(page.getByTestId("objectives-modal")).toHaveCount(0);
    });
  }
