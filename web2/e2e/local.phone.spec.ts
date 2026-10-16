import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { activateInLive, pageErrors, seatRow, mapPlanet } from "./actions";

// Local play on a Pixel 9 in portrait. Needs the engine: `scripts/build-wasm.sh`.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");

test("phone: a game of eight is played through the decision list @slow", async ({ page }) => {
  await page.goto("/?local=3&players=8&humans=1");
  // The main button of the footer: "Send", "Activate system", "Move nothing".
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  // On a phone, Dev is an item of "Reference and more", not a button of the header.
  const savedLine = async () => {
    await page.getByRole("button", { name: "Reference and more" }).click();
    await page.getByRole("menuitem", { name: "Dev" }).click();
    const line = page.getByRole("complementary", { name: "Dev" }).getByText(/^Local game · /);
    const text = (await line.textContent()) ?? "";
    await page.keyboard.press("Escape");
    await expect(page.getByRole("complementary", { name: "Dev" })).toBeHidden();
    return text;
  };
  await expect(send).toBeDisabled();
  await page.screenshot({ path: "shots/phone/local/first-choice.png", animations: "disabled" });
  // A later movement moves nothing: the main button already says so.
  let moved = false;
  for (let count = 0; count < 12; count++) {
    const before = await savedLine();
    const system = page.locator("g.system.target");
    // The picker of the turn menu has no main button until an action is staged.
    const picker = (await send.count()) === 0;
    const moving = !picker && /Move/.test((await send.textContent()) ?? "");
    if (picker && !moved) {
      // A tactical action, so that the run has a choice on the map: the picker opens it as the
      // draft, and its system is applied. The movement is then open in Live, on the map.
      await activateInLive(page, page.locator("g.system").filter({ hasText: "#23" }));
      await page.screenshot({
        path: "shots/phone/local/system-choice.png",
        animations: "disabled",
      });
      continue;
    } else if (picker) {
      // The strategy card, then pass.
      await pane("Action").click();
      await page
        .locator("#step-panel")
        .getByRole("button", { name: /^(\d \S.*|Take no more actions in this round)$/ })
        .and(page.locator(":enabled"))
        .first()
        .click();
    } else if (!moving && (await system.count())) {
      await pane("Map").click();
      const near = system.filter({ hasText: "#23" });
      await ((await near.count()) ? near : system).first().click();
    } else if (!moving && (await send.isDisabled())) {
      await pane("Action").click();
      await page.locator("#step-panel").getByRole("button", { pressed: false }).first().click();
    } else if (moving && !moved) {
      // The first movement: the map stays open, and the fleet of a system opens in a sheet over
      // it. The sheet has the room of the footer.
      moved = true;
      await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
      await page.locator("g.system").filter({ hasText: "#01" }).click();
      const sheet = page.getByRole("group", { name: /^Move from / });
      await sheet.getByRole("button", { name: /^Advanced Carrier I 1 of / }).click();
      await expect(send).toBeHidden();
      await page.screenshot({ path: "shots/phone/local/movement.png", animations: "disabled" });
      // The check of the sheet closes it: the main button is back.
      await sheet.getByRole("button", { name: "Done with this system" }).click();
      await expect(send).toHaveText(/Move fleet/);
      await expect(page.locator(".galaxy .t-staged")).toHaveCount(1);
      // An undo of the movement stages it again.
      await send.click();
      await expect(send).not.toHaveText(/Move/);
      await pane("Action").click();
      await page.getByRole("button", { name: "Undo" }).click();
      await expect(send).toHaveText(/Move/);
      // The undo takes the player back to a step on the map: the map opens.
      await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
      await expect(pane("Map")).not.toHaveAccessibleName(/choice open/);
      await expect(send).toHaveText(/Move fleet/);
      await expect(page.locator(".galaxy .t-staged")).toHaveCount(1);
      await page.screenshot({ path: "shots/phone/local/undo.png", animations: "disabled" });
      // An undo of the activation opens the map too.
      await pane("Action").click();
      await page.getByRole("button", { name: "Undo" }).click();
      await expect(send).toHaveText(/Activate system/);
      await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
      await system.filter({ hasText: "#23" }).first().click();
      await send.click();
      // The movement opens again, with what is kept staged for it.
      await expect(send).toHaveText(/^Move (nothing|fleet)/);
    }
    // The main button is under every pane, in view.
    await expect(send).toBeInViewport({ ratio: 1 });
    await send.click();
    await expect.poll(savedLine).not.toBe(before);
    // Nothing is cut at the side.
    const width = page.viewportSize()!.width;
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  }
  expect(existsSync("shots/phone/local/system-choice.png"), "a system was chosen on the map").toBe(
    true,
  );
  expect(existsSync("shots/phone/local/movement.png"), "a ship was moved from the sheet").toBe(
    true,
  );
  await pane("Players").click();
  await page.screenshot({ path: "shots/phone/local/players.png", animations: "disabled" });
});

test("phone: a draft is staged on the map and applied from the toolbar", async ({ page }) => {
  await page.goto("/?local=3&players=8&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const row = (name: string | RegExp) => page.locator("#step-panel").getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  await expect(send).toBeDisabled();
  await pane("Action").click();
  await row(/^8\. Imperial$/).click();
  await send.click();
  await expect(row(/^Activate a system$/)).toBeVisible();

  await page
    .getByRole("navigation", { name: "Workspace" })
    .getByRole("button", { name: "Draft" })
    .click();
  // The system is chosen on the map, and the main button records it.
  // The choice is on the map: the draft of the player opens it.
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  await system("23").click();
  await expect(send).toHaveText(/Activate system/);
  await expect(send).toBeInViewport({ ratio: 1 });
  // The click shows what is in the system too.
  const details = page.getByRole("button", { name: "Close system details" });
  await expect(details).toBeVisible();
  await expect(page.getByRole("heading", { name: /· 23$/ })).toBeVisible();
  await fits();
  await page.screenshot({
    path: "shots/phone/local/activation-details.png",
    animations: "disabled",
  });
  await send.click();
  // The details close with the activation: the movement has the whole map.
  await expect(details).toBeHidden();
  // Every change of the movement is recorded, and nothing follows it here: the footer has no
  // main button. The controls of the draft are in the toolbar.
  await system("01").click();
  const sheet = page.getByRole("group", { name: /^Move from / });
  // The fleet sheet has what is in the system, as the inspector has it.
  await expect(sheet.getByText("4 / 2")).toBeAttached();
  await sheet.getByRole("button", { name: /^Advanced Carrier I 1 of / }).click();
  await sheet.getByRole("button", { name: "Done with this system" }).click();
  await expect(send).toHaveCount(0);
  await expect(system("23")).toHaveAttribute("aria-label", /1 Sol ship/);
  await fits();
  await page.screenshot({ path: "shots/phone/local/draft.png", animations: "disabled" });
  const apply = page.getByRole("button", { name: "Apply to Live" });
  await expect(apply).toBeInViewport({ ratio: 1 });
  await apply.click();
  await fits();
  await page.screenshot({ path: "shots/phone/local/draft-apply.png", animations: "disabled" });
  await page.getByRole("dialog").getByRole("button", { name: "Apply to Live" }).click();
  await expect(apply).toHaveCount(0);
  await expect(system("23")).toHaveAttribute("aria-label", /1 Sol ship/);
  await fits();
});

test("phone: the recorded activation of a draft is edited in the width", async ({ page }) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?local=3&players=8&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const row = (name: string | RegExp) => page.locator("#step-panel").getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  await expect(send).toBeDisabled();
  await pane("Action").click();
  await row(/^8\. Imperial$/).click();
  await send.click();
  await row(/^Activate a system$/).click();
  await pane("Map").click();
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);
  await system("23").click();
  await send.click();
  await system("01").click();
  const sheet = page.getByRole("group", { name: /^Move from / });
  await sheet.getByRole("button", { name: /^Advanced Carrier I 1 of / }).click();
  await sheet.getByRole("button", { name: "Done with this system" }).click();
  await expect(system("23")).toHaveAttribute("aria-label", /1 Sol ship/);

  // The activation opens from its tab, with "Edit activation" in its heading.
  await pane("Action").click();
  await page.getByRole("tab", { name: /Activation/ }).click();
  await row("Edit activation").click();
  const cancel = page.locator("footer").getByRole("button", { name: "Cancel edits" });
  await expect(cancel).toBeInViewport({ ratio: 1 });
  await expect(send).toBeInViewport({ ratio: 1 });
  await fits();
  await page.screenshot({
    path: "shots/phone/local/draft-edit-activation.png",
    animations: "disabled",
  });
  // Another system on the map replaces the activation; the movement stays.
  await pane("Map").click();
  await system("71").click();
  await send.click();
  await expect(system("71")).toHaveAttribute("aria-label", /1 Sol ship/);
  await fits();
});

