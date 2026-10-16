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
    await page.goto("/?example=live-strategic");
    const hint = page.getByRole("button", { name: "Rules: Leadership" });
    await expect(page.getByRole("note")).toHaveCount(0);
    await hint.hover();
    await expect(page.getByRole("note")).toContainText("Gain 3 command tokens");
    await hint.click();
    await page.mouse.move(10, 10);
    await expect(page.getByRole("note")).toBeVisible();
    await shot(page, "web2/flow/rules-hint");
  });

  test("pay on the board only", async ({ page }) => {
    await page.goto("/?example=live-strategic");
    await expect(page.locator("#step-panel").getByRole("checkbox")).toHaveCount(0);
    await page.getByRole("button", { name: /Exhaust Jord/ }).click();
    await expect(page.locator("#step-panel")).toContainText("Jord 2");
    await shot(page, "web2/flow/strategic-pay");
  });

  test("apply a draft", async ({ page }) => {
    await page.goto("/?example=draft-combat");
    await page.locator("header").getByRole("button", { name: "Apply to Live" }).click();
    await shot(page, "web2/flow/apply-dialog");
    await page.getByRole("dialog").getByRole("button", { name: "Apply to Live" }).click();
    await shot(page, "web2/flow/applied");
  });

  test("secondary, log and history", async ({ page }) => {
    await page.goto("/?example=live-secondary");
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

  test("choose an action by key, go back, end the turn", async ({ page }) => {
    await page.goto("/?example=live-picker");
    const panel = page.locator("#step-panel");
    await panel.waitFor();
    // A tactical action opens as a private draft. Esc goes back; nothing was sent.
    await page.keyboard.press("t");
    await expect(panel.getByRole("heading", { name: "Activation", exact: true })).toBeVisible();
    await shot(page, "web2/flow/picker-tactical-draft");
    await page.keyboard.press("Escape");
    await expect(panel.getByRole("heading", { name: "Choose an action" })).toBeVisible();
    // A strategic action is staged, not sent. Esc clears the pools, then the bought token, then
    // drops the action.
    await page.keyboard.press("1");
    await expect(page.getByText("Not sent")).toBeVisible();
    await shot(page, "web2/flow/picker-strategic-staged");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await expect(panel.getByRole("heading", { name: "Choose an action" })).toBeVisible();
    // A component action, sent with Enter. The turn stays open until End turn.
    await page.keyboard.press("a");
    await page.keyboard.press("1");
    await page.getByRole("button", { name: /Choose Jord/ }).click();
    await page.keyboard.press("Enter");
    await expect(panel).toContainText("Before you end your turn");
    await shot(page, "web2/flow/closing-component");
    await page.keyboard.press("Enter");
    await expect(panel.getByRole("heading", { name: "Blair’s turn" })).toBeVisible();
    await shot(page, "web2/flow/turn-ended");
  });

  test("pass after the strategy card is used", async ({ page }) => {
    await page.goto("/?example=live-picker");
    const panel = page.locator("#step-panel");
    await panel.waitFor();
    await page.keyboard.press("1");
    const planets = page.getByRole("button", { name: /^Exhaust / });
    await planets.nth(0).click();
    await planets.nth(1).click();
    await page.keyboard.press("Enter");
    const simulate = page.getByRole("button", { name: /^Simulate .* \(demo\)$/ });
    const end = page.getByRole("button", { name: "End turn" });
    for (let seat = 0; seat < 8 && !(await end.count()); seat++) {
      await simulate.click();
    }
    await shot(page, "web2/flow/closing-strategic");
    await page.keyboard.press("Enter");
    await simulate.click();
    await expect(panel.getByRole("button", { name: /^Take no more actions/ })).toBeEnabled();
    await shot(page, "web2/flow/picker-pass");
    await page.keyboard.press("p");
    await expect(page.locator("header")).toContainText("You passed");
  });

  test("four players: two strategy cards, action cards in a second step", async ({ page }) => {
    await page.goto("/?example=live-picker-4p");
    const panel = page.locator("#step-panel");
    await panel.waitFor();
    await expect(page.getByRole("region", { name: "Players" }).getByRole("row")).toHaveCount(5);
    // 5 Trade is used: its key does nothing. A, then 2, picks the second action card.
    await page.keyboard.press("5");
    await page.keyboard.press("a");
    await expect(panel.getByRole("heading", { name: "Play an action card" })).toBeVisible();
    await shot(page, "web2/flow/4p-action-cards");
    await page.keyboard.press("4");
    await expect(panel.getByRole("heading", { name: "Ghost Ship" })).toBeVisible();
    await shot(page, "web2/flow/4p-action-card-staged");
    // Esc goes one level at a time: the list of action cards, then the actions.
    await page.keyboard.press("Escape");
    await expect(panel.getByRole("heading", { name: "Play an action card" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(panel.getByRole("heading", { name: "Choose an action" })).toBeVisible();
    await page.keyboard.press("a");
    await page.keyboard.press("4");
    await page.keyboard.press("Enter");
    await expect(panel).toContainText("Before you end your turn");
    await page.keyboard.press("Enter");
    await page.getByRole("button", { name: /^Simulate .* \(demo\)$/ }).click();
    await expect(panel.getByRole("button", { name: "Play an action card" })).toBeEnabled();
    await expect(panel).toContainText("3 playable");
    await shot(page, "web2/flow/4p-next-turn");
  });

  test("decision on the board: a system for Unexpected Action", async ({ page }) => {
    await page.goto("/?example=live-picker-4p");
    const panel = page.locator("#step-panel");
    await panel.waitFor();
    await page.keyboard.press("a");
    await page.keyboard.press("2");
    await expect(panel.getByRole("heading", { name: "Choose a system" })).toBeVisible();
    await shot(page, "web2/flow/decision-system");
    await page.getByRole("button", { name: /^Choose .*system 38/ }).click();
    await expect(panel).toContainText("Selected");
    await shot(page, "web2/flow/decision-system-chosen");
    // Esc clears the choice; the card stays open.
    await page.keyboard.press("Escape");
    await expect(panel).toContainText("No system chosen");
    await page.getByRole("button", { name: /^Choose .*system 38/ }).click();
    await page.keyboard.press("Enter");
    await expect(panel).toContainText("Before you end your turn");
  });

  test("decision in the player table: a player for Spy", async ({ page }) => {
    await page.goto("/?example=live-picker-4p");
    const panel = page.locator("#step-panel");
    const players = page.getByRole("region", { name: "Players" });
    await panel.waitFor();
    await page.keyboard.press("a");
    await page.keyboard.press("3");
    await expect(players.getByRole("button", { name: /^Choose Casey/ })).toBeDisabled();
    await shot(page, "web2/flow/decision-player");
    await players.getByRole("button", { name: /^Choose Blair/ }).click();
    await page.mouse.move(400, 900);
    await expect(panel).toContainText("you take 1 at random");
    await shot(page, "web2/flow/decision-player-chosen");
    await page.keyboard.press("Enter");
    await expect(panel).toContainText("Jamie took 1 action card from Blair");
  });

  test("decision in a list: the answers of an offer", async ({ page }) => {
    await page.goto("/?example=live-decision-offer");
    const panel = page.locator("#step-panel");
    await panel.waitFor();
    await page.keyboard.press("2");
    await expect(panel.getByRole("button", { name: /^Convert commodities/ })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await shot(page, "web2/flow/decision-offer-chosen");
    await page.keyboard.press("Enter");
    await expect(panel).toContainText("Trade goods 2 → 3");
  });

  test("decision with counters: remove ships over the fleet supply", async ({ page }) => {
    await page.goto("/?example=live-decision-units");
    const panel = page.locator("#step-panel");
    await panel.waitFor();
    await expect(panel.getByRole("button", { name: "Remove ships" })).toBeDisabled();
    await panel.getByRole("button", { name: "Add removed cruiser" }).click();
    await shot(page, "web2/flow/decision-units-staged");
    await page.keyboard.press("Enter");
    await expect(panel).toContainText("Fleet supply 2 / 2");
  });

  test("reaction to another player's card: Pass, or stage a card and play it", async ({ page }) => {
    await page.goto("/?example=live-reaction");
    const panel = page.locator("#step-panel");
    const window = panel.getByRole("group", { name: /^Reaction · After Blair plays/ });
    await expect(window).toBeVisible();
    await expect(panel.getByRole("button", { name: "Pass" })).toBeVisible();
    await shot(page, "web2/flow/reaction-window");
    await page.keyboard.press("1");
    await expect(panel.getByRole("button", { name: "Play Sabotage" })).toBeVisible();
    await shot(page, "web2/flow/reaction-staged");
    await page.keyboard.press("Escape");
    await expect(panel.getByRole("button", { name: "Pass" })).toBeVisible();
    // The setting of a card is in the Cards sheet; the window links to it.
    await window.getByRole("button", { name: "Offer settings" }).click();
    const sheet = page.getByRole("complementary", { name: "Cards" });
    const setting = sheet.getByRole("combobox", { name: "Offer Sabotage" });
    await setting.selectOption("never");
    await expect(setting).toHaveValue("never");
    await shot(page, "web2/flow/never-offer");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Enter");
    await expect(panel).toContainText("Blair gained 3 trade goods");
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

  test("auto-pay stages a payment on the map; it can be cleared", async ({ page }) => {
    await page.goto("/?example=live-strategic");
    const panel = page.locator("#step-panel");
    // The controls of a payment are on the board; the footer of the panel has its line.
    const bar = page.getByRole("group", { name: "Payment" });
    await expect(bar).toContainText("0 / 3");
    await expect(bar).toContainText("3 more");
    await bar.getByRole("button", { name: "Auto-pay" }).click();
    await expect(bar).toContainText("✓ Paid");
    await expect(panel.locator("footer")).toContainText("3 / 3");
    await expect(panel.getByRole("button", { name: /Resolve primary/ })).toBeEnabled();
    await shot(page, "web2/flow/auto-pay");
    await bar.getByRole("button", { name: "Clear payment" }).click();
    await expect(bar).toContainText("0 / 3");
    await bar.getByRole("button", { name: "Add trade good" }).click();
    await expect(panel.locator("footer")).toContainText("1 trade good");
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

    test("Diplomacy: system on the board, then planets; Esc clears; Enter sends", async ({
      page,
    }) => {
      await start(page, "live-strategy-2");
      await expect(panel(page)).toContainText("No system chosen");
      await page.getByRole("button", { name: /^Choose .*Starpoint/ }).click();
      await expect(panel(page)).toContainText("Alex, Blair and 5 more place a command token here");
      await page.getByRole("button", { name: /^Choose Vefut/ }).click();
      await page.getByRole("button", { name: /^Choose Quann/ }).click();
      await expect(panel(page)).toContainText("2 / 2");
      await shot(page, "web2/flow/strategy-diplomacy");
      await page.keyboard.press("Escape");
      await expect(panel(page)).toContainText("1 / 2");
      await page.keyboard.press("Enter");
      await expect(panel(page)).toContainText("Jamie readied Vefut");
    });

    test("Politics: speaker in the player table, then the agenda cards", async ({ page }) => {
      await start(page, "live-strategy-3");
      await expect(
        page.getByRole("button", { name: /^Choose Alex.*Is the speaker/ }),
      ).toBeDisabled();
      await page.getByRole("button", { name: /^Choose Blair/ }).click();
      await expect(panel(page)).toContainText("Speaker: Alex → Blair");
      await shot(page, "web2/flow/strategy-politics-speaker");
      await page.keyboard.press("Enter");
      await expect(panel(page)).toContainText("Fleet Regulations");
      await page.keyboard.press("1");
      await page.keyboard.press("3");
      await expect(panel(page)).toContainText("2 / 2 placed");
      await panel(page).getByRole("button", { name: "Swap" }).click();
      await shot(page, "web2/flow/strategy-politics-agenda");
      await fits(page);
      await page.keyboard.press("Enter");
      await expect(panel(page)).toContainText("made Blair the speaker");
    });

    test("Construction: structure by key, planets on the board", async ({ page }) => {
      await start(page, "live-strategy-4");
      await page.keyboard.press("1");
      await page.getByRole("button", { name: /^Choose Vefut/ }).click();
      await page.getByRole("button", { name: /^Choose Starpoint/ }).click();
      await expect(panel(page)).toContainText("Starpoint has 0 space docks and 1 PDS");
      await shot(page, "web2/flow/strategy-construction");
      await page.keyboard.press("Enter");
      await expect(panel(page)).toContainText(
        "placed a space dock on Vefut and a PDS on Starpoint",
      );
    });

    test("Trade: players in the player table", async ({ page }) => {
      await start(page, "live-strategy-5");
      await page.getByRole("button", { name: /^Choose Alex/ }).click();
      await page.getByRole("button", { name: /^Choose Casey/ }).click();
      await expect(
        page.getByRole("button", { name: /^Choose Bartholomew.*Commodities full/ }),
      ).toBeDisabled();
      await expect(panel(page)).toContainText("2 chosen");
      await shot(page, "web2/flow/strategy-trade");
      await page.keyboard.press("Enter");
      await expect(panel(page)).toContainText("chose Alex and Casey for a free replenish");
    });

    test("Warfare: token on the board, pools as counters", async ({ page }) => {
      await start(page, "live-strategy-6");
      await page
        .getByRole("button", { name: /^Choose .*Corneeq/ })
        .first()
        .click();
      await page.getByRole("button", { name: "Add fleet pool token" }).click();
      await shot(page, "web2/flow/strategy-warfare");
      await page.keyboard.press("Enter");
      await expect(panel(page)).toContainText("Pools 3 · 4 · 2 → 3 · 5 · 2");
    });

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

    test("Warfare secondary: production in the draft", async ({ page }) => {
      await start(page, "live-secondary-6");
      await page.getByRole("button", { name: "Add cruiser" }).click();
      await page.getByRole("button", { name: "Add infantry" }).click();
      await page.getByRole("button", { name: "Auto-pay" }).click();
      await fits(page);
      await shot(page, "web2/flow/strategy-warfare-secondary");
    });

    test("Imperial: objective by key, the outcome below", async ({ page }) => {
      await start(page, "live-strategy-8");
      await expect(panel(page)).toContainText("Victory points 6 → 6");
      await page.keyboard.press("3");
      await expect(panel(page)).toContainText("Victory points 6 → 7");
      await shot(page, "web2/flow/strategy-imperial");
      await page.keyboard.press("Enter");
      await expect(panel(page)).toContainText("scored Expand Borders");
    });

    test("one list shows what every player did", async ({ page }) => {
      await start(page, "live-strategy-5");
      await page.keyboard.press("Enter");
      await expect(panel(page)).toContainText("Jamie gained 3 trade goods");
      for (let seat = 0; seat < 3; seat++) {
        await page.getByRole("button", { name: /Simulate/ }).click();
      }
      await expect(panel(page)).toContainText("Followed");
      await expect(panel(page)).toContainText("Waits for");
      await fits(page);
      await shot(page, "web2/flow/strategy-seat-list");
    });

    test("a secondary without a choice: Follow or Pass by key", async ({ page }) => {
      await start(page, "live-secondary-5");
      await page.keyboard.press("1");
      await expect(panel(page)).toContainText("Free · chosen by Alex");
      await shot(page, "web2/flow/strategy-trade-secondary");
      await page.keyboard.press("Enter");
      await expect(panel(page)).toContainText("Follow: replenished commodities");
    });
  });

  test("a shortcut does nothing in a text field or under an open rule text or menu", async ({
    page,
  }) => {
    await page.goto("/?example=live-picker");
    const panel = page.locator("#step-panel");
    const picker = panel.getByRole("heading", { name: "Choose an action" });
    const activation = panel.getByRole("heading", { name: "Activation", exact: true });
    await expect(picker).toBeVisible();
    // A pinned rule text takes the keys. Esc closes it and nothing else.
    await panel
      .getByRole("button", { name: /^Rules: / })
      .first()
      .click();
    await expect(page.getByRole("note")).toBeVisible();
    await page.keyboard.press("t");
    await expect(picker).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("note")).toHaveCount(0);
    await expect(picker).toBeVisible();
    await page.keyboard.press("t");
    await expect(activation).toBeVisible();
    // A text field keeps its keys.
    await page.getByPlaceholder("Name or number").focus();
    await page.keyboard.press("Escape");
    await expect(activation).toBeVisible();
    // An open menu takes Esc.
    await page.getByRole("button", { name: "More draft actions" }).click();
    await expect(page.getByRole("menu")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("menu")).toHaveCount(0);
    await expect(activation).toBeVisible();
    // With nothing open, Esc goes one level back.
    await page.keyboard.press("Escape");
    await expect(picker).toBeVisible();
  });

  test("a row that is not allowed has no key", async ({ page }) => {
    await page.goto("/?example=live-picker");
    await page.locator("#step-panel").waitFor();
    await page.keyboard.press("p");
    await page.keyboard.press("c");
    await expect(page.getByRole("heading", { name: "Choose an action" })).toBeVisible();
    await expect(page.getByRole("button", { name: /^Take no more actions/ })).toBeDisabled();
  });

  for (const viewer of ["hacan", "observer"]) {
    test(`live combat as ${viewer}`, async ({ page }) => {
      await page.goto(`/?example=live-combat&viewer=${viewer}`);
      await page.locator("#step-panel").waitFor();
      await shot(page, `web2/flow/combat-${viewer}`);
    });
  }
});
