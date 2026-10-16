import { expect, test, type Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";
import { prefix, SIZES, type SizeName } from "./scene";

// Nightly 2026-10-08, run 30: the Dreadnought row of the production builder lay behind the sticky
// Reset / Confirm / Done bar. Every unit row and its - / + buttons must be reachable by scrolling the
// dialog, with nothing from the footer over them once it is scrolled to the end.
const unit = (id: string, cost: number, extra: Record<string, unknown> = {}) => ({
  id: `build|${id}|1`,
  kind: "produce",
  label: `produce 1x ${id} for ${cost}`,
  payload: { unit: id, count: 1, cost, available_resources: 12, ...extra },
});
const choice = {
  prompt: "produce in 14 (4 left)",
  context: { subtype: "produce_unit", target: { System: "14" }, outstanding: [{ kind: "production_capacity", amount: 4, paid: 0 }] },
  options: [
    unit("fighter", 1),
    unit("carrier", 3),
    unit("cruiser", 2),
    unit("destroyer", 1),
    unit("dreadnought", 4),
    unit("war_sun", 12),
    unit("infantry", 1),
    unit("mech", 2),
    { id: "done_producing", kind: "decline", label: "produce nothing further" },
  ],
};

/**
 * Controls of every unit row that are under another element (the sticky footer, the header) or off
 * screen when their row is scrolled to the middle of the dialog, and after scrolling to its very end
 * (the last row must clear the footer there too).
 */
async function coveredControls(page: Page) {
  return page.evaluate(() => {
    const drawer = document.querySelector<HTMLElement>('[data-testid="production-builder-drawer"]')!;
    const scroller = [drawer, ...Array.from(drawer.querySelectorAll<HTMLElement>("*"))].find(
      (el) => el.scrollHeight > el.clientHeight + 2 && /auto|scroll/.test(getComputedStyle(el).overflowY),
    );
    const out: string[] = [];
    const rows = Array.from(document.querySelectorAll<HTMLElement>('[data-testid^="produce-option-"]'));
    const check = (row: HTMLElement, when: string) => {
      for (const control of Array.from(row.querySelectorAll<HTMLElement>("button"))) {
        const r = control.getBoundingClientRect();
        const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        const name = `${row.getAttribute("data-testid")} ${control.getAttribute("data-testid") ?? control.getAttribute("aria-label")} (${when})`;
        if (r.bottom > window.innerHeight || r.top < 0 || !top || !(control === top || control.contains(top))) out.push(name);
      }
    };
    for (const row of rows) {
      row.scrollIntoView({ block: "center" });
      check(row, "centred");
    }
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
    check(rows[rows.length - 1], "end of scroll");
    return { rows: rows.length, covered: out, scrolled: !!scroller };
  });
}

for (const size of Object.keys(SIZES) as SizeName[])
  test(`${prefix} production rows reachable ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(SIZES[size]);
    await openMockedGame(page, { players: [playerWithHand(), opponent], choice });
    const drawer = page.getByTestId("production-builder-drawer");
    await drawer.waitFor();
    await page.waitForTimeout(300);
    await shot(page, testInfo, `${prefix}-production-top-${size}`);
    const result = await coveredControls(page);
    console.log(`MEASURE ${size}: ${result.rows} rows, covered at the end of the scroll: ${JSON.stringify(result.covered)}`);
    await page.waitForTimeout(200);
    await shot(page, testInfo, `${prefix}-production-end-${size}`);
    if (prefix === "before") return;
    expect(result.rows).toBeGreaterThanOrEqual(8);
    expect(result.covered, "no row control is under the footer or off screen at the end of the scroll").toEqual([]);
    // Keyboard focus on the Dreadnought + scrolls it clear of the footer (scroll-padding).
    const focusCovered = await page.evaluate(() => {
      const button = document.querySelector<HTMLElement>('[data-testid="produce-unit-btn-build|dreadnought|1"]')!;
      document.querySelector('[data-testid="production-builder-drawer"]')!.scrollTop = 0;
      button.focus();
      const r = button.getBoundingClientRect();
      const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return !(top && (button === top || button.contains(top)));
    });
    expect(focusCovered, "a focused row is not under the footer").toBe(false);
    // The Dreadnought row can be used: its + raises the count.
    await page.getByTestId("produce-unit-btn-build|dreadnought|1").click();
    await expect(page.getByTestId("produce-count-build|dreadnought|1")).toHaveText("1");
  });
