import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "../AH-secondary-prep-real-ui/prep";
import { prefix, SIZES, type SizeName } from "./scene";

// The "Preparing" banner docks at the bottom of a phone and could not be folded, so with the stand-in
// question minimized it still covered ~170 px of the map and hid the Resume pill behind it. It now has
// a fold control (title + Save plan row only), and the pill sits above it. TI4_SHOT_PREFIX=before is
// how the "before" shots were taken, on the base commit.
const sizes: SizeName[] = ["390x844", "360x740", "844x390"];

for (const size of sizes)
  test(`${prefix} prepare banner with the question minimized ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(SIZES[size]);
    await openWaiting(page, { name: "Technology", card: "pok7technology", viewer: { trade_goods: 5 } });
    await page.getByTestId("secondary-prep-chip").evaluate((el) => (el as HTMLElement).click());
    await page.getByTestId("prepare-banner").waitFor();
    await page.getByTestId("minimize-choice-button").click();
    await page.getByTestId("minimized-choice-banner").waitFor();
    const measure = async (label: string) => {
      const banner = (await page.getByTestId("prepare-banner").boundingBox())!;
      const pill = (await page.getByTestId("minimized-choice-banner").boundingBox())!;
      const overlap = pill.x < banner.x + banner.width && banner.x < pill.x + pill.width && pill.y < banner.y + banner.height && banner.y < pill.y + pill.height;
      console.log(`MEASURE ${size} ${label}: banner ${Math.round(banner.width)}x${Math.round(banner.height)} at y=${Math.round(banner.y)}; pill y=${Math.round(pill.y)}; overlap ${overlap}`);
      return { banner, pill, overlap };
    };
    const open = await measure("banner open");
    await shot(page, testInfo, `${prefix}-banner-open-${size}`);
    if (prefix === "before") return;
    expect(open.overlap, "the pill clears the banner").toBe(false);
    await page.getByTestId("prep-fold").click();
    await expect(page.getByTestId("prepare-banner")).toHaveAttribute("data-folded", "true");
    const folded = await measure("banner folded");
    await shot(page, testInfo, `${prefix}-banner-folded-${size}`);
    expect(folded.overlap).toBe(false);
    expect(folded.banner.height).toBeLessThan(open.banner.height / 2);
    for (const id of ["prep-save", "prep-fold"]) expect((await page.getByTestId(id).boundingBox())!.height).toBeGreaterThanOrEqual(43.5);
    // Resume still brings the question back, with the banner as it was left.
    await page.getByTestId("resume-choice-button").click();
    await page.getByTestId("secondary-yes-btn").waitFor();
    await expect(page.getByTestId("prepare-banner")).toHaveAttribute("data-folded", "true");
  });
