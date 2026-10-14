import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { galleryBoard, galleryEventLog, systemActivationOptions } from "../_shared/fixtures";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";
import { box, onTop, overlaps, prefix, SIZES, type SizeName } from "./scene";

// Nightly 2026-10-08, runs 21 and 25: with a system selected during activation the system detail popup
// lay over the "Confirm Activation" bar (and the confirm could not be tapped). The popup now ends above
// the activation bar on every layout, and can still be folded on a phone.
type Variant = SizeName | "desktop-short" | "desktop-log";
const sizes: Variant[] = ["desktop", "desktop-short", "desktop-log", "390x844", "360x740", "844x390"];
const viewport = (name: Variant) => (name === "desktop-short" ? { width: 1100, height: 640 } : name === "desktop-log" ? SIZES.desktop : SIZES[name]);

for (const size of sizes)
  test(`${prefix} system popup and activation bar ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport(size));
    await openMockedGame(page, {
      players: [playerWithHand(), opponent],
      board: galleryBoard,
      events: galleryEventLog,
      choice: { prompt: "Choose a system to activate", context: { subtype: "activate_system" }, options: systemActivationOptions },
    });
    await page.getByTestId("ti4-board-svg").waitFor();
    await page.waitForTimeout(350);
    // The docked event log shortens the board column; the activation bar rises with it.
    if (size === "desktop-log") await page.getByTestId("event-log-toggle").click();
    await page.waitForTimeout(300);
    await page.getByTestId("system-hex-18").click();
    await page.getByTestId("system-inspector").waitFor();
    await page.waitForTimeout(300);
    const panel = page.getByTestId("system-inspector");
    const bar = page.getByTestId("system-activation-bar");
    const confirm = page.getByTestId("confirm-activation-btn");
    const p = await box(panel);
    const b = await box(bar);
    const covered = overlaps(p, b);
    console.log(`MEASURE ${size}: popup y=${Math.round(p.y)}..${Math.round(p.y + p.height)} bar y=${Math.round(b.y)}..${Math.round(b.y + b.height)} overlap ${covered}`);
    await shot(page, testInfo, `${prefix}-popup-${size}`);
    if (prefix === "before") return;
    expect(covered, "popup does not cover the activation bar").toBe(false);
    expect(await onTop(page, confirm), "Confirm Activation is the topmost element at its centre").toBe(true);
    expect(await onTop(page, page.getByTestId("cancel-activation-btn"))).toBe(true);
  });