test("phone: a draft entry that no longer fits is marked in its step and removed", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?local=3&players=8&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const panel = page.locator("#step-panel");
  const row = (name: string | RegExp) => panel.getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  await expect(send).toBeDisabled();
  await pane("Action").click();
  await row(/^8\. Imperial$/).click();
  await send.click();
  await row(/^Activate a system$/).click();
  await pane("Map").click();
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);
  await system("23").click();
  await send.click();
  await system("01").click();
  const sheet = page.getByRole("group", { name: /^Move from / });
  await sheet.getByRole("button", { name: /^Advanced Carrier I 1 of / }).click();
  await sheet.getByRole("button", { name: /^Destroyer I 1 of / }).click();
  await sheet.getByRole("button", { name: "Done with this system" }).click();
  await expect(system("23")).toHaveAttribute("aria-label", /2 Sol ships/);

  // Only the destroyer reaches #27: the carrier is marked in the movement, with why.
  await pane("Action").click();
  await page.getByRole("tab", { name: /Activation/ }).click();
  await row("Edit activation").click();
  await pane("Map").click();
  await system("27").click();
  await send.click();
  await pane("Action").click();
  await expect(page.getByRole("tab", { name: /Movement/ })).toHaveAttribute(
    "data-state",
    "needs-review",
  );
  const review = panel.getByRole("group", { name: "Needs review" });
  await expect(review.getByText(/^✗ Advanced Carrier I from .+ · #01$/)).toBeVisible();
  const remove = review.getByRole("button", { name: "Remove" });
  await expect(remove).toBeInViewport({ ratio: 1 });
  await expect(page.locator("footer").getByText(/no longer fits: remove it/)).toBeVisible();
  await fits();
  await page.screenshot({ path: "shots/phone/local/draft-review.png", animations: "disabled" });
  await remove.click();
  await expect(review).toHaveCount(0);
  await expect(system("27")).toHaveAttribute("aria-label", /1 Sol ship/);
  await fits();
});

test("phone: the Objectives sheet has the eight seats in its width, and a filter by seat", async ({
  page,
}) => {
  await page.goto("/?scenario=late-game&humans=1");
  await expect(page.getByRole("heading", { name: "Choose an action" })).toBeVisible();
  await page.getByRole("button", { name: "Reference and more" }).click();
  await page.getByRole("menuitem", { name: "Objectives" }).click();
  const sheet = page.getByRole("complementary", { name: "Objectives" });
  const seats = sheet.getByRole("group", { name: "Victory points" }).getByRole("button");
  await expect(seats).toHaveCount(8);
  // The sheet has the room of the footer and of the tabs.
  await expect(page.locator("main > footer")).toHaveCount(0);
  await expect(page.getByRole("navigation", { name: "Panes" })).toHaveCount(0);
  // Nothing is cut at the side, of the page or of the sheet. The viewer is first.
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(412);
  const scroller = sheet.locator("div.overflow-auto");
  expect(await scroller.evaluate((element) => element.scrollWidth - element.clientWidth)).toBe(0);
  await expect(seats.nth(0)).toHaveAccessibleName(/\(you\)/);
  await expect(seats.nth(7)).toBeInViewport({ ratio: 1 });
  await page.screenshot({ path: "shots/phone/local/objectives.png", animations: "disabled" });
  // A seat is the filter: the objectives that it scored are not shown.
  const cards = sheet
    .getByRole("region", { name: "Public objectives · stage I", exact: true })
    .getByRole("article");
  await expect(cards).toHaveCount(5);
  await seats.nth(1).click();
  await expect(seats.nth(1)).toHaveAttribute("aria-pressed", "true");
  expect(await cards.count()).toBeLessThan(5);
  await page.screenshot({ path: "shots/phone/local/objectives-seat.png", animations: "disabled" });
  // The standings stay in view when the sheet scrolls.
  await seats.nth(1).click();
  await scroller.evaluate((element) => element.scrollTo(0, element.scrollHeight));
  await expect(seats.nth(0)).toBeInViewport();
  await page.screenshot({ path: "shots/phone/local/objectives-end.png", animations: "disabled" });
  await sheet.getByRole("button", { name: "Close Objectives" }).click();
  await expect(page.getByRole("navigation", { name: "Panes" })).toBeVisible();
});

test("phone: the Dev sheet plays another seat", async ({ page }) => {
  await page.goto("/?local=3&players=8&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  await expect(send).toBeDisabled();
  const sheet = page.getByRole("complementary", { name: "Dev" });
  const seats = sheet.getByRole("region", { name: "Play as" });
  const open = async () => {
    await page.getByRole("button", { name: "Reference and more" }).click();
    await page.getByRole("menuitem", { name: "Dev" }).click();
    await expect(seats).toBeVisible();
  };
  await open();
  await expect(seats.getByRole("button", { name: "Seat A" })).toContainText("✓ Playing");
  // Nothing of the sheet is cut at the side.
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(412);
  await page.screenshot({ path: "shots/phone/local/dev-play-as.png", animations: "disabled" });
  await seats.getByRole("button", { name: "Seat C" }).click();
  await expect(sheet).toBeHidden();
  await open();
  await expect(seats.getByRole("button", { name: "Seat C" })).toContainText("✓ Playing");
  await expect(seats.getByRole("button", { name: "Seat A" })).toContainText("Random");
});

test("phone: the Dev sheet leads to a game on the server, and its menu leads back", async ({
  page,
}) => {
  await page.goto("/?local=3&players=8&humans=1");
  await expect(page.locator('footer button[aria-keyshortcuts="Enter"]')).toBeDisabled();
  await page.getByRole("button", { name: "Reference and more" }).click();
  await page.getByRole("menuitem", { name: "Dev" }).click();
  const row = page
    .getByRole("complementary", { name: "Dev" })
    .getByRole("region", { name: "Multiplayer" });
  await row.scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(412);
  await page.screenshot({ path: "shots/phone/local/dev-multiplayer.png", animations: "disabled" });
  await row.getByRole("button", { name: /^Multiplayer/ }).click();
  await expect(page).toHaveURL(/\?online=[^&]+&seat=[^&]+$/);
  await expect(page.getByText("Round 1 · Strategy phase")).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(412);
  // A game on the server has no Dev item; the start screen is the last item of the menu.
  await page.getByRole("button", { name: "Reference and more" }).click();
  await expect(page.getByRole("menuitem", { name: "Dev" })).toHaveCount(0);
  await expect(page.getByRole("menuitem").last()).toHaveText("Start");
  await page.screenshot({ path: "shots/phone/online/menu.png", animations: "disabled" });
});

test("phone: a movement into a fleet of another seat has the odds of the space combat", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?scenario=late-game&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  // Seat C has two cruisers and a destroyer in 32.
  await activateInLive(page, system("32"));
  await expect(send).toHaveText(/Move nothing/);
  await system("24").click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await fleet.getByRole("button", { name: "Dreadnought II 1 of 1" }).click();
  await fleet.getByRole("button", { name: "Done with this system" }).click();

  // The inspector of the active system has the odds under "Arrives".
  await system("32").click();
  const arriving = page
    .locator("section[aria-label='Galaxy board'] > div")
    .filter({ has: page.getByRole("button", { name: "Close system details" }) })
    .getByRole("region", { name: "Space combat" });
  await expect(arriving).toHaveAttribute("aria-busy", "false");
  await expect(arriving).toContainText(/Win \d+% · Draw \d+% · Loss \d+%/);
  await expect(arriving.getByLabel(/^Survivors of Sol/)).toHaveText(/^\s*\d\.\d \/ 1\s*$/);
  await fits();
  await page.screenshot({ path: "shots/phone/local/inspector-odds.png", animations: "disabled" });
  const numbers = (await arriving.textContent()) ?? "";

  // The panel has the same odds in the card of the active system.
  await pane("Action").click();
  const odds = page.locator("#step-panel").getByRole("region", { name: "Space combat" });
  await expect(odds).toContainText(numbers);
  await expect(odds.getByLabel("Survivors of Letnev")).toHaveText(
    /^\s*\d\.\d \/ 2\s+\d\.\d \/ 1\s*$/,
  );
  await fits();
  await page.screenshot({ path: "shots/phone/local/movement-odds.png", animations: "disabled" });
});

