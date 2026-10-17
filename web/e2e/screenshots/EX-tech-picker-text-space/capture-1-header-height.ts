import { expect, test } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { shot } from "../_shared/shot";
import { openResearch, prefix, SIZES, type SizeName, type Variant } from "./scene";

// The Select Research picker: are the technologies' own texts there, and how tall is everything above the
// first technology. `before` runs (TI4_SHOT_PREFIX=before, on the release without the change) only measure.
test.describe.configure({ retries: 2, mode: "serial" });

const here = dirname(fileURLToPath(import.meta.url));
const cases: Array<{ key: string; v: Variant }> = [
  { key: "free-skips", v: { card: "primary", planets: true, tradeGoods: 0 } },
  { key: "free-noskips", v: { card: "primary", planets: false, tradeGoods: 3 } },
];

for (const size of Object.keys(SIZES) as SizeName[]) {
  for (const { key, v } of cases) {
    test(`${key} ${size}`, async ({ page }, testInfo) => {
      await openResearch(page, size, v);
      const m = await page.evaluate(() => {
        const rect = (sel: string) => document.querySelector(sel)?.getBoundingClientRect();
        const first = rect('[data-testid^="tech-name-"]')!;
        const skips = rect('[data-testid="tech-skips-list"]')!;
        const filter = rect('[data-testid="tech-filter"]')!;
        const pill = document.querySelector<HTMLElement>('button[data-testid^="tech-skip-"]');
        const effect = document.querySelector<HTMLElement>('[data-testid^="tech-effect-"]');
        const er = effect?.getBoundingClientRect();
        const panel = rect('[data-testid="tech-research-controls"]')!;
        return {
          firstCardTop: Math.round(first.top),
          researchPanelBottom: Math.round(panel.bottom),
          skipsTop: Math.round(skips.top),
          filterTop: Math.round(filter.top),
          filterBottom: Math.round(filter.bottom),
          skipPillHeight: pill ? Math.round(pill.getBoundingClientRect().height) : null,
          effectVisible: !!er && er.height > 0,
          effectHeight: er ? Math.round(er.height) : 0,
        };
      });
      const file = join(here, "out", `measure-${prefix}.json`);
      mkdirSync(dirname(file), { recursive: true });
      let all: Record<string, unknown> = {};
      try {
        all = JSON.parse(readFileSync(file, "utf8"));
      } catch {
        /* first write */
      }
      all[`${key} ${size}`] = m;
      writeFileSync(file, JSON.stringify(all, null, 1));
      console.log(`${prefix} ${key} ${size}: ${JSON.stringify(m)}`);
      if (prefix !== "before") {
        // The technology's own text is shown on every size, the players row stays hidden on phones.
        expect(m.effectVisible).toBe(true);
        expect(m.effectHeight).toBeGreaterThan(10);
        if (SIZES[size].width < 720 || SIZES[size].height < 500) {
          await expect(page.getByTestId("tech-modal-roster")).toBeHidden();
        }
        if (key === "free-skips" && m.skipPillHeight !== null) expect(m.skipPillHeight).toBeLessThanOrEqual(44);
      }
      await shot(page, testInfo, `${prefix}-${key}-${size}`);
    });
  }
}
