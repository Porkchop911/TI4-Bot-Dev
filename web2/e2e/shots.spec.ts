import { expect, test, type Locator, type Page } from "@playwright/test";
import { EXAMPLE_IDS as EXAMPLES } from "../src/mock/exampleList";

// Screenshots of every example.
// Run all: `npm run shots` (the projects "shots" and "shots-phone"; not part of `npm run e2e`).
// Run some: `npm run shots -- -g draft-movement`.
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

  // The movement is staged on the map: a system opens its fleet there, with a token for each
  // ship and a slot for each unit in a hold. A draft records every change; nothing is sent.
  for (const example of ["draft-movement", "draft-movement-cases"]) {
    test(`movement on the map · ${example}`, async ({ page }) => {
      await page.goto(`/?example=${example}`);
      const panel = page.locator("#step-panel");
      await panel.waitFor();
      // The map is quiet: a check mark on a system with a staged move, and no path until one opens.
      await expect(page.locator(".galaxy .t-staged").first()).toBeAttached();
      await expect(page.locator(".galaxy .t-mark", { hasText: "→" }).first()).toBeAttached();
      await expect(page.locator(".galaxy .route")).toHaveCount(0);
      await expect(page.locator("#step-panel footer button")).toHaveCount(0);
      await shot(page, `web2/movement/${example}`);
      // The damaged dreadnought at Tar’Mann stays in both examples: it is moved and taken back.
      await page.getByRole("button", { name: /^Move from Tar’Mann, system 23/ }).click();
      const sheet = page.getByRole("group", { name: "Move from Tar’Mann · #23" });
      const token = sheet.getByRole("button", {
        name: "Dreadnought (damaged) 1 of 1",
        exact: true,
      });
      await expect(token).toHaveAttribute("aria-pressed", "false");
      await token.click();
      await expect(token).toHaveAttribute("aria-pressed", "true");
      await expect(page.locator(".galaxy .route.staged").first()).toBeAttached();
      await shot(page, `web2/movement/${example}-sheet`);
      // The reset of the sheet takes back the ships that leave this system.
      await sheet.getByRole("button", { name: "Reset" }).click();
      await expect(sheet.getByRole("button", { name: "Reset" })).toBeDisabled();
      await expect(token).toHaveAttribute("aria-pressed", "false");
    });
  }

  test("movement with Gravity Drive in place of the gravity rift", async ({ page }) => {
    await page.goto("/?example=draft-movement-cases");
    await page.getByRole("button", { name: /^Move from Vefut, system 31/ }).click();
    const sheet = page.getByRole("group", { name: "Move from Vefut · #31" });
    const drive = sheet.getByRole("button", { name: "Gravity Drive on Carrier 1 of 1" });
    // The carrier of Lodor has Gravity Drive. It gives it back when it stays.
    await expect(drive).toBeDisabled();
    await expect(sheet.getByText("⚄ Rift roll")).toHaveCount(2);
    await page.getByRole("button", { name: /^Move from Lodor, system 26/ }).click();
    await page
      .getByRole("group", { name: "Move from Lodor · #26" })
      .getByRole("button", { name: "Carrier 1 of 1", exact: true })
      .click();
    await page.getByRole("button", { name: /^Move from Vefut, system 31/ }).click();
    await shot(page, "web2/movement/rift");
    await drive.click();
    await expect(drive).toHaveAttribute("aria-pressed", "true");
    // The carrier goes round the rift: its hold is safe, and the destroyer still rolls.
    await expect(sheet.getByText("⚄ Rift roll")).toHaveCount(1);
    await expect(sheet).toContainText("via #19 Wellon");
    await shot(page, "web2/movement/gravity-drive");
  });

  test("the active system lists what is committed to it", async ({ page }) => {
    await page.goto("/?example=draft-movement-cases");
    await page.getByRole("button", { name: /^Inspect Starpoint .*system 27/ }).click();
    await expect(page.getByRole("region", { name: "From Jord · #1" })).toBeVisible();
    await shot(page, "web2/movement/committed");
    // A system in the list opens its fleet, where the controls are.
    await page.getByRole("button", { name: /^From Lodor/ }).click();
    await expect(page.getByRole("group", { name: "Move from Lodor · #26" })).toBeVisible();
  });

  test("movement of four carriers and four dreadnoughts", async ({ page }) => {
    await page.goto("/?example=draft-movement-cases");
    await page.getByRole("button", { name: /^Move from Everra, system 58/ }).click();
    const sheet = page.getByRole("group", { name: "Move from Everra · #58" });
    for (const index of [1, 2, 3, 4]) {
      await sheet.getByRole("button", { name: `Carrier ${index} of 4`, exact: true }).click();
    }
    await sheet.getByRole("button", { name: "Dreadnought 1 of 4", exact: true }).click();
    // The chosen kind of unit goes into the slot that is tapped: every ship has its own hold.
    await sheet.getByRole("button", { name: /^Load Fighter, Space area/ }).click();
    await sheet.getByRole("button", { name: "Load Fighter on Carrier 2 of 4" }).first().click();
    await expect(sheet).toContainText("5 ships · 1 cargo");
    await shot(page, "web2/movement/many-ships-one-loaded");
    // "Fill" loads the chosen kind on every ship that has room: 8 fighters, then the infantry.
    await sheet.getByRole("button", { name: "Fill with Fighter" }).click();
    await expect(sheet).toContainText("5 ships · 8 cargo");
    await sheet.getByRole("button", { name: /^Load Infantry, Planet Everra/ }).click();
    await sheet.getByRole("button", { name: "Fill with Infantry" }).click();
    await expect(sheet).toContainText("5 ships · 16 cargo");
    await shot(page, "web2/movement/many-ships");
    for (const view of ["Space combat", "Ground combat"]) {
      await page
        .getByRole("group", { name: "Map view" })
        .getByRole("button", { name: view })
        .click();
      await shot(page, `web2/movement/many-ships-${view.split(" ")[0].toLowerCase()}`);
    }
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

  test("rule text opens on hover and stays on click", async ({ page }) => {
    await page.goto("/?example=live-strategy-7");
    const hint = page.getByRole("button", { name: "Rules: Technology" });
    await expect(page.getByRole("note")).toHaveCount(0);
    await hint.hover();
    await expect(page.getByRole("note")).toContainText("Research 1 technology");
    await hint.click();
    await page.mouse.move(10, 10);
    await expect(page.getByRole("note")).toBeVisible();
    await shot(page, "web2/flow/rules-hint");
  });

  test("apply a draft", async ({ page }) => {
    await page.goto("/?example=draft-combat");
    await page.locator("header").getByRole("button", { name: "Apply to Live" }).click();
    await shot(page, "web2/flow/apply-dialog");
    await page.getByRole("dialog").getByRole("button", { name: "Apply to Live" }).click();
    await shot(page, "web2/flow/applied");
  });

  test("secondary, log and history", async ({ page }) => {
    await page.goto("/?example=live-secondary-7");
    await page.getByRole("button", { name: "Simulate Alex (demo)" }).click();
    await shot(page, "web2/flow/secondary");
    await page
      .getByRole("navigation", { name: "Reference" })
      .getByRole("button", { name: "Log", exact: true })
      .click();
    await shot(page, "web2/flow/log");
    await page
      .getByRole("complementary", { name: "Log" })
      .getByRole("button", { name: "Inspect", exact: true })
      .first()
      .click();
    await shot(page, "web2/flow/history");
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

  test.describe("strategy cards", () => {
    const panel = (page: Page) => page.locator("#step-panel");
    const start = async (page: Page, example: string) => {
      await page.goto(`/?example=${example}`);
      await panel(page).waitFor();
    };
    const fits = async (page: Page) => {
      const overflow = await page
        .locator("#step-panel > div.overflow-y-auto")
        .evaluate((element) => element.scrollHeight - element.clientHeight);
      expect(overflow, "the step content needs vertical scroll").toBeLessThanOrEqual(0);
    };

    test("Technology: the tree, the second technology and its payment", async ({ page }) => {
      await start(page, "live-strategy-7");
      const cell = (name: string) => panel(page).getByRole("button", { name, exact: true });
      await expect(cell("Graviton Laser System, 1 prerequisite missing")).toBeDisabled();
      await cell("Sarween Tools").click();
      await cell("Graviton Laser System").click();
      await expect(page.getByRole("group", { name: "Payment" })).toContainText("0 / 6");
      await page.getByRole("button", { name: "Auto-pay" }).click();
      await fits(page);
      await shot(page, "web2/flow/strategy-technology");
      await page.keyboard.press("Enter");
      await expect(panel(page)).toContainText("researched Sarween Tools and Graviton Laser System");
    });

    test("Technology secondary: the tree and the payment fit", async ({ page }) => {
      await start(page, "live-secondary-7");
      await panel(page).getByRole("button", { name: "Sling Relay", exact: true }).click();
      await page.getByRole("button", { name: "Auto-pay" }).click();
      await fits(page);
      await shot(page, "web2/flow/strategy-technology-secondary");
      await page.getByRole("button", { name: /Mark draft ready/ }).click();
      await expect(panel(page)).toContainText("Follow: researched Sling Relay for 4 resources");
    });

    test("one list shows what every player did", async ({ page }) => {
      await start(page, "live-strategy-7");
      await panel(page).getByRole("button", { name: "Sarween Tools", exact: true }).click();
      await page.keyboard.press("Enter");
      await expect(panel(page)).toContainText("Jamie researched Sarween Tools");
      for (let seat = 0; seat < 3; seat++) {
        await page.getByRole("button", { name: /Simulate/ }).click();
      }
      await expect(panel(page)).toContainText("Followed");
      await expect(panel(page)).toContainText("Waits for");
      await fits(page);
      await shot(page, "web2/flow/strategy-seat-list");
    });
  });

  for (const viewer of ["hacan", "observer"]) {
    test(`live combat as ${viewer}`, async ({ page }) => {
      await page.goto(`/?example=live-combat&viewer=${viewer}`);
      await page.locator("#step-panel").waitFor();
      await shot(page, `web2/flow/combat-${viewer}`);
    });
  }
});