test("phone: the rolls of the gravity rift and the space cannon offense are in the width", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?scenario=late-game&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const panel = page.locator("#step-panel");
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  // Perimeter (#60) has a PDS of seat C; the fleet in the gravity rift (#41) reaches it.
  await activateInLive(page, system("60"));
  await system("41").click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await fleet.getByRole("button", { name: "Advanced Carrier II 1 of 1" }).click();
  await fleet.getByRole("button", { name: /^Load Spec Ops II, Space area, 2 left/ }).click();
  await fleet
    .getByRole("button", { name: "Load Spec Ops II on Advanced Carrier II 1 of 1" })
    .first()
    .click();
  await fleet.getByRole("button", { name: "Cruiser II 1 of 1" }).click();
  await fleet.getByRole("button", { name: "Done with this system" }).click();
  await send.click();
  await pane("Action").click();
  await expect(panel.getByRole("heading").first()).toHaveText(/^Cancel a hit/);
  await panel.getByRole("button", { name: /^Sustain damage/ }).click();
  await send.click();
  // The landings are on the board: the choice of the player opens the map.
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  await pane("Action").click();
  await expect(panel.getByRole("heading").first()).toHaveText("Invasion");
  await page.getByRole("tab", { name: /^\S* ?Movement/ }).click();
  await expect(panel.getByRole("heading", { name: "Gravity rift" })).toBeVisible();
  await expect(panel.getByRole("region", { name: "Defender" })).toContainText("PDS");
  await fits();
  await page.screenshot({ path: "shots/phone/local/movement-dice.png", animations: "disabled" });
});

test("phone: a space combat is the step of its tactical action, in the width", async ({ page }) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  await page.goto("/?scenario=ongoing_combat&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const panel = page.locator("#step-panel");
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  await page
    .getByRole("navigation", { name: "Panes" })
    .getByRole("button", { name: "Action" })
    .click();
  await expect(panel.getByRole("heading").first()).toHaveText("Space combat");
  const attacker = panel.getByRole("region", { name: "Attacker" });
  // A long printed name takes a second line: it is in view, not only on hover.
  await expect(attacker.getByText("Advanced Carrier I", { exact: true })).toBeVisible();
  await expect(panel.getByRole("region", { name: "Defender" })).toContainText("Cruiser I");
  await expect(panel.getByRole("button", { name: /Stay and fight/ })).toBeVisible();
  await expect(send).toBeVisible();
  await fits();
  await page.screenshot({ path: "shots/phone/local/space-combat.png", animations: "disabled" });
  expect(errors).toEqual([]);
});

test("phone: the landings on a defended planet have the odds of the ground combat", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?scenario=ongoing_invasion_coexistence&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  // Blitz is offered at the start of the invasion: passed.
  await expect(page.locator("#step-panel").getByText("Blitz", { exact: true })).toBeVisible();
  await send.click();
  await pane("Map").click();
  const sheet = page.getByRole("group", { name: /^Land in / });
  await sheet.getByRole("button", { name: /^Land Infantry I on Qucen/ }).click();
  await sheet.getByRole("button", { name: /^Land Infantry I on Rarron/ }).click();
  const odds = sheet.getByRole("region", { name: "Ground combat" });
  await expect(odds).toHaveCount(2);
  await expect(odds.first()).toHaveAttribute("aria-busy", "false");
  await expect(odds.first()).toContainText(/Win \d+%/);
  await fits();
  await page.screenshot({ path: "shots/phone/local/invasion-odds.png", animations: "disabled" });
  // The panel has the first round of each combat, with the odds in the headers.
  await sheet.getByRole("button", { name: "Done with the landings" }).click();
  await pane("Action").click();
  const table = page
    .locator("#step-panel")
    .getByRole("region", { name: "Qucen'n · Ground combat · Round 1" });
  await expect(table.getByRole("region", { name: "Attacker" })).toContainText(/\d+% win/);
  await fits();
  await page.screenshot({ path: "shots/phone/local/invasion-preview.png", animations: "disabled" });
});

test("phone: the landings are staged in a sheet over the map", async ({ page }) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?scenario=late-game&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  await activateInLive(page, system("38"));
  await expect(send).toHaveText(/Move nothing/);
  await system("63").click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await fleet.getByRole("button", { name: "Advanced Carrier II 1 of 1" }).click();
  await fleet.getByRole("button", { name: /^Load Spec Ops II, Planet Vorhal/ }).click();
  await fleet
    .getByRole("button", { name: "Load Spec Ops II on Advanced Carrier II 1 of 1" })
    .first()
    .click();
  await fleet
    .getByRole("button", { name: "Load Spec Ops II on Advanced Carrier II 1 of 1" })
    .first()
    .click();
  await fleet.getByRole("button", { name: "Done with this system" }).click();
  await send.click();

  // The map stays open, and the landings are a sheet over it, with the room of the footer.
  const sheet = page.getByRole("group", { name: /^Land in / });
  await expect(sheet).toBeVisible();
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  await expect(send).toBeHidden();
  await sheet.getByRole("button", { name: "Land Spec Ops II on Abyz" }).click();
  await sheet.getByRole("button", { name: "Land Spec Ops II on Fria" }).click();
  await expect(sheet.getByText("2 units land")).toBeVisible();
  await fits();
  await page.screenshot({ path: "shots/phone/local/invasion.png", animations: "disabled" });
  // The type of a planet opens its exploration deck, as a sheet over the pane.
  await expect(sheet.getByText("Unexplored", { exact: true })).toHaveCount(2);
  await sheet.getByRole("button", { name: /^Hazardous planet Fria/ }).click();
  const decks = page.getByRole("complementary", { name: "Exploration" });
  await expect(decks.getByRole("region", { name: "Hazardous deck" })).toBeInViewport();
  await expect(
    decks.getByRole("region", { name: "Hazardous deck" }).getByText("19 of 20 cards left"),
  ).toBeVisible();
  await fits();
  await page.screenshot({
    path: "shots/phone/local/exploration-deck.png",
    animations: "disabled",
  });
  await page.getByRole("button", { name: "Close Exploration" }).click();
  await expect(sheet).toBeVisible();
  // The check of the sheet closes it: the main button is back, in view.
  await sheet.getByRole("button", { name: "Done with the landings" }).click();
  await expect(send).toHaveText(/Commit ground forces/);
  await expect(send).toBeInViewport({ ratio: 1 });
  // The panel says what lands, and a row of it opens the sheet again, on the map.
  await pane("Action").click();
  await page.screenshot({ path: "shots/phone/local/invasion-panel.png", animations: "disabled" });
  await fits();
  await page
    .locator("#step-panel")
    .getByRole("button", { name: /^Abyz 1 Spec Ops II$/ })
    .click();
  await pane("Map").click();
  await expect(sheet).toBeVisible();
  await sheet.getByRole("button", { name: "Done with the landings" }).click();
  await send.click();
  await expect(send).not.toHaveText(/Commit/);
  await fits();
});

