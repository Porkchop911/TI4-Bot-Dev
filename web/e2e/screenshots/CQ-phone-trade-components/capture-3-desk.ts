import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { proposeChoice, tradePlayers } from "../AG-trade-staging-desk/_trade";
import { PHONES, checking, placement, prefix } from "./scene";

// After Open: the trade desk on a phone, as it first appears (is the partner named on screen?).
for (const phone of PHONES.filter((p) => p.name === "390x664")) {
  test(`trade desk top at ${phone.name}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: phone.width, height: phone.height });
    await openMockedGame(page, { players: tradePlayers, choice: proposeChoice });
    const desk = page.getByTestId("trade-desk-modal");
    await desk.waitFor();
    // The partner is named in the desk header, on screen without scrolling.
    const title = desk.getByText("transaction with Jolnar", { exact: true }).filter({ visible: true }).last();
    const p = await placement(title);
    console.log(`${phone.name} desk title: ${JSON.stringify(p)}`);
    if (checking) expect(p.inViewport && p.covered === null, "partner named on screen").toBe(true);
    await shot(page, testInfo, `${prefix}-desk-${phone.name}`);
  });
}
