import { expect, test, type Page } from "@playwright/test";
import { shot } from "../_shared/shot";
import { DOCK_IDS, openScene, PHONES, tappable } from "./scene";

const panelOrder = async (page: Page) => {
  const items = page.getByTestId("toast-history-item");
  // Newest first: the last push (Tobi moving) leads, the first (Mira's card) ends.
  await expect(items).toHaveCount(4);
  await expect(items.first()).toContainText("Tobi");
  await expect(items.last()).toContainText("Mira");
};

test("desktop: open list, newest first, actor labels, badge cleared", async ({ page }, testInfo) => {
  await openScene(page, "desktop");
  await page.getByTestId("toast-history-toggle").click();
  await expect(page.getByTestId("toast-history-panel")).toBeVisible();
  await panelOrder(page);
  await expect(page.locator('[data-testid="toast-history-unread"]:visible')).toHaveCount(0);
  await shot(page, testInfo, "panel-desktop");
});

for (const size of PHONES)
  test(`phone: open list ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size);
    await page.getByTestId("toast-history-mobile-toggle").click();
    const panel = page.getByTestId("toast-history-panel");
    await expect(panel).toBeVisible();
    await panelOrder(page);
    const box = (await panel.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(page.viewportSize()!.width);
    expect(await tappable(page, "toast-history-mobile-toggle")).toBe(true);
    await shot(page, testInfo, `panel-${size}`);
  });

// The list opens above the Objectives sheet (and the other drawers), and every dock button stays
// tappable while a sheet is open.
for (const size of PHONES)
  for (const sheet of ["players", "objectives", "events"] as const) {
    test(`phone: list over the ${sheet} sheet ${size}`, async ({ page }, testInfo) => {
      await openScene(page, size);
      const toggle = { players: "player-sheet-toggle", objectives: "objectives-mobile-button", events: "event-log-mobile-toggle" }[sheet];
      await page.getByTestId(toggle).click();
      for (const id of DOCK_IDS) expect(await tappable(page, id), `${id} with ${sheet} open`).toBe(true);
      await page.getByTestId("toast-history-mobile-toggle").click();
      const panel = page.getByTestId("toast-history-panel");
      await expect(panel).toBeVisible();
      const onTop = await panel.evaluate((el) => {
        const r = el.getBoundingClientRect();
        const top = document.elementFromPoint(r.x + r.width / 2, r.y + 20);
        return top === el || el.contains(top);
      });
      expect(onTop, "the list is on top").toBe(true);
      await shot(page, testInfo, `panel-over-${sheet}-${size}`);
      // Switching to another sheet from the dock still works with the list open.
      await page.getByTestId("event-log-mobile-toggle").click();
      await expect(page.getByTestId("toast-history-panel")).toHaveCount(0);
    });
  }
