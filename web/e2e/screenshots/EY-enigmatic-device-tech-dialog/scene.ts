import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

/** Which side of the change a run is: `before` is taken on the commit without the change. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const SIZES = {
  desktop: { width: 1440, height: 900 },
  "390x844": { width: 390, height: 844 },
} as const;
export type SizeName = keyof typeof SIZES;

/** A seat with no technologies and no specialties: only technologies without prerequisites qualify. */
const RESEARCHABLE = ["amd", "ps", "sar", "st", "nm", "pi", "dd2"];
/** What the old engine offered on top: technologies whose prerequisites the seat lacks. */
const UNMET = ["gd", "cr2", "ws", "x89c4"];

/** The Enigmatic Device (exploration card ed1) asking which technology to research. */
export async function openDevice(page: Page, size: SizeName) {
  await page.setViewportSize(SIZES[size]);
  const ids = prefix === "before" ? [...RESEARCHABLE, ...UNMET] : RESEARCHABLE;
  await openMockedGame(page, {
    board: { ...galleryBoard, systems: {} },
    players: [playerWithHand({ technologies: [], trade_goods: 8 }), opponent],
    choice: {
      prompt: "research which technology with the Enigmatic Device",
      context: {
        subtype: "enigmatic_device_choose_technology",
        source: { Content: "ed1" },
      },
      options: ids.map((id) => ({ id, label: `research ${id}`, kind: "technology" })),
    },
  });
  await page.waitForTimeout(900);
}
