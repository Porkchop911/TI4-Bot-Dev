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

// The movement is staged on the map (see e2e/shots.spec.ts): the sheet of a system has the room,
// because a draft records every change and the footer has no button.
test.describe("movement", () => {
  for (const example of ["draft-movement", "draft-movement-cases"]) {
    test(`phone movement · ${example}`, async ({ page }) => {
      await page.goto(`/?example=${example}`);
      await page.locator("#step-panel").waitFor({ state: "attached" });
      await expect(pane(page, "Map")).toHaveAttribute("aria-pressed", "true");
      await expect(page.locator("main > footer")).toHaveCount(0);
      await expect(page.locator(".galaxy .route")).toHaveCount(0);
      await shot(page, `movement/${example}-map`);
      await page.getByRole("button", { name: /^Move from Jord, system 1,/ }).tap();
      const sheet = page.getByRole("group", { name: "Move from Jord · #1" });
      await expect(sheet).toBeVisible();
      await shot(page, `movement/${example}-sheet`);
      const width = page.viewportSize()!.width;
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
      // The reset of the sheet takes back what leaves this system, and the mark on the map.
      const marks = await page.locator(".galaxy .t-staged").count();
      await sheet.getByRole("button", { name: "Reset" }).tap();
      await expect(page.locator(".galaxy .t-staged")).toHaveCount(marks - 1);
      // The check of the sheet closes it and marks the system, also when nothing leaves it.
      await sheet.getByRole("button", { name: "Done with this system" }).tap();
      await expect(sheet).toBeHidden();
      await expect(page.locator(".galaxy .t-staged")).toHaveCount(marks);
      await pane(page, "Action").click();
      await shot(page, `movement/${example}-action`);
    });
  }

  test("phone movement · Gravity Drive in place of the gravity rift", async ({ page }) => {
    await page.goto("/?example=draft-movement-cases");
    await page.getByRole("button", { name: /^Move from Lodor, system 26/ }).tap();
    await shot(page, "movement/pickup");
    await page
      .getByRole("group", { name: "Move from Lodor · #26" })
      .getByRole("button", { name: "Carrier 1 of 1", exact: true })
      .tap();
    await page.getByRole("button", { name: "Done with this system" }).tap();
    await page.getByRole("button", { name: /^Move from Vefut, system 31/ }).tap();
    const sheet = page.getByRole("group", { name: "Move from Vefut · #31" });
    await shot(page, "movement/rift");
    await sheet.getByRole("button", { name: "Gravity Drive on Carrier 1 of 1" }).tap();
    await expect(sheet.getByText("⚄ Rift roll")).toHaveCount(1);
    await shot(page, "movement/gravity-drive");
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
      page.viewportSize()!.width,
    );
  });

  test("phone movement · the active system lists what is committed to it", async ({ page }) => {
    await page.goto("/?example=draft-movement-cases");
    await page.getByRole("button", { name: /^Inspect Starpoint .*system 27/ }).tap();
    await expect(page.getByRole("region", { name: "From Jord · #1" })).toBeVisible();
    await shot(page, "movement/committed");
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
      page.viewportSize()!.width,
    );
  });

  test("phone movement · four carriers and four dreadnoughts", async ({ page }) => {
    await page.goto("/?example=draft-movement-cases");
    await page.getByRole("button", { name: /^Move from Everra, system 58/ }).tap();
    const sheet = page.getByRole("group", { name: "Move from Everra · #58" });
    await shot(page, "movement/many-ships-empty");
    for (const index of [1, 2, 3, 4]) {
      await sheet.getByRole("button", { name: `Carrier ${index} of 4`, exact: true }).tap();
    }
    await sheet.getByRole("button", { name: "Dreadnought 1 of 4", exact: true }).tap();
    await sheet.getByRole("button", { name: "Dreadnought 2 of 4", exact: true }).tap();
    await sheet.getByRole("button", { name: /^Load Fighter, Space area/ }).tap();
    await sheet.getByRole("button", { name: "Load Fighter on Carrier 2 of 4" }).first().tap();
    await shot(page, "movement/many-ships-one-loaded");
    await sheet.getByRole("button", { name: "Fill with Fighter" }).tap();
    await sheet.getByRole("button", { name: /^Load Infantry, Planet Everra/ }).tap();
    await sheet.getByRole("button", { name: "Fill with Infantry" }).tap();
    await expect(sheet).toContainText("6 ships");
    await shot(page, "movement/many-ships");
    await page
      .getByRole("group", { name: "Map view" })
      .getByRole("button", { name: "Space combat" })
      .tap();
    await sheet.getByRole("button", { name: "Done with this system" }).tap();
    await shot(page, "movement/many-ships-space");
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
      page.viewportSize()!.width,
    );
  });
});

test.describe("flows", () => {
  test("the three panes", async ({ page }) => {
    await page.goto("/?example=live-strategy-4");
    for (const name of ["Map", "Players", "Action"]) {
      await pane(page, name).click();
      await expect(pane(page, name)).toHaveAttribute("aria-pressed", "true");
      await shot(page, `flow/pane-${name.toLowerCase()}`);
    }
  });

  test("a tap on a system opens the inspector; two fingers zoom", async ({ page }) => {
    await page.goto("/?example=live-strategy-4");
    await pane(page, "Map").click();
    const svg = page.locator(".galaxy svg");
    const before = await svg.getAttribute("viewBox");
    const box = (await svg.boundingBox())!;
    const cx = box.x + box.width / 2;
    const cy = box.y + box.height / 2;
    // Two fingers that move apart. Playwright has no pinch, so the pointer events are sent.
    await svg.evaluate(
      (element, [x, y]) => {
        const send = (type: string, id: number, dx: number) =>
          (type === "pointerdown" ? element : document).dispatchEvent(
            new PointerEvent(type, {
              pointerId: id,
              pointerType: "touch",
              button: 0,
              clientX: x + dx,
              clientY: y,
              bubbles: true,
            }),
          );
        send("pointerdown", 1, -40);
        send("pointerdown", 2, 40);
        send("pointermove", 1, -120);
        send("pointermove", 2, 120);
        send("pointerup", 1, -120);
        send("pointerup", 2, 120);
      },
      [cx, cy],
    );
    const after = await svg.getAttribute("viewBox");
    const width = (viewBox: string | null) => Number(viewBox!.split(" ")[2]);
    expect(width(after) * 2.9).toBeLessThan(width(before));
    await shot(page, "flow/pinch");
    await page.getByRole("button", { name: /^Inspect .*system 40/ }).tap();
    await expect(page.getByRole("button", { name: "Close system details" })).toBeVisible();
    await shot(page, "flow/inspector");
  });

  test("reference sheets are in a menu and cover the pane", async ({ page }) => {
    await page.goto("/?example=live-strategy-4");
    await page.getByRole("button", { name: "Reference and more" }).tap();
    await page.getByRole("menuitem", { name: "Objectives" }).tap();
    await expect(page.getByRole("complementary", { name: "Objectives" })).toBeVisible();
    await shot(page, "flow/reference");
  });

  test("the other app is in the reference menu", async ({ page }) => {
    await page.goto("/?example=live-strategy-4");
    await page.getByRole("button", { name: "Reference and more" }).tap();
    await expect(page.getByRole("menuitem", { name: "Local game" })).toBeVisible();
    await shot(page, "flow/menu-other-app");
  });

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
