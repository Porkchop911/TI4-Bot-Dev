import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

/** Which side of the change a run is: `before` is taken on the commit without the change. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
  "360x740": { width: 360, height: 740 },
} as const;
export type SizeName = keyof typeof SIZES;

const researchIds = ["amd", "nm", "st", "ps", "det", "pa", "gd", "dxa", "gls", "md", "sr", "bs"];

export interface Variant {
  /** Technology primary (first free, 0 / 2) or a secondary (one pick, 4 resources). */
  card: "primary" | "secondary";
  /** The viewer's planets (specialty planets give Tech Skips) are on the board. */
  planets: boolean;
  tradeGoods: number;
}

/** The research decision of the Technology strategy card, opened as the real app does. */
export async function openResearch(page: Page, size: SizeName, v: Variant) {
  await page.setViewportSize(SIZES[size]);
  await openMockedGame(page, {
    board: v.planets ? galleryBoard : { ...galleryBoard, systems: {} },
    players: [playerWithHand({ technologies: [], trade_goods: v.tradeGoods }), opponent],
    choice: {
      prompt: "Research a technology",
      context: { subtype: "research_technology", source: { StrategyCard: { card: "Technology", secondary: v.card === "secondary" } } },
      options: researchIds.map((id) => ({ id, label: id, kind: "research", ...(v.card === "secondary" ? { payload: { cost: 4 } } : {}) })),
    },
  });
  await page.getByTestId("technology-modal").waitFor();
  await page.waitForTimeout(700); // the dialog fades in
}