test("phone: a production is a sheet on the map, then the map is the payment", async ({ page }) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?scenario=late-game&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  const row = (name: string | RegExp) => page.locator("#step-panel").getByRole("button", { name });
  await activateInLive(page, system("72"));
  // Nothing moves, and AI Development Algorithm is kept: a question of the list.
  await send.click();
  // The next question of the player is a list: the Action pane opens.
  await expect(pane("Action")).toHaveAttribute("aria-pressed", "true");
  await row(/^Decline/i).click();
  await send.click();
  await pane("Map").click();

  // The map stays open, and the build is a sheet over it, with the room of the footer.
  const sheet = page.getByRole("group", { name: /^Produce in / });
  await expect(sheet).toBeVisible();
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  await expect(send).toBeHidden();
  await sheet.getByRole("button", { name: "Add Fighter II" }).click();
  await sheet.getByRole("button", { name: "Add Cruiser II" }).click();
  await sheet.getByRole("button", { name: "Add Spec Ops II, Planet Velnor" }).click();
  // The printed values are behind the name: a tap opens the card, and nothing is cut at the side.
  const card = page.getByRole("note");
  await sheet.getByRole("button", { name: "War Sun, printed values" }).tap();
  await expect(card).toContainText("BOMBARDMENT 3+ (x3)");
  await expect(card).toBeInViewport({ ratio: 1 });
  await fits();
  await page.screenshot({
    path: "shots/phone/local/production-card.png",
    animations: "disabled",
  });
  await sheet.getByRole("button", { name: "War Sun, printed values" }).tap();
  await expect(card).toHaveCount(0);
  await expect(sheet.getByRole("status").first()).toContainText("5 units · 3 resources");
  await fits();
  await page.screenshot({
    path: "shots/phone/local/production-build.png",
    animations: "disabled",
  });

  // The check of the sheet closes it: the map is the payment, and the main button is in view.
  await sheet.getByRole("button", { name: "Done with the build" }).click();
  await expect(sheet).toBeHidden();
  const bar = page.getByRole("group", { name: "Payment" });
  await expect(bar).toBeVisible();
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  await expect(send).toHaveText(/Produce units/);
  await expect(send).toBeDisabled();
  await expect(send).toBeInViewport({ ratio: 1 });
  await fits();
  await page.screenshot({ path: "shots/phone/local/production-pay.png", animations: "disabled" });
  // A planet pays by a tap, and the trade goods pay the rest.
  await page.locator("g.planet.payable").filter({ hasText: "Lisis" }).click();
  await expect(bar.getByRole("status").first()).toContainText("1 more");
  await bar.getByRole("button", { name: "Pay rest" }).click();
  await expect(bar.getByRole("status").first()).toContainText("✓ Paid");
  await expect(send).toBeEnabled();
  await fits();
  await page.screenshot({
    path: "shots/phone/local/production-paid.png",
    animations: "disabled",
  });
  // The panel says what is produced, and a row of it opens the build again.
  await pane("Action").click();
  await fits();
  await page.screenshot({
    path: "shots/phone/local/production-panel.png",
    animations: "disabled",
  });
  await row(/^Planet Velnor 2 Spec Ops II$/).click();
  await pane("Map").click();
  await expect(sheet).toBeVisible();
  await sheet.getByRole("button", { name: "Done with the build" }).click();
  await send.click();
  await expect(send).not.toHaveText(/Produce/);
  await fits();
});

test("phone: a draft goes on to the landings and the production", async ({ page }) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  await page.goto("/?scenario=late-game&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const row = (name: string | RegExp) => page.locator("#step-panel").getByRole("button", { name });
  const footer = page.locator("footer");
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  const draft = () =>
    page.getByRole("navigation", { name: "Workspace" }).getByRole("button", { name: "Draft" });
  await expect(row(/^Activate a system$/)).toBeVisible();
  await draft().click();
  await pane("Map").click();
  // The systems are the choice once the engine answered the draft.
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);

  // The system of the two space docks. Nothing moves, and AI Development Algorithm is kept.
  await system("72").click();
  await send.click();
  await expect(send).toHaveText(/Move nothing/);
  await send.click();
  await pane("Action").click();
  await row(/^Decline/i).click();
  await expect(send).toHaveText(/Record/);
  await send.click();
  await pane("Map").click();
  // The build of the draft is the sheet over the map, as in Live.
  const sheet = page.getByRole("group", { name: /^Produce in / });
  await expect(sheet).toBeVisible();
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  await sheet.getByRole("button", { name: "Add Cruiser II" }).click();
  await expect(sheet.getByRole("status").first()).toContainText("1 unit · 1 resource");
  await fits();
  await page.screenshot({
    path: "shots/phone/local/draft-production.png",
    animations: "disabled",
  });
  // The check of the sheet goes on to the payment; a planet pays, and the draft is whole.
  await sheet.getByRole("button", { name: "Done with the build" }).click();
  await expect(sheet).toBeHidden();
  await expect(page.getByRole("group", { name: "Payment" })).toBeVisible();
  await page.locator("g.planet.payable").filter({ hasText: "Lisis" }).click();
  await expect(footer.getByText("The action ends here. Every step is drafted.")).toBeVisible();
  await fits();
  await page.screenshot({ path: "shots/phone/local/draft-payment.png", animations: "disabled" });
  const apply = page.getByRole("button", { name: "Apply to Live" });
  await expect(apply).toBeInViewport({ ratio: 1 });
  await apply.click();
  await expect(page.getByRole("dialog").getByText("1 purchase produced and paid")).toBeVisible();
  await fits();
  await page.keyboard.press("Escape");

  // Another action: a carrier lands infantry, and the draft ends where the landing draws a card.
  await page.getByRole("button", { name: "Reference and more" }).click();
  await page.getByRole("menuitem", { name: "Start over" }).click();
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);
  await system("38").click();
  await send.click();
  await expect(send).toHaveText(/Move nothing/);
  await system("63").click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await fleet.getByRole("button", { name: "Advanced Carrier II 1 of 1" }).click();
  await fleet.getByRole("button", { name: /^Load Spec Ops II, Planet Vorhal, 2 left/ }).click();
  await fleet
    .getByRole("button", { name: "Load Spec Ops II on Advanced Carrier II 1 of 1" })
    .first()
    .click();
  await fleet.getByRole("button", { name: /^Done with/ }).click();
  await expect(send).toHaveText(/Go to invasion/);
  await expect(send).toBeInViewport({ ratio: 1 });
  await send.click();
  const landing = page.getByRole("group", { name: /^Land in / });
  await expect(landing).toBeVisible();
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  await landing
    .getByRole("button", { name: /^Land .*Abyz/ })
    .first()
    .click();
  await fits();
  await page.screenshot({ path: "shots/phone/local/draft-invasion.png", animations: "disabled" });
  await landing.getByRole("button", { name: /^Done with/ }).click();
  await expect(footer.getByText("The draft ends at a roll or a draw.")).toBeVisible();
  await expect(apply).toBeEnabled();
  await fits();
  // Applied, the game lands the unit and goes on in Live.
  await apply.click();
  await expect(page.getByRole("dialog").getByText("1 ground force lands")).toBeVisible();
  await page.getByRole("dialog").getByRole("button", { name: "Apply to Live" }).click();
  await expect(apply).toHaveCount(0);
  await fits();
  expect(errors).toEqual([]);
});

test("phone: the sheet of a player has its cards, and nothing is cut at the side", async ({
  page,
}) => {
  await page.goto("/?scenario=late-game&humans=1");
  await page
    .getByRole("navigation", { name: "Panes" })
    .getByRole("button", { name: "Players" })
    .click();
  await page.getByRole("button", { name: /^Seat A, Sol.*Open player sheet/ }).click();
  const sheet = page.getByRole("complementary", { name: "Seat A · Sol" });
  await expect(sheet.getByRole("article", { name: "Evelyn DeLouis" })).toContainText("Exhausted");
  await expect(sheet.getByRole("article", { name: "Jace X, 4th Air Legion" })).toContainText(
    "Unlock: Have 3 scored objectives.",
  );
  await expect(sheet.getByRole("article", { name: "Bio-Stims" })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: "shots/local/phone-player-sheet.png", animations: "disabled" });
});

