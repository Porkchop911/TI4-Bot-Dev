import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import type { Locator, Page, TestInfo } from "@playwright/test";

/** Folder of the artifact the running capture belongs to. */
export const artifactDir = (testInfo: TestInfo) => dirname(testInfo.file);

/** Stops every CSS animation and transition, so pulsing highlights render the same on each run. */
export async function freeze(page: Page) {
  await page.addStyleTag({ content: "*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}" });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(300);
}

export interface ShotOptions {
  /** Capture only this element (default: the viewport). */
  of?: Locator;
  /** Extra pixels around `of`. */
  pad?: number;
}

/**
 * Writes <artifact folder>/out/<name>.png. Names are stable, so a rerun overwrites the same file.
 * Animations and carets are disabled for repeatable pixels.
 */
export async function shot(page: Page, testInfo: TestInfo, name: string, options: ShotOptions = {}) {
  const out = join(artifactDir(testInfo), "out");
  mkdirSync(out, { recursive: true });
  const path = join(out, `${name}.png`);
  await freeze(page);
  if (options.of) {
    const box = await options.of.boundingBox();
    if (!box) throw new Error(`shot ${name}: element has no bounding box`);
    const pad = options.pad ?? 0;
    const viewport = page.viewportSize() ?? { width: 1440, height: 900 };
    const x = Math.max(0, box.x - pad);
    const y = Math.max(0, box.y - pad);
    await page.screenshot({
      path,
      animations: "disabled",
      caret: "hide",
      clip: {
        x,
        y,
        width: Math.min(viewport.width - x, box.width + 2 * pad),
        height: Math.min(viewport.height - y, box.height + 2 * pad),
      },
    });
  } else {
    await page.screenshot({ path, animations: "disabled", caret: "hide" });
  }
  return path;
}
