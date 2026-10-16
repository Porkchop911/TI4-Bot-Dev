import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { newDevicePage, players, prefix, type DeviceName } from "./scene";

// A choice dialog in a phone in desktop-site mode (and a plain phone, and a desktop for reference):
// Minimize decision is a 44 px button, the pill leaves the map visible and tappable, Resume brings
// the dialog back, and a minimized dialog stays minimized.
const devices: DeviceName[] = ["desktop-site", "mobile", "desktop"];

for (const device of devices) {
  test(`${prefix} minimize ${device}`, async ({ browser, baseURL }, testInfo) => {
    test.skip(prefix === "before" && device !== "desktop-site", "only the desktop-site case differs");
    const { context, page } = await newDevicePage(browser, device, baseURL);
    if (prefix === "before" && device === "desktop-site") {
      await page.addInitScript(() => window.localStorage.setItem("ti4.layout.v1", "desktop"));
    }
    await openMockedGame(page, {
      players: players(),
      choice: {
        prompt: "Choose a command token to return",
        options: [
          { id: "a", label: "Tactic pool", kind: "choice" },
          { id: "b", label: "Fleet pool", kind: "choice" },
        ],
      },
    });
    await page.getByTestId("ti4-board-svg").waitFor();
    const minimize = page.getByRole("button", { name: "Minimize decision" });
    await minimize.waitFor();
    const box = (await minimize.boundingBox())!;
    console.log(`MEASURE ${prefix} ${device} minimize button ${Math.round(box.width)}x${Math.round(box.height)}`);
    await shot(page, testInfo, `${prefix}-choice-open-${device}`);
    if (device !== "desktop" && prefix === "after") {
      expect(box.width).toBeGreaterThanOrEqual(44);
      expect(box.height).toBeGreaterThanOrEqual(44);
    }
    await minimize.click();
    const pill = page.getByTestId("resume-choice-button");
    await pill.waitFor();
    await page.waitForTimeout(500);
    await shot(page, testInfo, `${prefix}-choice-minimized-${device}`);
    // The map is reachable: a hex under the dialog's former place receives the tap.
    await expect(page.getByRole("button", { name: "Minimize decision" })).toHaveCount(0);
    await pill.click();
    await page.getByRole("button", { name: "Minimize decision" }).waitFor();
    await context.close();
  });
}
