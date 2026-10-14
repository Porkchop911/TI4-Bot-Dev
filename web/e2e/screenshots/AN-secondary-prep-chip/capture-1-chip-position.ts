import { expect, test, type Page } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "../AH-secondary-prep-real-ui/prep";

// Where the "Prepare your secondary" chip, the expanded prepare panel and the prepared chip sit.
// TI4_SHOT_PREFIX=before is how the committed "before" shots were taken (on the base commit,
// unified-2026-10-08); the default "after" regenerates the current layout and asserts the chip
// never covers the toolbar, the seat legend, the header (turn banner) or the top row of tiles.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const viewports = [
  { id: "1280x720", width: 1280, height: 720 },
  { id: "1440x900", width: 1440, height: 900 },
  { id: "1024x768", width: 1024, height: 768 },
  { id: "390x844", width: 390, height: 844 },
];

type Box = { x: number; y: number; width: number; height: number };
const overlaps = (a: Box, b: Box) =>
  a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

async function boxOf(page: Page, selector: string): Promise<Box> {
  const box = await page.locator(selector).first().boundingBox();
  if (!box) throw new Error(`no bounding box for ${selector}`);
  return box;
}

/** The prep element must keep clear of everything it could cover. `tiles` is off for the panel, which is a deliberate overlay. */
async function expectClear(page: Page, what: string, tiles: boolean) {
  const prepId = tiles ? "secondary-prep-chips" : "secondary-prep";
  const prep = await boxOf(page, `[data-testid="${prepId}"]`);
  const named: [string, string][] = [
    ["toolbar", ".map-overlay-toolbar"],
    ["seat legend", ".board-seat-legend"],
    ["header / turn banner", ".app-shell__header"],
  ];
  for (const [label, selector] of named) {
    expect(overlaps(prep, await boxOf(page, selector)), `${what} must not cover the ${label}`).toBe(false);
  }
  const floating = page.locator(".app-shell__mobile-actions");
  if (await floating.isVisible()) {
    expect(overlaps(prep, await boxOf(page, ".app-shell__mobile-actions")), `${what} must not cover the Players/Events buttons`).toBe(false);
  }
  if (tiles) {
    const hexes = await page.locator('[data-testid^="system-hex-"]').evaluateAll((els) =>
      els.map((el) => {
        const r = el.getBoundingClientRect();
        return { x: r.x, y: r.y, width: r.width, height: r.height };
      }),
    );
    const topRow = Math.min(...hexes.map((h) => h.y));
    for (const hex of hexes.filter((h) => h.y < topRow + 4)) {
      expect(overlaps(prep, hex), `${what} must not cover the top row of tiles`).toBe(false);
    }
  }
  const chrome = await boxOf(page, '[data-testid="board-chrome"]');
  if (tiles) {
    // The chip is a row of the toolbar strip itself.
    expect(prep.y, `${what} is inside the toolbar strip`).toBeGreaterThanOrEqual(chrome.y - 1);
    expect(prep.y + prep.height, `${what} is inside the toolbar strip`).toBeLessThanOrEqual(chrome.y + chrome.height + 1);
  } else {
    // The banner floats right below the strip, measured from the board column.
    expect(prep.y, `${what} starts below the toolbar strip`).toBeGreaterThanOrEqual(chrome.y + chrome.height - 1);
  }
}

for (const viewport of viewports) {
  test(`${prefix} ${viewport.id}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await openWaiting(page, { name: "Technology", card: "pok7technology", viewer: { trade_goods: 5 } });
    await page.getByTestId("ti4-board-svg").waitFor();
    await page.getByTestId("secondary-prep-chip").waitFor();
    if (prefix === "after") await expectClear(page, "the chip", true);
    await shot(page, testInfo, `${prefix}-${viewport.id}-chip`);

    await page.getByTestId("secondary-prep-chip").click();
    await page.getByTestId("prepare-banner").waitFor();
    if (prefix === "after") await expectClear(page, "the prepare banner", false);
    await shot(page, testInfo, `${prefix}-${viewport.id}-panel`);

    await page.getByTestId("secondary-yes-btn").click();
    await page.locator('[data-testid^="tech-card-"][data-selectable="true"]').first().click();
    await page.getByTestId("confirm-research-btn").click();
    await page.getByTestId("secondary-prep-chip").filter({ hasText: "Prepared" }).waitFor();
    if (prefix === "after") await expectClear(page, "the prepared chip", true);
    await shot(page, testInfo, `${prefix}-${viewport.id}-prepared`);
  });
}

// The app header can be taller than the toolbar row (the base commit placed the chip header-height too low).
test(`${prefix} tall header`, async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await openWaiting(page, { name: "Technology", card: "pok7technology" });
  await page.getByTestId("secondary-prep-chip").waitFor();
  await page.addStyleTag({ content: ".app-shell__header{min-height:140px}" });
  await page.waitForTimeout(300);
  if (prefix === "after") await expectClear(page, "the chip under a tall header", true);
  await shot(page, testInfo, `${prefix}-tall-header`);
});
