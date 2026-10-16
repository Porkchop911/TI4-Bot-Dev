import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting } from "../AH-secondary-prep-real-ui/prep";
import { box, overlaps, prefix, SIZES, type SizeName } from "./scene";

// Nightly 2026-10-08, runs 44 and 60: the "Preparing" banner covered the title of the stand-in question
// (the dialog is centred in the whole screen and the banner floats over its top). The dialog now starts
// below the banner (desktop), above it (portrait phone) or left of it (landscape phone).
const sizes: (SizeName | "desktop-short")[] = ["desktop", "desktop-short", "390x844", "360x740", "844x390"];
const viewport = (name: SizeName | "desktop-short") => (name === "desktop-short" ? { width: 1280, height: 520 } : SIZES[name]);

for (const size of sizes)
  test(`${prefix} prepare banner keeps the question title visible ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport(size));
    await openWaiting(page, { name: "Politics", card: "pok3politics" });
    await page.getByTestId("secondary-prep-chip").evaluate((el) => (el as HTMLElement).click());
    await page.getByTestId("strategy-secondary-panel").waitFor();
    await page.waitForTimeout(300);
    const check = async (label: string) => {
      const banner = await box(page.getByTestId("prepare-banner"));
      const title = await box(page.getByTestId("choice-prompt"));
      const buttons = await page.getByTestId("strategy-secondary-panel").getByRole("button").evaluateAll((els) =>
        els.map((el) => el.getBoundingClientRect()).map((r) => ({ x: r.x, y: r.y, width: r.width, height: r.height })),
      );
      const hits = [title, ...buttons].filter((b) => overlaps(b, banner)).length;
      const fits = title.y >= 0 && [title, ...buttons].every((b) => b.y + b.height <= viewport(size).height + 0.5);
      console.log(`MEASURE ${size} ${label}: banner y=${Math.round(banner.y)}..${Math.round(banner.y + banner.height)}, title y=${Math.round(title.y)}, ${hits} controls under the banner`);
      return { hits, fits, titleVisible: title.y >= 0 && title.y + title.height <= viewport(size).height };
    };
    const open = await check("banner open");
    await shot(page, testInfo, `${prefix}-banner-${size}`);
    if (prefix === "before") return;
    expect(open.hits, "title, Spend and Skip clear of the banner").toBe(0);
    expect(open.titleVisible, "the title is on screen").toBe(true);
    if (size !== "desktop") {
      await page.getByTestId("prep-fold").click();
      const folded = await check("banner folded");
      expect(folded.hits).toBe(0);
      if (size === "desktop-short") expect(folded.fits, "folded, the whole question fits above the banner").toBe(true);
      await shot(page, testInfo, `${prefix}-banner-folded-${size}`);
    }
  });