test("phone: a reaction window is on top of the action of another seat, with its card, its setting and Pass, and nothing is cut at the side", async ({
  page,
}) => {
  // Seat G is Sardakk: it holds Reverse Engineer, and the first seat of the round discards a card.
  await page.goto("/?scenario=late-game&humans=64");
  const panel = page.locator("#step-panel");
  await page
    .getByRole("navigation", { name: "Panes" })
    .getByRole("button", { name: "Action" })
    .click();
  const window = panel.getByRole("group", { name: /^Reaction · After .+ discards .+$/ });
  await expect(window).toBeVisible();
  await expect(window.getByRole("button", { name: "Reverse Engineer" })).toBeVisible();
  await expect(panel.getByRole("combobox", { name: "Offer Reverse Engineer" })).toBeInViewport({
    ratio: 1,
  });
  await expect(page.locator('footer button[aria-keyshortcuts="Enter"]')).toHaveText(/Pass/);
  // Nothing is cut at the side, and the main button is in view.
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  expect(
    await panel
      .locator("> div.overflow-y-auto")
      .evaluate((element) => element.scrollWidth - element.clientWidth),
  ).toBeLessThanOrEqual(0);
  await page.screenshot({ path: "shots/phone/local/reaction-window.png", animations: "disabled" });
  // A tap on the row stages the card: the main button plays it.
  await window.getByRole("button", { name: "Reverse Engineer" }).click();
  await expect(page.locator('footer button[aria-keyshortcuts="Enter"]')).toHaveText(
    /Play Reverse Engineer/,
  );
  for (const button of await page.locator("main > footer button").all()) {
    await expect(button).toBeInViewport({ ratio: 1 });
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
});

test("phone: the technology tree has its columns in the width, and a cell stages a research", async ({
  page,
}) => {
  await page.goto("/?scenario=research_tech_skips&humans=1");
  const panel = page.locator("#step-panel");
  await page
    .getByRole("navigation", { name: "Panes" })
    .getByRole("button", { name: "Action" })
    .click();
  await expect(panel.getByRole("heading", { name: "Research", exact: true })).toBeVisible();
  const cell = panel.getByRole("button", { name: "Sarween Tools", exact: true });
  await cell.click();
  await expect(cell).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator('footer button[aria-keyshortcuts="Enter"]')).toHaveText(/^Send/);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: "shots/phone/local/research-tree.png", animations: "disabled" });
});

test("phone: the command pools of the status phase fit the width, and Send is in view", async ({
  page,
}) => {
  // The load of the scenario, an answer, the editor and a second answer.
  test.setTimeout(10_000);
  // Sol is in the status phase with 2 · 3 · 2 on its sheet, and gains 3 tokens.
  await page.goto("/?scenario=score_objective_status&humans=1");
  const panel = page.locator("#step-panel");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  await pane("Action").click();
  await panel.getByRole("button", { name: "Decline" }).click();
  await send.click();
  await expect(panel.getByRole("heading").first()).toHaveText("Redistribute your command tokens");
  await expect(page.getByText(/into which pool/i)).toHaveCount(0);
  const pools = panel.getByRole("heading", { name: "Command pools" }).locator("..");
  await expect(pools).toContainText("7 / 10 placed");
  for (const pool of ["tactic", "fleet", "strategy"]) {
    await panel.getByRole("button", { name: `Add ${pool} pool token` }).click();
  }
  await expect(pools).toContainText("10 / 10 placed");
  // The three rows are in the width: the counter at the right edge of each is in view.
  for (const pool of ["tactic", "fleet", "strategy"]) {
    await expect(panel.getByRole("button", { name: `Add ${pool} pool token` })).toBeInViewport({
      ratio: 1,
    });
  }
  await expect(send).toBeEnabled();
  await expect(send).toBeInViewport({ ratio: 1 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: "shots/phone/local/command-pools.png", animations: "disabled" });
  await send.click();
  await pane("Players").click();
  await expect(seatRow(page, "Seat A")).toContainText("3 · 4 · 3");
});

test("phone: Leadership has the purchase and the pools in the width, and the payment on the map", async ({
  page,
}) => {
  await page.goto("/?scenario=strategy_leadership&humans=1");
  const panel = page.locator("#step-panel");
  const panes = page.getByRole("navigation", { name: "Panes" });
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: /^1 Leadership/ }).click();
  await page.locator('footer button[aria-keyshortcuts="Enter"]').click();
  await expect(panel.getByRole("heading", { name: "Command pools" })).toBeVisible();
  await panel.getByRole("button", { name: "Add bought token" }).click();
  // The bought token is paid on the map: the choice of the player opens it.
  await expect(panes.getByRole("button", { name: "Map" })).toHaveAttribute("aria-pressed", "true");
  await panes.getByRole("button", { name: "Action" }).click();
  for (const pool of ["tactic", "fleet", "strategy", "strategy"]) {
    await panel.getByRole("button", { name: `Add ${pool} pool token` }).click();
  }
  await expect(page.locator("footer")).toContainText("Pay 3 more influence");
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: "shots/phone/local/leadership.png", animations: "disabled" });
  await panes.getByRole("button", { name: "Map" }).click();
  const bar = page.getByRole("group", { name: "Payment" });
  await bar.getByRole("button", { name: "Auto-pay" }).click();
  await expect(bar.getByRole("status").first()).toContainText("✓ Paid");
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
});

test("phone: Technology has the tree in the width, and the payment of the second research on the map", async ({
  page,
}) => {
  await page.goto("/?scenario=strategy_technology&humans=1");
  const panel = page.locator("#step-panel");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: /^7 Technology/ }).click();
  await send.click();
  await panel.getByRole("button", { name: "Gravity Drive", exact: true }).click();
  await send.click();
  await panel.getByRole("button", { name: "Sarween Tools", exact: true }).click();
  await expect(page.locator("footer")).toContainText("Pay 6 more resources");
  await expect(send).toBeDisabled();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: "shots/phone/local/technology.png", animations: "disabled" });
  await panes.getByRole("button", { name: "Map" }).click();
  const bar = page.getByRole("group", { name: "Payment" });
  await bar.getByRole("button", { name: "Auto-pay" }).click();
  await expect(bar.getByRole("status").first()).toContainText("✓ Paid");
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
  await page.screenshot({ path: "shots/phone/local/technology-pay.png", animations: "disabled" });
  await expect(send).toBeEnabled();
  await send.click();
  await expect(bar).toHaveCount(0);
});

test("phone: the three panes, two fingers zoom the map, and a tap inspects a system", async ({
  page,
}) => {
  await page.goto("/?scenario=strategy_technology&humans=1");
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  for (const name of ["Map", "Players", "Action"]) {
    await pane(name).click();
    await expect(pane(name)).toHaveAttribute("aria-pressed", "true");
  }
  await pane("Map").click();
  const svg = page.locator(".galaxy svg");
  const before = await svg.getAttribute("viewBox");
  const box = (await svg.boundingBox())!;
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
    [box.x + box.width / 2, box.y + box.height / 2],
  );
  const width = (viewBox: string | null) => Number(viewBox!.split(" ")[2]);
  expect(width(await svg.getAttribute("viewBox")) * 2.9).toBeLessThan(width(before));
  await page.screenshot({ path: "shots/phone/local/pinch.png", animations: "disabled" });
  // The system in the middle of the zoomed map: Mecatol Rex.
  await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
  await expect(page.getByRole("button", { name: "Close system details" })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(
    page.viewportSize()!.width,
  );
});

