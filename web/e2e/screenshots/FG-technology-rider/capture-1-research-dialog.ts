import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { galleryBoard } from "../_shared/fixtures";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";

// A correct Technology Rider asks "research a technology" (subtype research_technology, source the
// action card) when the vote closes; the client shows the ordinary technology dialog for it:
// one free pick, no cost lines.
test.describe.configure({ retries: 2, mode: "serial" });

const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;

const researchIds = ["amd", "nm", "st", "ps", "det", "pa", "gd", "dxa", "gls", "md"];

async function openRiderResearch(page: Page, size: keyof typeof SIZES) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    board: { ...galleryBoard, systems: {} },
    players: [playerWithHand({ technologies: [] }), opponent],
    choice: {
      prompt: "research a technology",
      context: { subtype: "research_technology", source: { ActionCard: "tech_rider" } },
      options: researchIds.map((id) => ({ id, label: id, kind: "research", payload: { cost: 0, cost_tokens: 0 } })),
    },
  });
  await page.getByTestId("technology-modal").waitFor();
  await page.waitForTimeout(700); // the dialog fades in
}

for (const size of Object.keys(SIZES) as Array<keyof typeof SIZES>) {
  test(`technology rider research ${size}`, async ({ page }, testInfo) => {
    await openRiderResearch(page, size);
    // A rider's reward is one free pick: no second pick, no payment.
    await expect(page.getByTestId("technology-modal")).toBeVisible();
    await shot(page, testInfo, `rider-research-${size}`);
  });
}
