import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { hoverShot } from "../_shared/hover";
import { playerWithHand, opponent } from "../_shared/players";

// In the system inspector, the star beside a legendary planet's name says what it is.
test("tooltip: legendary planet in the system inspector", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent] });
  await page.getByTestId("system-hex-65").click();
  const planet = page.getByTestId("inspector-planet-primor");
  await planet.waitFor();
  await hoverShot(page, testInfo, "27-legendary-planet", planet.locator('[title="Legendary Planet"]'), { native: true, pad: 60 });
});
