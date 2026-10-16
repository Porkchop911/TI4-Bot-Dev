import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { proposeChoice, tradePlayers } from "../AG-trade-staging-desk/_trade";

/** The same desk as AG, from a server that accepts deals built from any amounts (`details.free_form`). */
export const freeFormChoice = { ...proposeChoice, nonce: "trade-free", details: { free_form: "x;" } };

export async function openFreeDesk(page: Page, width = 1100, height = 1250) {
  await page.setViewportSize({ width, height });
  await openMockedGame(page, { players: tradePlayers, choice: freeFormChoice });
  await page.getByTestId("trade-desk-modal").waitFor();
}

export { click, shotDesk } from "../AG-trade-staging-desk/_trade";
