import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

/** Which side of the change a run is: `before` is taken on the commit without the fix. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

/** Resources the gallery seat gets from its planets; trade goods are added up to the wanted total. */
const PLANET_RESOURCES = Number(process.env.TI4_PLANET_RESOURCES ?? 0);

const researchIds = ["amd", "nm", "st", "ps", "det", "pa", "gd", "dxa", "gls", "md", "sr", "bs"];

/** The Technology primary's research decision for a seat that can spend `total` resources. */
export async function openPrimary(page: Page, size: SizeName, total: number) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    // No planets: the viewer's resources are its trade goods alone.
    board: { ...galleryBoard, systems: {} },
    players: [playerWithHand({ technologies: [], trade_goods: Math.max(0, total - PLANET_RESOURCES) }), opponent],
    choice: {
      prompt: "Research a technology",
      context: { subtype: "research_technology", source: { StrategyCard: { card: "Technology", secondary: false } } },
      options: researchIds.map((id) => ({ id, label: id, kind: "research" })),
    },
  });
  await page.getByTestId("technology-modal").waitFor();
  await page.waitForTimeout(700); // the dialog fades in
}
