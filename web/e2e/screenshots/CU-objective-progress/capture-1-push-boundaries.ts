import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { actor } from "../_shared/players";
import {
  noHorizontalPageScroll,
  openObjectives,
  openScene,
  PUSH_STATES,
  SIZES,
  type PushState,
  type SizeName,
} from "./scene";

// Push Boundaries in the Objectives dialog: every seat's column lists ours/theirs per neighbour,
// blue when ahead, red when tied or behind, plus "Ahead of N of M neighbors · need 2".
const EXPECT: Record<PushState, { rows: [string, string][]; summary: string }> = {
  "ahead-2": {
    rows: [["12/10", "ahead"], ["12/9", "ahead"], ["12/13", "behind"]],
    summary: "Ahead of 2 of 3 neighbors · need 2",
  },
  "ahead-1": {
    rows: [["12/10", "ahead"], ["12/13", "behind"], ["12/12", "tied"]],
    summary: "Ahead of 1 of 3 neighbors · need 2",
  },
  tied: {
    rows: [["12/12", "tied"], ["12/12", "tied"], ["12/14", "behind"]],
    summary: "Ahead of 0 of 3 neighbors · need 2",
  },
};

for (const push of Object.keys(PUSH_STATES) as PushState[]) {
  for (const size of Object.keys(SIZES) as SizeName[]) {
    test(`push boundaries ${push} ${size}`, async ({ page }, testInfo) => {
      await openScene(page, size, push, 2);
      await openObjectives(page, size);
      const modal = page.getByTestId("objectives-modal");
      // The viewer's column on desktop, the card-column copy on a phone.
      const mine =
        size === "desktop"
          ? modal.getByTestId(`neighbour-progress-push_boundaries-${actor}`)
          : modal.getByTestId("neighbour-progress-push_boundaries-card");
      await expect(mine).toBeVisible();
      await expect(mine).toContainText(EXPECT[push].summary);
      const rows = mine.locator("li");
      await expect(rows).toHaveCount(3);
      for (const [index, [text, standing]] of EXPECT[push].rows.entries()) {
        await expect(rows.nth(index)).toContainText(text);
        await expect(rows.nth(index)).toHaveAttribute("data-standing", standing);
      }
      if (size === "390x844") expect(await noHorizontalPageScroll(page)).toBeLessThanOrEqual(0);
      await page.getByTestId("objective-row-push_boundaries").scrollIntoViewIfNeeded();
      await shot(page, testInfo, `1-push-${push}-${size}`);
    });
  }
}
