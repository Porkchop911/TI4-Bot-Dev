import { expect, type Page } from "@playwright/test";
import { boxes, overlaps, type Box, type Scene } from "./scenes";

export const COLLAPSED_MAX = 48;
export const EXPANDED_MAX = 0.4;

/**
 * The turn redo UI on a phone: strip at most 48 px, open sheet at most 40% of the viewport height,
 * neither over the header, the toolbar and seat legend, the Players / Events buttons, the turn
 * action bar or the decision dialog's primary buttons; Restore / Keep at least 44 px and reachable.
 */
export async function expectLayout(page: Page, scene: Scene, vp: { width: number; height: number }) {
  const all = await boxes(page);
  const pill = all.bar!;
  const sheet = all.sheet;
  expect(pill.height, "collapsed strip height").toBeLessThanOrEqual(COLLAPSED_MAX);
  if (sheet) {
    expect(sheet.height, "open sheet height").toBeLessThanOrEqual(vp.height * EXPANDED_MAX + 1);
    expect(sheet.y).toBeGreaterThanOrEqual(0);
    expect(sheet.y + sheet.height).toBeLessThanOrEqual(vp.height);
  }
  const keep: [string, Box | null][] = [
    ["header", all.header],
    ["toolbar", all.toolbar],
    ["seat legend", all.legend],
    ["Players / Events buttons", all.actions],
    ["turn action bar", all.turnBar],
    ["event log", all.log],
  ];
  for (const [label, box] of keep) {
    if (!box) continue;
    expect(overlaps(pill, box), `the strip must not cover the ${label}`).toBe(false);
    // On a short phone (740 px) with the turn action bar up, the map area between the strip and the
    // bar is under 50 px: the open sheet then grows upward over the map's own toolbar and seat
    // legend (never over the header, the bar, the Players / Events buttons or a dialog).
    const mapChrome = label === "toolbar" || label === "seat legend";
    if (sheet && !(mapChrome && vp.height < 800 && all.turnBar)) {
      expect(overlaps(sheet, box), `the open sheet must not cover the ${label}`).toBe(false);
    }
  }
  if (scene.choice) {
    // The decision dialog's primary buttons stay uncovered and are what a tap on them hits.
    const buttons = page.locator(".choice-dialog, .choice-workflow-dialog, .decision-modal, [role=dialog]").locator(".button--primary");
    const count = await buttons.count();
    for (let i = 0; i < count; i++) {
      const box = await buttons.nth(i).boundingBox();
      if (!box) continue;
      expect(overlaps(pill, box), "the strip must not cover a dialog button").toBe(false);
      // The dialog is modal and paints over the board's layer, so an open sheet can sit behind its
      // button; what counts is that the button is what a tap hits (below).
      const hit = await buttons.nth(i).evaluate((el) => {
        const r = el.getBoundingClientRect();
        const top = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
        return !!top && el.contains(top);
      });
      expect(hit, "the dialog button is on top").toBe(true);
    }
  }
  if (sheet && !scene.choice) {
    // Restore / Keep: at least 44 px tall, fully on screen, and what a tap on them hits.
    for (const id of ["turn-redo-restore", "turn-redo-keep"]) {
      const button = page.getByTestId(id);
      if (!(await button.count())) continue;
      const box = (await button.boundingBox())!;
      expect(box.height, `${id} height`).toBeGreaterThanOrEqual(44);
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(vp.width);
      expect(box.y + box.height).toBeLessThanOrEqual(vp.height);
      const hit = await button.evaluate((el) => {
        const r = el.getBoundingClientRect();
        const top = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
        return !!top && el.contains(top);
      });
      expect(hit, `${id} is reachable`).toBe(true);
    }
  }
}
