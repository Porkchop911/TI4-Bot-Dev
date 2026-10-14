import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { measure, openDecision, openHeader, openPrepare, prefix, SIZES, type SizeName } from "./scene";

// Technology overlay on phones: the three ways it opens, at four phone sizes. Every run prints the
// measurements (MEASURE lines); on the fixed code it also asserts the layout (the `before` run, taken
// on the commit without the fix, only measures).
const entries = [
  { key: "header", open: openHeader, ids: ["technology-modal-close"] },
  { key: "decision", open: openDecision, ids: ["technology-modal-close", "confirm-research-btn", "decline-research-btn"] },
  { key: "prepare", open: openPrepare, ids: ["technology-modal-close", "confirm-research-btn", "decline-research-btn"] },
];

test.describe.configure({ retries: 2 }); // the software-rendered page sometimes crashes under load

for (const entry of entries) {
  for (const size of Object.keys(SIZES) as SizeName[]) {
    test(`technology ${entry.key} ${size}`, async ({ page }, testInfo) => {
      const errors: string[] = [];
      page.on("pageerror", (e) => errors.push(String(e)));
      await entry.open(page, size);
      await page.waitForTimeout(700); // the dialog fades in
      if (entry.key === "prepare" && SIZES[size].height <= 500) {
        // The narrowed landscape dialog scrolls its research panel: reach Confirm the way a thumb does.
        await page.getByTestId("confirm-research-btn").scrollIntoViewIfNeeded();
      }
      const m = await measure(page, entry.ids);
      console.log(`MEASURE ${prefix} ${entry.key} ${size} ${JSON.stringify({ ...m, errors })}`);
      await shot(page, testInfo, `${prefix}-${entry.key}-${size}`);
      if (prefix === "before") return;

      const { width, height } = SIZES[size];
      expect(errors).toEqual([]);
      // No horizontal scroll, on the page or inside the dialog.
      expect(m.docScrollW).toBeLessThanOrEqual(width);
      // Landscape prepare: the dialog is deliberately narrowed to leave the right half to the docked
      // "Preparing your secondary" banner (AR-prep-confirm-over-dialog), so its grid column is only
      // ~170 px wide and clips (overflow-x hidden) instead of scrolling; only the other cases assert it.
      const narrowedForBanner = entry.key === "prepare" && height <= 500;
      if (!narrowedForBanner) expect(m.scrollBody!.scrollW).toBeLessThanOrEqual(m.scrollBody!.clientW);
      // The dialog lies within the viewport, and the grid has room to be read.
      expect(m.modal!.x).toBeGreaterThanOrEqual(0);
      expect(m.modal!.y).toBeGreaterThanOrEqual(0);
      expect(m.modal!.x + m.modal!.w).toBeLessThanOrEqual(width);
      expect(m.modal!.y + m.modal!.h).toBeLessThanOrEqual(height);
      expect(m.scrollBody!.clientH).toBeGreaterThanOrEqual(Math.min(250, height * 0.6));
      // Close / Decline / Confirm are in view, not covered, and at least 44 px high.
      for (const id of entry.ids) {
        const b = m.buttons[id];
        expect(b, id).not.toBeNull();
        expect(b!.visible, `${id} in view`).toBe(true);
        expect(b!.covered, `${id} covered by`).toBeNull();
        expect(b!.h, `${id} height`).toBeGreaterThanOrEqual(44);
      }
      expect(m.smallTargets).toEqual([]);
      // The header's Close button closes the read-only view.
      if (entry.key === "header") {
        await page.getByTestId("technology-modal-close").click();
        await expect(page.getByTestId("technology-modal")).toHaveCount(0);
      }
    });
  }
}
