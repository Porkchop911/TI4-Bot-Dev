import { expect, test, type Locator, type Page } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, horizontalOverflow, openTransit, prefix } from "./scene";

async function insideViewport(page: Page, locator: Locator) {
  const box = await locator.boundingBox();
  const viewport = page.viewportSize()!;
  expect(box, "element has a box").not.toBeNull();
  expect(box!.y).toBeGreaterThanOrEqual(0);
  expect(box!.y + box!.height).toBeLessThanOrEqual(viewport.height + 0.5);
  expect(box!.x).toBeGreaterThanOrEqual(0);
  expect(box!.x + box!.width).toBeLessThanOrEqual(viewport.width + 0.5);
}

async function tapTargetsAtLeast44(locator: Locator) {
  const heights = await locator.evaluateAll((els) => els.map((el) => el.getBoundingClientRect().height));
  expect(heights.length).toBeGreaterThan(0);
  for (const h of heights) expect(h).toBeGreaterThanOrEqual(44);
}

// Transit Diodes with 13 controlled planets and ground forces on 7 places (150+ options): what
// moves, where to, Confirm, at 390x844 and 360x740.
for (const size of SIZES) {
  test(`after-${size.label}`, async ({ browser, baseURL }, testInfo) => {
    test.skip(prefix !== "after", "after shots need the stepwise panel");
    const sent: { type?: string; option_id?: string }[] = [];
    const { context, page, options } = await openTransit(browser, size, baseURL, sent);
    expect(options.length).toBeGreaterThan(100);
    const panel = page.getByTestId("transit-panel");
    await panel.waitFor();
    expect(await horizontalOverflow(page)).toBeLessThanOrEqual(0);
    await insideViewport(page, panel);

    // Step 1: forces grouped by system; the list scrolls, Confirm and Finish stay visible.
    await expect(panel.getByRole("heading", { name: "#18 [Mecatol Rex]" })).toBeVisible();
    await expect(panel.getByTestId("transit-moves-left")).toHaveText("4 moves left");
    const list = page.getByTestId("transit-list");
    expect(await list.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
    await insideViewport(page, page.getByTestId("transit-confirm"));
    await insideViewport(page, page.getByTestId("transit-decline"));
    await expect(page.getByTestId("transit-confirm")).toBeDisabled();
    await tapTargetsAtLeast44(panel.locator(".transit-panel__force"));
    await tapTargetsAtLeast44(panel.locator(".transit-panel__actions .button"));
    await shot(page, testInfo, `after-1-what-moves-${size.label}`);

    // Scrolling inside the panel reaches the last system (Sol's home).
    const solMech = page.getByTestId("transit-force-1|sol_jord|mech");
    await solMech.scrollIntoViewIfNeeded();
    await insideViewport(page, solMech);
    await shot(page, testInfo, `after-2-scrolled-${size.label}`);

    // Step 2: the mech on Sol's Jord; destinations grouped by system with values and troops.
    await solMech.click();
    await expect(panel).toHaveAttribute("data-step", "2");
    await expect(page.getByTestId("transit-picked")).toContainText("Mech from Jord in #01 [Sol Home]");
    await expect(panel.getByRole("heading", { name: "#18 [Mecatol Rex]" })).toBeVisible();
    await tapTargetsAtLeast44(panel.locator(".transit-panel__destination"));
    await shot(page, testInfo, `after-3-where-to-${size.label}`);

    // Step 3: Lodor, further down the list; the summary names both ends.
    const lodor = page.getByTestId("transit-destination-lodor");
    await lodor.scrollIntoViewIfNeeded();
    await lodor.click();
    await expect(page.getByTestId("transit-summary")).toHaveText(
      "Move 1 Mech: Jord (#01 [Sol Home]) → Lodor (#26 [Lodor])",
    );
    await expect(page.getByTestId("transit-confirm")).toBeEnabled();
    expect(await horizontalOverflow(page)).toBeLessThanOrEqual(0);
    await shot(page, testInfo, `after-4-confirm-${size.label}`);

    // Hide folds the list away so the map is free; summary and Confirm stay.
    await page.getByTestId("transit-collapse").click();
    await expect(page.getByTestId("transit-list")).toHaveCount(0);
    await insideViewport(page, page.getByTestId("transit-confirm"));
    await shot(page, testInfo, `after-5-hidden-${size.label}`);

    // Confirm sends the engine's own option id.
    await page.getByTestId("transit-confirm").click();
    await expect.poll(() => sent.find((m) => m.type === "submit_choice")?.option_id).toBe("transit|1|sol_jord|mech|26|lodor");
    await context.close();
  });
}
