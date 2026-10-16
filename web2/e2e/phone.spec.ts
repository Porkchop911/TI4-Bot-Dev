import { expect, test, type Page } from "@playwright/test";
import { EXAMPLE_IDS as EXAMPLES } from "../src/mock/exampleList";

// The phone shell on a Pixel 9 in portrait: one pane at a time, the footer of the action under each.
// Run: `npm run shots` (with the desktop), or `WEB2_SHOTS=1 npx playwright test --project=shots-phone`.
// The screenshots go to shots/phone/.

const shot = (page: Page, name: string) =>
  page.screenshot({ path: `shots/phone/${name}.png`, animations: "disabled" });
const pane = (page: Page, name: string) =>
  page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });

test.describe("examples", () => {
  for (const example of EXAMPLES) {
    test(`phone ${example}`, async ({ page }) => {
      await page.goto(`/?example=${example}`);
      // The panel is in the page but not in view while the choice is on the map.
      await page.locator("#step-panel").waitFor({ state: "attached" });
      await shot(page, `examples/${example}`);
      // A pane may scroll down. Nothing is cut at the side, and the main button is in view.
      const width = page.viewportSize()!.width;
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
      const content = page.locator("#step-panel > div.overflow-y-auto");
      const overflow = await content.evaluate(
        (element) => element.scrollWidth - element.clientWidth,
      );
      expect(overflow, "the step content is wider than the screen").toBeLessThanOrEqual(0);
      for (const button of await page.locator("main > footer button").all()) {
        await expect(button).toBeInViewport({ ratio: 1 });
      }
      await pane(page, "Action").click();
      await shot(page, `action/${example}`);
    });
  }
});

test.describe("flows", () => {
  test("the app can be installed", async ({ page, request }) => {
    await page.goto("/");
    const href = await page.locator('link[rel="manifest"]').getAttribute("href");
    const manifest = await (await request.get(href!)).json();
    expect(manifest.display).toBe("fullscreen");
    expect(manifest.orientation).toBe("portrait");
    for (const icon of manifest.icons) {
      expect((await request.get(icon.src)).ok(), icon.src).toBe(true);
    }
  });
});