test("phone: the reference sheets and the other app are one menu, and a sheet covers the pane", async ({
  page,
}) => {
  await page.goto("/?scenario=strategy_technology&humans=1");
  await page.getByRole("button", { name: "Reference and more" }).tap();
  await expect(page.getByRole("menuitem", { name: "Start" })).toBeVisible();
  await page.getByRole("menuitem", { name: "Objectives" }).tap();
  await expect(page.getByRole("complementary", { name: "Objectives" })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Panes" })).toBeHidden();
  await page.screenshot({ path: "shots/phone/local/reference.png", animations: "disabled" });
});

test("phone: Diplomacy has the system and the planets in the width, chosen on the map", async ({
  page,
}) => {
  await page.goto("/?scenario=strategy_diplomacy&humans=1");
  const panel = page.locator("#step-panel");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const width = () => page.evaluate(() => document.documentElement.scrollWidth);
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: /^2 Diplomacy/ }).click();
  await page.locator('footer button[aria-keyshortcuts="Enter"]').click();
  // The choice is in the map: the choice of the player opens it.
  await expect(panes.getByRole("button", { name: "Map" })).toHaveAttribute("aria-pressed", "true");
  await panes.getByRole("button", { name: "Action" }).click();
  await expect(panel.getByRole("heading", { name: "Ready planets" })).toBeVisible();
  expect(await width()).toBe(page.viewportSize()!.width);
  await panes.getByRole("button", { name: "Map" }).click();
  await page.locator("g.system.target").filter({ hasText: "#37" }).tap();
  await page.getByRole("button", { name: "Close system details" }).tap();
  await mapPlanet(page, "Jord").tap();
  await mapPlanet(page, "Arinam").tap();
  await expect(page.locator("footer")).toContainText("Ready Jord, Arinam");
  expect(await width()).toBe(page.viewportSize()!.width);
  await panes.getByRole("button", { name: "Action" }).click();
  await expect(panel.getByRole("heading", { name: "Ready planets" }).locator("..")).toContainText(
    "2 / 2",
  );
  expect(await width()).toBe(page.viewportSize()!.width);
  await page.screenshot({ path: "shots/phone/local/diplomacy.png", animations: "disabled" });
});

test("phone: Warfare has the token on the map and the pools in the width", async ({ page }) => {
  await page.goto("/?scenario=strategy_warfare&humans=1");
  const panel = page.locator("#step-panel");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const width = () => page.evaluate(() => document.documentElement.scrollWidth);
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: /^6 Warfare/ }).click();
  await send.click();
  // The choice is in the map: the choice of the player opens it.
  await expect(panes.getByRole("button", { name: "Map" })).toHaveAttribute("aria-pressed", "true");
  await panes.getByRole("button", { name: "Action" }).click();
  await expect(panel.getByRole("heading", { name: "Command pools" })).toBeVisible();
  expect(await width()).toBe(page.viewportSize()!.width);
  await panes.getByRole("button", { name: "Map" }).click();
  await page.locator("g.system.target").filter({ hasText: "#29" }).tap();
  await page.getByRole("button", { name: "Close system details" }).tap();
  expect(await width()).toBe(page.viewportSize()!.width);
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: "Add fleet pool token" }).tap();
  await expect(
    panel.getByRole("heading", { name: "Command token" }).locator("../.."),
  ).toContainText("#29");
  await expect(page.locator("footer")).toContainText("Fleet 3 → 4");
  expect(await width()).toBe(page.viewportSize()!.width);
  await expect(send).toBeInViewport();
  await page.screenshot({ path: "shots/phone/local/warfare.png", animations: "disabled" });
  await send.click();
  await expect(panel.getByRole("heading", { name: "Command pools" })).toHaveCount(0);
});

test("phone: Warfare's secondary is the production of the home system, in the width", async ({
  page,
}) => {
  test.setTimeout(10_000);
  await page.goto("/?scenario=strategy_warfare&humans=3");
  const panel = page.locator("#step-panel");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const width = () => page.evaluate(() => document.documentElement.scrollWidth);
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: /^6 Warfare/ }).click();
  await send.click();
  await panes.getByRole("button", { name: "Map" }).click();
  await page.locator("g.system.target").filter({ hasText: "#29" }).tap();
  await page.getByRole("button", { name: "Close system details" }).tap();
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: "Add fleet pool token" }).tap();
  await send.click();
  await panel.getByRole("button", { name: "Produce" }).tap();
  expect(await width()).toBe(page.viewportSize()!.width);
  await send.click();
  await panes.getByRole("button", { name: "Map" }).click();
  const sheet = page.getByRole("group", { name: /^Produce in / });
  await expect(sheet).toBeVisible();
  await sheet.getByRole("button", { name: "Add Fighter" }).click();
  expect(await width()).toBe(page.viewportSize()!.width);
  await sheet.getByRole("button", { name: "Done with the build" }).click();
  await expect(send).toHaveText(/Produce units/);
  await expect(send).toBeInViewport();
  expect(await width()).toBe(page.viewportSize()!.width);
  await panes.getByRole("button", { name: "Action" }).click();
  await expect(panel.getByRole("heading", { name: "Warfare" })).toBeVisible();
  expect(await width()).toBe(page.viewportSize()!.width);
  await page.screenshot({
    path: "shots/phone/local/warfare-secondary.png",
    animations: "disabled",
  });
});

test("phone: Warfare of Thunder's Edge has the pools and the activation in the width", async ({
  page,
}) => {
  await page.goto("/?scenario=strategy_warfare_te&humans=1");
  const panel = page.locator("#step-panel");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const width = () => page.evaluate(() => document.documentElement.scrollWidth);
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: /^6 Warfare/ }).click();
  await send.click();
  await expect(panel.getByRole("heading", { name: "Command pools" })).toBeVisible();
  expect(await width()).toBe(page.viewportSize()!.width);
  await send.click();
  await expect(panel).toContainText("No command token · Choose on the map");
  await panes.getByRole("button", { name: "Map" }).click();
  await page.locator("g.system.target").filter({ hasText: "#29" }).tap();
  expect(await width()).toBe(page.viewportSize()!.width);
  await expect(send).toHaveText(/Activate system/);
  await expect(send).toBeInViewport();
  await panes.getByRole("button", { name: "Action" }).click();
  expect(await width()).toBe(page.viewportSize()!.width);
  await page.screenshot({ path: "shots/phone/local/warfare-te.png", animations: "disabled" });
});

test("phone: Politics has the speaker in the player table and the agenda deck in the width", async ({
  page,
}) => {
  await page.goto("/?scenario=strategy_politics&humans=1");
  const panel = page.locator("#step-panel");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const width = () => page.evaluate(() => document.documentElement.scrollWidth);
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: /^3 Politics/ }).click();
  await send.click();
  // The choice is in the player table: the choice of the player opens it.
  await expect(panes.getByRole("button", { name: "Players" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await panes.getByRole("button", { name: "Action" }).click();
  await expect(panel.getByRole("heading", { name: "Speaker" })).toBeVisible();
  expect(await width()).toBe(page.viewportSize()!.width);
  await panes.getByRole("button", { name: "Players" }).click();
  await page
    .getByRole("region", { name: "Players" })
    .getByRole("row")
    .filter({
      hasText: "Seat B",
    })
    .tap();
  await panes.getByRole("button", { name: "Action" }).click();
  await expect(panel).toContainText("Speaker: Sol → Hacan");
  await send.click();
  const deck = panel.getByRole("heading", { name: "Agenda deck after" }).locator("../..");
  await expect(deck).toContainText("0 / 2 placed");
  await panel
    .getByRole("button", { name: /Top of the deck/ })
    .first()
    .tap();
  await panel
    .getByRole("button", { name: /Bottom of the deck/ })
    .last()
    .tap();
  await expect(deck).toContainText("2 / 2 placed");
  expect(await width()).toBe(page.viewportSize()!.width);
  await expect(send).toBeInViewport();
  await page.screenshot({ path: "shots/phone/local/politics.png", animations: "disabled" });
});

test("phone: Construction has the structures in the width, the systems chosen on the map", async ({
  page,
}) => {
  await page.goto("/?scenario=strategy_construction&humans=1");
  const panel = page.locator("#step-panel");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const width = () => page.evaluate(() => document.documentElement.scrollWidth);
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: /^4 Construction/ }).click();
  await page.locator('footer button[aria-keyshortcuts="Enter"]').click();
  const pane = (name: string) => panes.getByRole("button", { name });
  const open = (name: string) => panel.getByRole("heading", { name, exact: true }).locator("../..");
  const system = open("System");
  // The system is chosen on the map, which opens; its planets and structures are in the panel,
  // which opens then.
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  await mapPlanet(page, "Rarron").tap();
  await expect(pane("Action")).toHaveAttribute("aria-pressed", "true");
  await expect(panel.getByRole("heading", { name: "Structures" })).toBeVisible();
  expect(await width()).toBe(page.viewportSize()!.width);
  // #29: Qucen'n, then Rarron, each with a space dock and a PDS.
  await open("Structure 1 of 2 · space dock or PDS")
    .getByRole("button", { name: "PDS", exact: true })
    .nth(1)
    .tap();
  // "Change" of the system: the map opens for the second.
  await system.getByRole("button", { name: "Change" }).tap();
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  // The details of #29 are a sheet over the map.
  await page.getByRole("button", { name: "Close system details" }).tap();
  await mapPlanet(page, "Jord").tap();
  await expect(pane("Action")).toHaveAttribute("aria-pressed", "true");
  await open("Structure 2 of 2 · PDS only").getByRole("button", { name: "PDS", exact: true }).tap();
  await expect(page.locator("footer")).toContainText("PDS on Rarron · PDS on Jord");
  expect(await width()).toBe(page.viewportSize()!.width);
  await expect(panel.getByRole("heading", { name: "Structures" }).locator("../..")).toContainText(
    "Jord · #01PDS 0 → 1",
  );
  expect(await width()).toBe(page.viewportSize()!.width);
  await page.screenshot({ path: "shots/phone/local/construction.png", animations: "disabled" });
});

