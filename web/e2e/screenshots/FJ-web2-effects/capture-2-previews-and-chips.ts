import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { showTitleTooltip } from "../_shared/tooltip";
import { asked, both, chips, clickText, DESKTOP, openLateGame, PHONE, press, sheet, toolbar } from "./scene";
import { startWeb2, type Web2Server } from "./web2";

// One real engine game, played through the page (every seat is played here):
//   Sol takes the Leadership strategic action -> Sardakk N'orr's Coup d'Etat window (a preview row)
//   -> Sardakk plays Coup -> Yin's Sabotage window over Coup (the played card's pending effect)
//   -> Yin passes, Sol's turn ends -> Hacan drafts a tactical action at Mecatol Rex with Flank Speed
//   -> Apply to Live -> Yin's Sabotage window over Flank Speed (pending effect) -> Yin passes
//   -> Sardakk's Reverse Engineer window (a fizzle row) with Flank Speed in force in the toolbar.
// The last shots add effects to the engine's `active_effects` (several chips, one with copies): the
// page reads them from the same message, shaped as `ti4_host::view::effects` serializes them.

let server: Web2Server;
test.beforeAll(async () => {
  server = await startWeb2();
});
test.afterAll(() => server?.stop());

/** More lingering effects, worded as `project_active_effects` words them (`effects.rs`). */
const MORE_EFFECTS = [
  {
    seat: "b",
    card: "war_machine1",
    effect: "production_boost",
    until: "tactical_action",
    copies: 2,
    amount: 8,
    title: { text: "{0}", refs: { 0: { kind: "action_card", card: "war_machine1" } } },
    summary: { text: "+8 production, -1 cost (2 cards)", refs: {} },
    expiry: "until the end of this tactical action",
  },
  {
    seat: "b",
    card: "solar_flare",
    effect: "no_space_cannon",
    until: "tactical_action",
    system: "18",
    title: { text: "{0}", refs: { 0: { kind: "action_card", card: "solar_flare" } } },
    summary: { text: "Others cannot use Space Cannon against these ships", refs: {} },
    expiry: "until the end of this tactical action",
  },
  {
    seat: "h",
    effect: "cancel_hits",
    until: "combat_round",
    amount: 2,
    title: { text: "Shields Holding / Maneuvering Jets", refs: {} },
    summary: { text: "Cancels the next 2 hits", refs: {} },
    expiry: "until the end of this combat round",
  },
];

test("reaction previews, pending effects and the effects in force", async ({ page }, testInfo) => {
  test.setTimeout(240_000);
  await openLateGame(page, server);
  const step = sheet(page);

  // Sol: Leadership. Sardakk N'orr is asked whether to play Coup d'Etat.
  await clickText(step, "1 Leadership");
  await press(step, "Use Leadership");
  await asked(page, /Cancels Sol.s Leadership strategic action/);
  await both(page, testInfo, "2-coup-preview");

  // Sardakk plays Coup; Yin is asked whether to Sabotage it. The played card says what it will do.
  await clickText(step, "Coup d'Etat");
  await press(step, "Play Coup");
  await asked(page, /Cancels Sardakk N.orr.s Coup d.Etat/);
  await expect(step).not.toContainText("Not resolved yet.");
  await both(page, testInfo, "3-sabotage-coup-pending");

  // Yin passes: the Coup ends Sol's turn. Hacan drafts a tactical action at Mecatol Rex (#18).
  await press(step, "Pass");
  await asked(page, "Activate a system");
  await clickText(step, "Activate a system");
  await step.getByPlaceholder("Name or number").fill("18");
  await page.keyboard.press("Enter");
  await press(step, "Activate system");
  // In the draft the game asks Hacan whether to play Flank Speed after the activation.
  await asked(page, "Play Flank Speed");
  await both(page, testInfo, "4-draft-flank-speed", false);
  await clickText(step, "Play Flank Speed");
  await press(step, "Record");
  await expect(chips(page)).toHaveCount(1, { timeout: 20_000 });
  await press(toolbar(page), "Apply to Live");
  await page.getByRole("dialog").getByRole("button", { name: "Apply to Live" }).click();

  // Live: Yin is asked whether to Sabotage Flank Speed.
  await asked(page, /\+1 to the move value of each of Hacan.s ships during this tactical action\./);
  await both(page, testInfo, "5-sabotage-flank-speed-pending");

  // Yin passes. Flank Speed is in force; Sardakk is asked about Reverse Engineer, which would fizzle.
  await press(step, "Pass");
  await asked(page, "Playing it now would change nothing.");
  await expect(chips(page)).toHaveCount(1);
  await expect(chips(page).first()).toContainText("Flank Speed");
  await both(page, testInfo, "6-reverse-engineer-fizzle-one-effect");

  // The chip on hover: its whole text (a native title, drawn here), then the card's "?" opened.
  const chip = chips(page).first();
  await showTitleTooltip(page, chip.locator("span[title]"));
  await shot(page, testInfo, "7-chip-hover-desktop", { of: toolbar(page), pad: 90 });
  await page.evaluate(() => {
    for (const node of document.querySelectorAll("[data-screenshot-overlay]")) node.remove();
  });
  await chip.getByRole("button", { name: /Flank Speed/ }).click();
  await page.waitForTimeout(400);
  await shot(page, testInfo, "8-chip-rules-desktop");
  await page.keyboard.press("Escape");
  await page.mouse.click(1500, 900);

  // Several effects at once, one of two stacked cards: the next message of the engine carries them.
  await page.evaluate((more) => {
    (window as unknown as { __ti4Edit: (message: { view?: { active_effects?: unknown[] } }) => unknown }).__ti4Edit = (
      message,
    ) => {
      if (message.view) message.view.active_effects = [...(message.view.active_effects ?? []), ...more];
      return message;
    };
  }, MORE_EFFECTS);
  await press(step, "Pass");
  await expect(chips(page)).toHaveCount(4, { timeout: 20_000 });
  await page.waitForTimeout(600);
  await both(page, testInfo, "9-several-effects");
  await shot(page, testInfo, "9-several-effects-toolbar-desktop", { of: toolbar(page), pad: 0 });
  // The stacked War Machine, whole on hover.
  await showTitleTooltip(page, chips(page).nth(1).locator("span[title]"));
  await shot(page, testInfo, "10-copies-hover-desktop", { of: toolbar(page), pad: 90 });
  await page.evaluate(() => {
    for (const node of document.querySelectorAll("[data-screenshot-overlay]")) node.remove();
  });
  await page.setViewportSize(PHONE);
  await page.waitForTimeout(600);
  await shot(page, testInfo, "9-several-effects-toolbar-phone", { of: toolbar(page), pad: 0 });
  await page.setViewportSize(DESKTOP);
});
