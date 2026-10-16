import { expect, test, type Locator, type Page } from "@playwright/test";
import { EXAMPLE_IDS as EXAMPLES } from "../src/mock/exampleList";

// Screenshots of every example.
// Run all: `npm run shots` (the projects "shots" and "shots-phone"; not part of `npm run e2e`).
// Run some: `npm run shots -- -g draft-production`.
// One size: the design target is 1920×1080 or larger. Narrower screens are a non-goal (AGENTS.md).
const SIZE = { width: 1920, height: 1080 };

const shot = (page: Page, name: string) =>
  page.screenshot({ path: `shots/${name}.png`, animations: "disabled" });

/**
 * Plays the open space battle with the first control that is offered, until `stop` is on the page.
 * The window at the start of a round is passed.
 */
async function advanceBattle(page: Page, stop: Locator) {
  const panel = page.locator("#step-panel");
  const steps = [
    panel.getByRole("button", { name: "Pass" }),
    panel.getByRole("button", { name: "Roll combat dice" }),
    panel.getByRole("button", { name: /^(Sustain damage|Destroy)/ }).and(page.locator(":enabled")),
    panel.getByRole("button", { name: /^Assign \d+ hits?$/ }).and(page.locator(":enabled")),
    panel.getByRole("button", { name: /^Simulate .* \(demo\)$/ }),
  ];
  for (let turn = 0; turn < 40 && !(await stop.count()); turn++) {
    for (const step of steps) {
      if (await step.count()) {
        await step.first().click();
        break;
      }
    }
  }
}

test.use({ viewport: SIZE });

test.describe("examples", () => {
  for (const example of EXAMPLES) {
    test(`web2 ${example}`, async ({ page }) => {
      await page.goto(`/?example=${example}`);
      await page.locator("#step-panel").waitFor();
      await shot(page, `web2/examples/${example}`);
      // The open step must fit: experts must not scroll to see the state of a step.
      const overflow = await page
        .locator("#step-panel > div.overflow-y-auto")
        .evaluate((element) => element.scrollHeight - element.clientHeight);
      expect(overflow, "the step content needs vertical scroll").toBeLessThanOrEqual(0);
    });
  }
});

test.describe("flows", () => {
  test("gallery", async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 1500 });
    await page.goto("/?gallery");
    await shot(page, "web2/gallery");
  });

  test("assign hits in space combat", async ({ page }) => {
    await page.goto("/?example=live-combat");
    // The window at the start of the round: the key stages the card, the main button plays it.
    await expect(
      page.getByRole("group", { name: "Reaction · Start of combat round 1" }),
    ).toBeVisible();
    await page.keyboard.press("1");
    await shot(page, "web2/flow/reaction-morale-boost");
    await page.getByRole("button", { name: "Play Morale Boost" }).click();
    await page.getByRole("button", { name: "Roll combat dice" }).click();
    await shot(page, "web2/flow/combat-rolled");
    for (let turn = 0; turn < 6; turn++) {
      const pick = page
        .locator("#step-panel")
        .getByRole("button", { name: /^(Sustain damage|Destroy)/ })
        .first();
      if (!(await pick.count()) || (await pick.isDisabled())) {
        break;
      }
      await pick.click();
    }
    await shot(page, "web2/flow/combat-assign");
  });

  test("pay for production on the board", async ({ page }) => {
    await page.goto("/?example=draft-production");
    await page.getByRole("button", { name: /Exhaust Jord/ }).click();
    await shot(page, "web2/flow/production-pay");
  });

  test("apply a draft", async ({ page }) => {
    await page.goto("/?example=draft-combat");
    await page.locator("header").getByRole("button", { name: "Apply to Live" }).click();
    await shot(page, "web2/flow/apply-dialog");
    await page.getByRole("dialog").getByRole("button", { name: "Apply to Live" }).click();
    await shot(page, "web2/flow/applied");
  });

  test("a retreat: the destination is chosen on the board", async ({ page }) => {
    await page.goto("/?example=live-combat");
    const panel = page.locator("#step-panel");
    await panel.getByRole("button", { name: "Pass" }).click();
    await panel.getByRole("button", { name: "Announce retreat" }).click();
    const main = panel.getByRole("button", { name: "Choose a destination" });
    await advanceBattle(page, main);
    await expect(main).toBeDisabled();
    await expect(page.getByRole("button", { name: /^Retreat to / })).toHaveCount(2);
    await page.getByRole("button", { name: /^Retreat to .*system 23,/ }).click();
    await shot(page, "web2/flow/retreat-board");
    await page.keyboard.press("Escape");
    await expect(main).toBeDisabled();
    await page.getByRole("button", { name: /^Retreat to .*system 1,/ }).click();
    await page.keyboard.press("Enter");
    await expect(panel.getByText("Jamie retreated to Jord · #1.")).toBeVisible();
  });

  test("Direct Hit is the same reaction window in a battle", async ({ page }) => {
    await page.goto("/?example=draft-combat");
    const panel = page.locator("#step-panel");
    await page.locator("header").getByRole("button", { name: "Apply to Live" }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Apply to Live" }).click();
    const window = panel.getByRole("group", { name: /^Reaction · After Alex/ });
    await advanceBattle(page, window);
    await expect(window).toBeVisible();
    await expect(panel.getByRole("button", { name: "Pass" })).toBeVisible();
    // The battle table must still fit under the window.
    const overflow = await panel
      .locator("> div.overflow-y-auto")
      .evaluate((element) => element.scrollHeight - element.clientHeight);
    expect(overflow, "the step content needs vertical scroll").toBeLessThanOrEqual(0);
    await page.keyboard.press("1");
    await shot(page, "web2/flow/reaction-direct-hit");
    await page.keyboard.press("Enter");
    await expect(window).toHaveCount(0);
  });

  test("auto-pay stages a payment of a production on the map", async ({ page }) => {
    const panel = page.locator("#step-panel");
    const bar = page.getByRole("group", { name: "Payment" });
    await page.goto("/?example=draft-production");
    await bar.getByRole("button", { name: "Auto-pay" }).click();
    await expect(panel.locator("footer")).toContainText("Payment");
    await shot(page, "web2/flow/auto-pay-production");
  });

  for (const viewer of ["hacan", "observer"]) {
    test(`live combat as ${viewer}`, async ({ page }) => {
      await page.goto(`/?example=live-combat&viewer=${viewer}`);
      await page.locator("#step-panel").waitFor();
      await shot(page, `web2/flow/combat-${viewer}`);
    });
  }
});
