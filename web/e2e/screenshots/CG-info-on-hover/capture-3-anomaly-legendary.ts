import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";

// Selecting a system explains its anomaly, and a legendary planet's ability sits on its card.
const SIZES = [["desktop", { width: 1440, height: 900 }], ["390x844", { width: 390, height: 844 }]] as const;

for (const [system, name, text] of [
  ["41", "gravity-rift", /Gravity rift:.*1 to 3/],
  ["42", "nebula", /Nebula:.*move value of 1/],
  ["65", "legendary", null],
] as const) {
  for (const [size, viewport] of SIZES) {
    test(`${name} ${size}`, async ({ page }, testInfo) => {
      await page.setViewportSize(viewport);
      await openMockedGame(page, { players: [playerWithHand(), opponent] });
      await page.getByTestId(`system-hex-${system}`).click();
      await page.getByTestId("system-inspector").waitFor();
      if (text) await expect(page.getByTestId("inspector-anomaly-rule")).toHaveText(text);
      else await expect(page.getByTestId("inspector-legendary-primor")).toContainText("The Atrament");
      await page.waitForTimeout(500);
      await shot(page, testInfo, `after-${name}-${size}`);
    });
  }
}
