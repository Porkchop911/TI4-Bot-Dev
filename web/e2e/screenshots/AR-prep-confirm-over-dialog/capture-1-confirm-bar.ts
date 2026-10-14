import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting, push, save, secondaryQuestion } from "../AH-secondary-prep-real-ui/prep";

// A prepared secondary in Review mode: when the real question opens (a backdrop dialog), the
// "Prepared ... Confirm" bar must stay above it. Since the overlays container rises to the dialog
// layer while a dialog is open, the floating prep panel was covered by it and Confirm could not be
// clicked (consolidated smoke run, seeds 7007/81). The prefix is "before" on the commit without the fix.
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

test("prepared Confirm bar above the real question's dialog", async ({ page }, testInfo) => {
  const game = await openWaiting(page, { name: "Technology", card: "pok7technology" });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  await save(page);
  if (prefix === "before") {
    // The layering without the fix (the prep panel one layer above the old overlays layer).
    await page.addStyleTag({ content: ".secondary-prep { z-index: calc(var(--layer-modal, 30) + 1) !important; }" });
  }
  push(game, {
    version: 42,
    history: { cursor: 41, redo_count: 0, generation: 0 },
    choice: secondaryQuestion("pok7technology", "spend a strategy token and 4 resources to research", "spend"),
  });
  const confirm = page.getByTestId("secondary-prepared-confirm");
  await confirm.waitFor();
  await page.waitForTimeout(300);
  const topmost = await confirm.evaluate((el) => {
    const r = el.getBoundingClientRect();
    const top = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
    return top === el || el.contains(top) ? "the Confirm button" : `${top?.getAttribute("data-testid") ?? top?.tagName}`;
  });
  const dialogOpen = await page.locator(".choice-dialog, .choice-workflow-dialog, .decision-modal").count();
  console.log(`confirm-bar ${prefix}: topmost at the button centre = ${topmost}; dialogs open = ${dialogOpen}`);
  await shot(page, testInfo, `${prefix}-1-confirm-bar`);
});
