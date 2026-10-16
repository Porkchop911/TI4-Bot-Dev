import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openScene, prefix, technologyScript } from "./scene";

// Technology, prepare mode. After: the engine's own research list (a technology that needs a
// planet skip is on it), the way it would pay the 4 resources, and a small "as of now" note.
// Before (the commit without the preview): a list worked out in the browser from the seat's own
// technologies, flagged approximate, with nothing that needs a skip or a waiver.
test("secondary prep: Technology list, exact versus estimate", async ({ page }, testInfo) => {
  await openScene(page, { name: "Technology", card: "pok7technology", script: prefix === "before" ? null : technologyScript });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  await page.getByTestId("technology-modal").waitFor();
  await page.getByTestId(prefix === "before" ? "prepare-approximate" : "prepare-as-of-now").waitFor();
  // Use a planet's tech skip as in the real question, then pick the first second-row technology
  // the list lets through (after: the engine listed it; before: it was never offered).
  const skip = page.locator('[data-testid^="tech-skip-"]').first();
  // dispatchEvent: before the banner fix the banner covers these toggles, which a real click cannot reach.
  if (await skip.count()) await skip.dispatchEvent("click");
  const secondRow = page.locator('[data-testid="tech-card-gd"], [data-testid="tech-card-dxa"], [data-testid="tech-card-gls"], [data-testid="tech-card-sr"], [data-testid="tech-card-bs"], [data-testid="tech-card-pi"]');
  const pick = secondRow.and(page.locator('[data-selectable="true"]')).first();
  if (await pick.count()) await pick.click();
  await page.waitForTimeout(200);
  const selectable = await secondRow.and(page.locator('[data-selectable="true"]')).count();
  console.log(`technology ${prefix}: second-row technologies selectable = ${selectable}`);
  await shot(page, testInfo, `${prefix}-1-technology`);
});