test("phone: Construction of Thunder's Edge has the ability and the structures in the width", async ({
  page,
}) => {
  await page.goto("/?scenario=strategy_construction_te&humans=1");
  const panel = page.locator("#step-panel");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const width = () => page.evaluate(() => document.documentElement.scrollWidth);
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: /^4 Construction/ }).click();
  await page.locator('footer button[aria-keyshortcuts="Enter"]').click();
  expect(await width()).toBe(page.viewportSize()!.width);
  // A structure: the system on the map, which opens, then the structure of a planet in the
  // panel, which opens then.
  await panel.getByRole("button", { name: /Place a structure/ }).tap();
  await expect(panes.getByRole("button", { name: "Map" })).toHaveAttribute("aria-pressed", "true");
  await mapPlanet(page, "Rarron").tap();
  await expect(panes.getByRole("button", { name: "Action" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(panel.getByRole("heading", { name: "Structures" })).toBeVisible();
  const open = (name: string) => panel.getByRole("heading", { name, exact: true }).locator("../..");
  // #29: Qucen'n, then Rarron, each with a space dock and a PDS.
  await open("Structure 1 of 2 · space dock or PDS")
    .getByRole("button", { name: "Space dock", exact: true })
    .nth(1)
    .tap();
  await expect(page.locator("footer")).toContainText("Space dock on Rarron");
  expect(await width()).toBe(page.viewportSize()!.width);
  await expect(open("Structure 2 of 2 · space dock or PDS")).toBeVisible();
  expect(await width()).toBe(page.viewportSize()!.width);
  await page.screenshot({ path: "shots/phone/local/construction-te.png", animations: "disabled" });
});

test("phone: Trade has the other players in the player table, and its screen in the width", async ({
  page,
}) => {
  await page.goto("/?scenario=strategy_trade&humans=1");
  const panel = page.locator("#step-panel");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const width = () => page.evaluate(() => document.documentElement.scrollWidth);
  const row = (name: string) =>
    page.getByRole("region", { name: "Players" }).getByRole("row").filter({ hasText: name });
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: /^5 Trade/ }).click();
  await send.click();
  // The choice is in the player table: the choice of the player opens it.
  await expect(panes.getByRole("button", { name: "Players" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await panes.getByRole("button", { name: "Action" }).click();
  await expect(panel.getByRole("heading", { name: "Other players" })).toBeVisible();
  await expect(panel).toContainText("Trade goods 4 → 7");
  expect(await width()).toBe(page.viewportSize()!.width);
  await panes.getByRole("button", { name: "Players" }).click();
  await expect(row("Seat B")).toContainText("Commodities 0 → 6");
  await row("Seat B").tap();
  await row("Seat C").tap();
  expect(await width()).toBe(page.viewportSize()!.width);
  await panes.getByRole("button", { name: "Action" }).click();
  await expect(
    panel.getByRole("heading", { name: "Other players" }).locator("../.."),
  ).toContainText("2 / 2 chosen");
  expect(await width()).toBe(page.viewportSize()!.width);
  await expect(send).toBeInViewport();
  await page.screenshot({ path: "shots/phone/local/trade.png", animations: "disabled" });
  await send.click();
  await expect(panel.getByRole("heading", { name: "Other players" })).toHaveCount(0);
});

test("phone: Imperial has the objectives and what follows in the width", async ({ page }) => {
  await page.goto("/?scenario=strategy_imperial&humans=1");
  const panel = page.locator("#step-panel");
  const panes = page.getByRole("navigation", { name: "Panes" });
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const width = () => page.evaluate(() => document.documentElement.scrollWidth);
  await panes.getByRole("button", { name: "Action" }).click();
  await panel.getByRole("button", { name: /^8 Imperial/ }).click();
  await send.click();
  await expect(panel.getByRole("heading", { name: "Public objectives" })).toBeVisible();
  await expect(panel).toContainText("Secret objectives 1 → 2 of 3");
  expect(await width()).toBe(page.viewportSize()!.width);
  await panel.getByRole("button", { name: /Negotiate Trade Routes/ }).tap();
  await expect(send).toHaveText(/^Resolve primary↵?$/);
  expect(await width()).toBe(page.viewportSize()!.width);
  await expect(send).toBeInViewport();
  await page.screenshot({ path: "shots/phone/local/imperial.png", animations: "disabled" });
  await send.click();
  await expect(panel.getByRole("heading", { name: "Public objectives" })).toHaveCount(0);
});

test("phone: the Agendas sheet has the laws and the open groups in its width", async ({ page }) => {
  const errors = pageErrors(page);
  await page.goto("/?scenario=late-game&humans=1");
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  await page.getByRole("button", { name: "Reference and more" }).click();
  await page.getByRole("menuitem", { name: "Agendas" }).click();
  const sheet = page.getByRole("complementary", { name: "Agendas" });
  await expect(
    sheet.getByRole("region", { name: "Laws in play" }).getByRole("article"),
  ).toHaveCount(2);
  await fits();
  await page.screenshot({ path: "shots/phone/local/agendas.png", animations: "disabled" });
  // Every group is opened: a closed one is gone from this list when it is clicked.
  const closed = sheet.getByRole("button", { expanded: false });
  for (let left = await closed.count(); left > 0; left--) {
    await closed.first().click();
  }
  await expect(sheet.getByText("Minister of War")).toBeVisible();
  await fits();
  await sheet.getByText("Colonial Redistribution").scrollIntoViewIfNeeded();
  await page.screenshot({ path: "shots/phone/local/agendas-deck.png", animations: "disabled" });
  await sheet.getByRole("button", { name: "Close Agendas" }).click();
  await expect(sheet).toBeHidden();
  expect(errors).toEqual([]);
});

test("phone: the draft of a secondary is the row of the viewer in the list of the seats", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  // The other seats wait for a step, so that the page is shown the game between their decisions.
  await page.addInitScript(() => localStorage.setItem("ti4.local.stepping", "1"));
  // In the game of this seed Politics is played before the seat has its turn.
  await page.goto("/?local=6&players=8&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const panel = page.locator("#step-panel");
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const width = page.viewportSize()!.width;
  await pane("Action").click();
  await panel.getByRole("button", { name: "8. Imperial", exact: true }).click();
  await send.click();
  // Another seat plays Politics after some steps of the seats before it.
  const draft = panel.getByText("✎ Private draft");
  const politics = panel.getByRole("heading", { name: "Politics" });
  const done = async () => (await politics.isVisible()) && (await draft.isVisible());
  const step = page.getByRole("button", { name: /^Step/ });
  const status = page.locator("header strong");
  for (let steps = 0; steps < 200 && !(await done()); steps++) {
    const before = await status.textContent();
    await step.click();
    // The step is taken when the game says something else. A seat that decides twice says the
    // same: then the next step is tried after a moment.
    await expect
      .poll(async () => (await done()) || (await status.textContent()) !== before, {
        timeout: 300,
      })
      .toBe(true)
      .catch(() => {});
  }
  await expect(draft).toBeVisible();
  await expect(panel.getByText("Strategy pool 2 → 1")).toBeVisible();
  await page.screenshot({
    path: "shots/phone/local/secondary-draft.png",
    animations: "disabled",
  });
  // Nothing is cut at the side, and every button of the footer is in view.
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  const content = page.locator("#step-panel > div.overflow-y-auto");
  expect(
    await content.evaluate((element) => element.scrollWidth - element.clientWidth),
  ).toBeLessThanOrEqual(0);
  await panel.getByRole("button", { name: /Decline/ }).click();
  for (const button of await page.locator("main > footer button").all()) {
    await expect(button).toBeInViewport({ ratio: 1 });
  }
  await send.click();
  await expect(send).toHaveText(/^Mark draft ready/);
  await send.click();
  await expect(panel.getByText("✎ Draft ready")).toBeVisible();
  await page.screenshot({
    path: "shots/phone/local/secondary-ready.png",
    animations: "disabled",
  });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
});

test("phone: single player is set up on the panes of the lobby, with the long name", async ({
  page,
}) => {
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
      412,
    );
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const main = page.locator('button[aria-keyshortcuts="Enter"]');
  const long = "Bartholomew Featherstonehaugh the Third of Mecatol";
  await page.addInitScript((name) => localStorage.setItem("ti4.nickname", name), long);
  await page.goto("/?single&seed=3&players=8");

  // The Players pane is first: eight seats, nothing is cut at the side, the main button in reach.
  const rows = page.getByRole("region", { name: "Players" }).locator("ul.overflow-y-auto > li");
  await expect(rows).toHaveCount(8);
  await expect(rows.nth(0)).toContainText(`${long} (you)`);
  await expect(main).toHaveText(/^Start game/);
  await expect(main).toBeInViewport();
  await fits();
  await page.screenshot({ path: "shots/phone/local/single-players.png", animations: "disabled" });

  // An open seat has the three models of the server after "Random": its row wraps them, and
  // nothing is cut at the side.
  await rows.nth(7).getByRole("button", { name: "Remove Rule bot 7" }).tap();
  const kinds = rows.nth(7).getByRole("button");
  await expect(kinds).toHaveText([
    "Rule bot",
    "Random",
    "Synthetic model 1",
    "Synthetic model 2",
    "Synthetic model 3",
  ]);
  await fits();
  for (const kind of await kinds.all()) {
    const box = (await kind.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(412);
  }
  await expect(main).toBeInViewport();
  await page.screenshot({ path: "shots/phone/local/single-models.png", animations: "disabled" });
  await kinds.filter({ hasText: "Synthetic model 3" }).tap();
  await expect(rows.nth(7)).toContainText("Synthetic model 3 1");
  await expect(main).toBeEnabled();

  await pane("Setup").tap();
  const setup = page.getByRole("region", { name: "Setup" });
  await expect(setup.getByLabel("Players", { exact: true })).toHaveText("8");
  await setup.getByRole("button", { name: "One player fewer" }).tap();
  await expect(setup.getByLabel("Players", { exact: true })).toHaveText("7");
  await expect(main).toBeInViewport();
  await fits();
  await page.screenshot({ path: "shots/phone/local/single-setup.png", animations: "disabled" });

  await pane("Map").tap();
  await expect(page.locator("g.system.home")).toHaveCount(7);
  await expect(main).toBeInViewport();
  await fits();

  // The game of the table has the panes of a game.
  await main.tap();
  await expect(pane("Action")).toBeVisible();
  await expect(page.locator('footer button[aria-keyshortcuts="Enter"]')).toBeVisible();
  await fits();
});

test("phone: the turn menu and its lists fit the width; a choice of a player opens its pane", async ({
  page,
}) => {
  await page.goto("/?scenario=late-game&humans=16");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const row = (name: string | RegExp) => page.locator("#step-panel").getByRole("button", { name });
  const heading = page.locator("#step-panel").getByRole("heading").first();
  const width = page.viewportSize()!.width;
  // Nothing is cut at the side: not the page, and not the open step.
  const fits = async () => {
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
    const content = page.locator("#step-panel > div.overflow-y-auto");
    expect(
      await content.evaluate((element) => element.scrollWidth - element.clientWidth),
    ).toBeLessThanOrEqual(0);
  };
  await expect(pane("Action")).toHaveAttribute("aria-pressed", "true");
  await expect(row(/^Activate a system$/)).toBeVisible();
  await fits();
  await page.screenshot({ path: "shots/phone/local/picker.png", animations: "disabled" });
  // A phone has no keys: a row is tapped, and "Back to actions" goes back.
  await row(/^Play an action card or use a component$/).tap();
  await expect(heading).toHaveText("Play an action card or use a component");
  await fits();
  await page.screenshot({ path: "shots/phone/local/picker-cards.png", animations: "disabled" });
  await page.getByRole("button", { name: "Back to actions" }).tap();
  await row(/^5 Trade$/).tap();
  await expect(send).toHaveText(/Use Trade/);
  await fits();
  await send.tap();
  // The choice of a player is made in the player table: the choice of the player opens it.
  await expect(pane("Players")).toHaveAttribute("aria-pressed", "true");
  await expect(pane("Players")).not.toHaveAccessibleName(/choice open/);
  await page
    .getByRole("region", { name: "Players" })
    .getByRole("button", { name: /^Choose Seat B/ })
    .tap();
  await expect(send).toBeEnabled();
  await expect(send).toBeInViewport({ ratio: 1 });
  await fits();
  await page.screenshot({ path: "shots/phone/local/pick-player.png", animations: "disabled" });
});

test("phone: a component action chooses its planet on the Map pane, then the turn ends", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?scenario=late-game&humans=1");
  const send = page.locator('footer button[aria-keyshortcuts="Enter"]');
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const row = (name: string | RegExp) => page.locator("#step-panel").getByRole("button", { name });
  const heading = page.locator("#step-panel").getByRole("heading").first();
  const width = page.viewportSize()!.width;
  const fits = async () => {
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
    const content = page.locator("#step-panel > div.overflow-y-auto");
    expect(
      await content.evaluate((element) => element.scrollWidth - element.clientWidth),
    ).toBeLessThanOrEqual(0);
  };
  await row(/^Play an action card or use a component$/).tap();
  await row(/^Orbital Drop$/).tap();
  await expect(heading).toHaveText("Orbital Drop");
  await fits();
  await send.tap();
  // The planet is chosen on the map: the map opens.
  await expect(pane("Map")).toHaveAttribute("aria-pressed", "true");
  await fits();
  await page.screenshot({ path: "shots/phone/local/component.png", animations: "disabled" });
  await page.getByRole("button", { name: /^Choose Jord,/ }).tap();
  await expect(send).toBeEnabled();
  await expect(send).toBeInViewport({ ratio: 1 });
  await fits();
  await page.screenshot({ path: "shots/phone/local/component-planet.png", animations: "disabled" });
  await send.tap();
  // The turn closes in the list: the Action pane opens.
  await expect(pane("Action")).toHaveAttribute("aria-pressed", "true");
  await expect(heading).toHaveText("End your turn");
  await expect(send).toHaveText(/End turn/);
  await fits();
  await page.screenshot({ path: "shots/phone/local/turn-closing.png", animations: "disabled" });
});

test("phone: the movement says what no ship can load, a wormhole and a damaged ship", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  await page.goto("/?scenario=late-game&humans=1");
  const pane = (name: string) =>
    page.getByRole("navigation", { name: "Panes" }).getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const width = page.viewportSize()!.width;
  const fits = async () =>
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
  await activateInLive(page, system("42"));
  await pane("Action").click();
  await expect(
    page.locator("#step-panel").getByText(/^⚠ Stay: 2 Spec Ops II \(Planet Vega Major\)/),
  ).toBeVisible();
  await fits();
  await page.screenshot({ path: "shots/phone/local/movement-cases.png", animations: "disabled" });

  await pane("Map").click();
  await system("25").click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await fleet.getByRole("button", { name: "Cruiser II 1 of 1" }).tap();
  await expect(fleet.getByText(/β wormhole/).first()).toBeVisible();
  await fits();
  await fleet.getByRole("button", { name: "Done with this system" }).tap();
  await system("63").click();
  await expect(
    fleet.getByRole("button", { name: "Dreadnought II (damaged) 1 of 1", exact: true }),
  ).toBeVisible();
  await fits();
  expect(errors).toEqual([]);
});
