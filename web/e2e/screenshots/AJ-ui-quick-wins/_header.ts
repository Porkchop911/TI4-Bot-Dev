import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { systemActivationOptions } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

/** A pending system activation; the engine stage id behind it is `activate_system`. */
export async function openHeaderGame(page: Page, width: number, height = 800) {
  await page.setViewportSize({ width, height });
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: {
      prompt: "choose a system to activate",
      context: { subtype: "activate_system" },
      options: systemActivationOptions,
    },
  });
  await page.getByTestId("turn-status-banner").waitFor();
}
