import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { activateInLive, pageErrors, seatRow, mapPlanet } from "./actions";

// A real game of the wasm engine in the shell: `?local=<seed>&players=8&humans=<mask>`.
// Needs the engine: `scripts/build-wasm.sh`. Screenshots go to shots/local/.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");
test.use({ viewport: { width: 1920, height: 1080 } });
// The engine plays the other seats between two choices, which takes a moment.

const panel = (page: Page) => page.locator("#step-panel");
/** The main button of the footer: "Send", "Activate system", "Move fleet". Enter does what it says. */
const send = (page: Page) => page.locator('footer button[aria-keyshortcuts="Enter"]');
const heading = (page: Page) => panel(page).getByRole("heading").first();
const settings = (page: Page) => page.getByRole("complementary", { name: "Dev" });
const undo = (page: Page) => page.getByRole("button", { name: "Undo" });
/** A question of the game is open: its main button, or the picker of the turn menu, which has none. */
const questionOpen = (page: Page) =>
  expect(
    send(page)
      .or(heading(page).filter({ hasText: "Choose an action" }))
      .first(),
  ).toBeVisible();

/** At the turn menu, the row of a strategy card ("2 Diplomacy") stages its action, and Enter sends it. */
async function playStrategyCard(page: Page, row: string) {
  await expect(heading(page)).toHaveText("Choose an action");
  await panel(page)
    .getByRole("button", { name: new RegExp(`^${row}`) })
    .click();
  await page.keyboard.press("Enter");
}

/** Opens the Dev sheet from the header. */
async function openSettings(page: Page) {
  await page.getByRole("button", { name: "Dev", exact: true }).click();
  await expect(settings(page)).toBeVisible();
}

/**
 * The line of the Dev sheet ("Local game · seed 42 · 8 seats · 4 answers saved · 31 decisions of
 * all seats"). Read, then closed.
 */
async function savedLine(page: Page, start = /^Local game · /) {
  await openSettings(page);
  const line = (await settings(page).getByText(start).textContent()) ?? "";
  await page.keyboard.press("Escape");
  await expect(settings(page)).toBeHidden();
  return line;
}

/** A saved game as the save store keeps it (IndexedDB `ti4-offline`, object store `games`). */
interface StoredGame {
  id: string;
  slot?: string;
  schema: number;
  engine: { wasmSha256: string };
  summary: { answers: number };
  save: {
    decisions: string[];
    answers: string[];
    game: { table?: { setup: object; seats: { kind: string }[] } };
    [field: string]: unknown;
  };
}

/** The page made the database of the save store (a read before would make another one). */
const storeReady = (page: Page) =>
  expect
    .poll(() =>
      page.evaluate(
        async () => (await indexedDB.databases()).find((db) => db.name === "ti4-offline")?.version,
      ),
    )
    .toBe(2);

/** The saved games of the browser of the page. */
const savedGames = async (page: Page) => {
  await storeReady(page);
  return page.evaluate(
    () =>
      new Promise<StoredGame[]>((resolve, reject) => {
        const open = indexedDB.open("ti4-offline");
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const db = open.result;
          const all = db.transaction("games").objectStore("games").getAll();
          all.onerror = () => reject(all.error);
          all.onsuccess = () => {
            db.close();
            resolve(all.result);
          };
        };
      }),
  );
};

/** The saved game of a slot, or undefined. */
const savedGame = async (page: Page, slot: string) =>
  (await savedGames(page)).find((game) => game.slot === slot);

/** The saved game of a slot, once it is written. */
async function written(page: Page, slot: string, answers = 0) {
  await expect
    .poll(async () => (await savedGame(page, slot))?.summary.answers ?? -1)
    .toBeGreaterThanOrEqual(answers);
  return (await savedGame(page, slot))!;
}

/** Writes a record of the save store as it is, as an older or another page may have left it. */
const putRecord = async (page: Page, record: object) => {
  await storeReady(page);
  await page.evaluate(
    (row) =>
      new Promise<void>((resolve, reject) => {
        const open = indexedDB.open("ti4-offline");
        open.onerror = () => reject(open.error);
        open.onsuccess = () => {
          const db = open.result;
          const tx = db.transaction("games", "readwrite");
          tx.objectStore("games").put(row);
          tx.oncomplete = () => {
            db.close();
            resolve();
          };
          tx.onerror = () => reject(tx.error);
        };
      }),
    record,
  );
};

/** The open step does not need vertical scroll at 1920×1080 (AGENTS.md, Text). */
async function fitsHeight(page: Page) {
  const overflow = await page
    .locator("#step-panel > div.overflow-y-auto")
    .evaluate((element) => element.scrollHeight - element.clientHeight);
  expect(overflow, "the step content needs vertical scroll").toBeLessThanOrEqual(0);
}

/** The button that places a gained command token, while the pools of Leadership wait for one. */
const tokensToPlace = (page: Page) =>
  panel(page).getByRole("button", { name: "Add tactic pool token" }).and(page.locator(":enabled"));

/**
 * Answers the open choice: with the first system when it is on the board, with the gained tokens
 * in the tactic pool for the pools of Leadership, with nothing when the main button already has
 * something to send (a movement of no ship), or else with its first row.
 */
async function answer(page: Page) {
  const saved = await savedLine(page);
  const system = page.locator("g.system.target");
  if ((await heading(page).textContent()) === "Choose an action") {
    // The turn menu: the strategy card, then pass. A tactical action would open the draft.
    await panel(page)
      .getByRole("button", { name: /^(\d \S.*|Take no more actions in this round)$/ })
      .and(page.locator(":enabled"))
      .first()
      .click();
  } else if ((await heading(page).textContent()) === "End your turn") {
    await send(page).click();
    await expect.poll(() => savedLine(page)).not.toBe(saved);
    return;
  } else if (await system.count()) {
    await system.first().click();
  } else if (await tokensToPlace(page).count()) {
    // The command pools of Leadership: each gained token goes into the tactic pool.
    while (await send(page).isDisabled()) {
      await tokensToPlace(page).click();
    }
  } else if (await send(page).isDisabled()) {
    // A row of the choice: a button that can be pressed (not the rules behind the heading).
    await panel(page).locator('button[aria-pressed="false"]:enabled').first().click();
  }
  await expect(send(page)).toBeEnabled();
  await send(page).click();
  // The answer is saved, and the next choice is open.
  await expect.poll(() => savedLine(page)).not.toBe(saved);
  await questionOpen(page);
}

test("a game of eight is played through the decision list @slow", async ({ page }) => {
  const errors = pageErrors(page, { console: true });
  await page.goto("/?local=3&players=8&humans=1");
  await expect(send(page)).toBeDisabled();
  await expect(page.locator("g.system")).toHaveCount(55);
  await expect(page.getByRole("region", { name: "Players" }).locator("tbody tr")).toHaveCount(8);
  await expect(page.getByText("Round 1 · Strategy phase")).toBeVisible();
  await expect(heading(page)).toHaveText("Choose a strategy card");
  await page.screenshot({ path: "shots/local/first-choice.png", animations: "disabled" });

  // Enter does what the main button says, and a number key stages a row.
  await page.keyboard.press("2");
  await expect(panel(page).getByRole("button", { pressed: true })).toHaveCount(1);
  await expect(page.getByText("Not sent")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(panel(page).getByRole("button", { pressed: true })).toHaveCount(0);
  await page.keyboard.press("2");
  await page.keyboard.press("Enter");
  await expect.poll(() => savedLine(page)).toContain("1 answers saved");
  await questionOpen(page);

  for (let count = 1; count < 30; count++) {
    await answer(page);
  }
  await expect(page.getByText("Round 1 · Strategy phase")).toHaveCount(0);
  await page.screenshot({ path: "shots/local/after-30-choices.png", animations: "disabled" });

  // The Cards sheet: the hand against the limit, and the deck. A card that is not held is faked.
  await page.getByRole("button", { name: "Cards", exact: true }).click();
  const cards = page.getByRole("complementary", { name: "Cards" });
  await expect(cards.getByText(/^Hand \d+ \/ 7$/)).toBeVisible();
  await expect(cards.getByText(/of 122 cards left/)).toBeVisible();
  // The deck is in groups by when a card is played. A card with "ACTION:" has nothing to fake.
  const action = cards.getByRole("region", { name: "Action", exact: true });
  // A group is closed until the player opens it.
  await expect(action.getByText(/unseen$/)).toHaveCount(0);
  await action.getByRole("button", { name: /^Action/ }).click();
  await expect(action.getByText(/unseen$/).first()).toBeVisible();
  await expect(action.getByRole("button")).toHaveCount(1);
  await cards.getByRole("region", { name: "Agenda phase" }).getByRole("button").click();
  await cards
    .getByRole("button", { name: /^Fake / })
    .first()
    .click();
  await expect(cards.getByRole("button", { name: "Stop faking" })).toBeVisible();
  await page.screenshot({ path: "shots/local/cards.png", animations: "disabled" });
  await page.keyboard.press("Escape");
  await expect(cards).toBeHidden();

  // The Secrets sheet: the objectives of the viewer, and the deck in groups by what is asked.
  await page.getByRole("button", { name: "Secrets", exact: true }).click();
  const secrets = page.getByRole("complementary", { name: "Secrets" });
  const mine = secrets.getByRole("region", { name: "Your secret objectives" });
  await expect(mine.getByText(/^\d scored · \d \/ 3$/)).toBeVisible();
  await expect(mine.getByRole("article").first()).toContainText("Not scored");
  await expect(secrets.getByText(/^\d+ of 40 unseen · \d+ in the deck$/)).toBeVisible();
  const ships = secrets.getByRole("region", { name: "Ships on the board" });
  // A group is closed until the player opens it.
  await expect(ships.getByText("Unseen", { exact: true })).toHaveCount(0);
  await ships.getByRole("button").click();
  // Each objective says where it is: unseen, in the hand of the viewer, or scored by a seat.
  await expect(ships.getByText(/^(Unseen|In your hand|✓ Scored by.*)$/)).toHaveCount(6);
  await expect(ships.getByText("Unseen", { exact: true }).first()).toBeVisible();
  await page.screenshot({ path: "shots/local/secrets.png", animations: "disabled" });
  await page.keyboard.press("Escape");
  await expect(secrets).toBeHidden();
  expect(errors).toEqual([]);
});

test("a tactical action: the system and the movement are staged on the map", async ({ page }) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  await page.goto("/?local=3&players=8&humans=1");
  await expect(send(page)).toBeDisabled();
  const row = (name: string | RegExp) => panel(page).getByRole("button", { name });
  await row("8. Imperial").click();
  await send(page).click();
  // The picker opens the tactical action as the private draft, at its activation.
  await row(/^Activate a system$/).click();

  // Activation: the board is the picker, and the panel has the choice, not a list of systems.
  await expect(heading(page)).toHaveText("Activation");
  await expect(page.getByText("Private draft").first()).toBeVisible();
  await expect(send(page)).toHaveText(/Activate system/);
  await expect(send(page)).toBeDisabled();
  await expect(panel(page).getByRole("button", { name: /^Activate \d/ })).toHaveCount(0);
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  // Every system can be activated, but a hyperlane and the system with a command token of the
  // seat (Diplomacy of Jol-Nar put one at 12). Only those where the action does something stand
  // out: the systems that the ships reach, and home with its space dock.
  await expect(page.locator("g.system.target")).toHaveCount(48);
  await expect(system("12")).not.toHaveClass(/target/);
  const standOut = await page.locator("g.system.target:not(.plain)").count();
  expect(standOut).toBeGreaterThan(1);
  expect(standOut).toBeLessThan(48);
  await expect(system("01")).not.toHaveClass(/plain/);
  await page.screenshot({ path: "shots/local/activation-open.png", animations: "disabled" });
  // A click chooses the system and shows what is in it.
  const details = page.getByRole("button", { name: "Close system details" });
  await system("01").click();
  await expect(panel(page).getByText("Production 6")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Jord - Sol · 01" })).toBeVisible();
  await expect(page.getByText("2 Advanced Carrier I")).toBeVisible();
  await page.screenshot({ path: "shots/local/activation-details.png", animations: "disabled" });
  // The details cover the corner of the board where 23 is.
  await details.click();
  await system("23").click();
  await expect(page.locator("g.system.chosen")).toHaveCount(1);
  await expect(page.getByRole("heading", { name: /· 23$/ })).toBeVisible();
  await expect(panel(page).getByText("3 ships in range")).toBeVisible();
  await expect(panel(page).getByText("From 1 system")).toBeVisible();
  await page.screenshot({ path: "shots/local/activation.png", animations: "disabled" });
  // Closing the details does not take the choice back.
  await details.click();
  await expect(details).toBeHidden();
  await expect(page.locator("g.system.chosen")).toHaveCount(1);
  await expect(send(page)).toBeEnabled();
  await page.keyboard.press("Enter");
  // The draft records the system; "Apply to Live" sends the action and the system.
  await expect(heading(page)).toHaveText("Movement");
  await page.getByRole("button", { name: "Apply to Live" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Apply to Live" }).click();

  // Movement: nothing is staged, and moving nothing is what the main button says.
  await expect(page.getByText("Private draft")).toHaveCount(0);
  await expect(heading(page)).toHaveText("Movement");
  await expect(send(page)).toHaveText(/Move nothing/);
  await expect.poll(() => savedLine(page)).toContain("3 answers saved");
  // The system with the ships opens its fleet on the map.
  await system("01").click();
  const sheet = page.getByRole("group", { name: /^Move from Jord/ });
  await expect(sheet).toBeVisible();
  // The fleet sheet has what is in the system, as the inspector has it.
  await expect(sheet.getByText("4 / 2")).toBeVisible();
  await sheet.getByRole("button", { name: "Advanced Carrier I 1 of 2" }).click();
  await sheet.getByRole("button", { name: /^Load Spec Ops I, Planet Jord, 5 left/ }).click();
  await sheet
    .getByRole("button", { name: "Load Spec Ops I on Advanced Carrier I 1 of 2" })
    .first()
    .click();
  await sheet
    .getByRole("button", { name: "Load Spec Ops I on Advanced Carrier I 1 of 2" })
    .first()
    .click();
  await sheet.getByRole("button", { name: "Destroyer I 1 of 1" }).click();
  await expect(sheet.getByText("2 ships · 2 cargo")).toBeVisible();
  await expect(
    sheet.getByRole("button", { name: /^Load Spec Ops I, Planet Jord, 3 left/ }),
  ).toBeVisible();
  await expect(panel(page).getByText("Fleet supply")).toBeVisible();
  await expect(send(page)).toHaveText(/Move fleet/);
  // The board shows what the movement leaves and what arrives.
  await expect(system("01")).toHaveAttribute("aria-label", /handled, 4 Sol ships/);
  await expect(system("23")).toHaveAttribute("aria-label", /2 Sol ships/);
  await page.screenshot({ path: "shots/local/movement.png", animations: "disabled" });
  // Nothing was sent yet.
  await expect.poll(() => savedLine(page)).toContain("3 answers saved");
  // A reload keeps what is staged, and with it the mark of the system.
  await page.reload();
  await expect(send(page)).toHaveText(/Move fleet/);
  await expect(system("01")).toHaveAttribute("aria-label", /handled, 4 Sol ships/);
  await system("01").click();
  await expect(sheet.getByText("2 ships · 2 cargo")).toBeVisible();

  // "Move fleet" sends the whole movement: each ship and each unit is one answer of the game.
  await send(page).click();
  await expect.poll(() => savedLine(page)).toContain("9 answers saved");
  await expect(heading(page)).not.toHaveText("Movement");
  await expect(system("23")).toHaveAttribute("aria-label", /2 Sol ships/);
  await expect(system("01")).toHaveAttribute("aria-label", /4 Sol ships/);
  await page.screenshot({ path: "shots/local/after-movement.png", animations: "disabled" });

  // One undo takes the movement back whole, and it is staged again as it was sent.
  await undo(page).click();
  await expect(heading(page)).toHaveText("Movement");
  await expect.poll(() => savedLine(page)).toContain("3 answers saved");
  await expect(send(page)).toHaveText(/Move fleet/);
  await expect(system("01")).toHaveAttribute("aria-label", /handled, 4 Sol ships/);
  // The sheet of the system is still open, where the player left it.
  await expect(sheet.getByText("2 ships · 2 cargo")).toBeVisible();
  // So one part of it can change: the destroyer stays, the carrier moves with its hold.
  await sheet.getByRole("button", { name: "Destroyer I 1 of 1" }).click();
  await expect(sheet.getByText("1 ship · 2 cargo")).toBeVisible();
  await page.screenshot({ path: "shots/local/movement-undone.png", animations: "disabled" });
  await send(page).click();
  await expect.poll(() => savedLine(page)).toContain("8 answers saved");
  await expect(system("23")).toHaveAttribute("aria-label", /1 Sol ship/);
  // One undo again, and one more opens the activation again.
  await undo(page).click();
  await expect(heading(page)).toHaveText("Movement");
  await expect.poll(() => savedLine(page)).toContain("3 answers saved");
  await undo(page).click();
  await expect(heading(page)).toHaveText("Activation");
  await expect.poll(() => savedLine(page)).toContain("2 answers saved");
  expect(errors).toEqual([]);
});

test("hotseat: the viewer is the seat that is asked", async ({ page }) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?local=3&players=8&humans=255");
  await expect(send(page)).toBeDisabled();
  const me = page
    .getByRole("region", { name: "Players" })
    .locator('tbody tr:has([title="Active player"]) th');
  const seen = new Set<string>();
  for (let count = 0; count < 16; count++) {
    seen.add((await me.first().textContent()) ?? "");
    await answer(page);
  }
  // The strategy phase asks every seat once.
  expect(seen.size).toBe(8);
});

test("an inspected system shows its forces", async ({ page }) => {
  await page.goto("/?local=3&players=8&humans=1");
  await expect(send(page)).toBeDisabled();
  await page.getByRole("button", { name: /^Inspect .*system 01/ }).click();
  await expect(page.getByRole("heading", { name: "Jord - Sol · 01" })).toBeVisible();
  await expect(page.getByText("2 Advanced Carrier I")).toBeVisible();
  await page.screenshot({ path: "shots/local/inspector.png", animations: "disabled" });
});

test("a movement into a fleet of another seat has the odds of the space combat", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  await page.goto("/?scenario=late-game&humans=1");
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  // Seat C has two cruisers and a destroyer in 32, and a PDS on its planet.
  await activateInLive(page, system("32"));
  await expect(heading(page)).toHaveText("Movement");
  const odds = panel(page).getByRole("region", { name: "Space combat" });
  await expect(odds).toHaveCount(0);
  await system("24").click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await fleet.getByRole("button", { name: "Dreadnought II 1 of 1" }).click();
  await fleet.getByRole("button", { name: "Done with this system" }).click();

  // The panel has Win, Draw and Loss, and the survivors of each side against the ships that fight.
  await expect(odds).toHaveAttribute("aria-busy", "false");
  await expect(odds).toContainText(/Space combat ·\s+Letnev/);
  await expect(odds).toContainText(/Win \d+% · Draw \d+% · Loss \d+%/);
  await expect(odds.getByLabel(/^Survivors of Sol/)).toHaveText(/^\s*\d\.\d \/ 1\s*$/);
  await expect(odds.getByLabel("Survivors of Letnev")).toHaveText(
    /^\s*\d\.\d \/ 2\s+\d\.\d \/ 1\s*$/,
  );
  await expect(odds.getByTitle("Cruiser I", { exact: true })).toBeVisible();
  await odds.getByRole("button", { name: "About the odds" }).click();
  await expect(page.getByRole("note")).toContainText("2000 rollouts");
  await page.keyboard.press("Escape");
  // The open step does not scroll at 1920×1080.
  const overflow = await page
    .locator("#step-panel > div.overflow-y-auto")
    .evaluate((element) => element.scrollHeight - element.clientHeight);
  expect(overflow, "the step content needs vertical scroll").toBeLessThanOrEqual(0);
  await page.screenshot({ path: "shots/local/movement-odds.png", animations: "disabled" });

  // The inspector of the active system has the same odds under "Arrives".
  await system("32").click();
  const inspector = page
    .locator("section[aria-label='Galaxy board'] > div")
    .filter({ has: page.getByRole("button", { name: "Close system details" }) });
  const arriving = inspector.getByRole("region", { name: "Space combat" });
  await expect(arriving).toContainText((await odds.textContent()) ?? "");
  await page.screenshot({ path: "shots/local/inspector-odds.png", animations: "disabled" });
  expect(errors).toEqual([]);
});

test("the movement says what no ship can load, a path through a wormhole, and a damaged ship", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  await page.goto("/?scenario=late-game&humans=1");
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  await activateInLive(page, system("42"));
  await expect(heading(page)).toHaveText("Movement");
  // No gun of any seat reaches #42: the facts prove that the space cannon offense is skipped.
  await expect(panel(page).getByRole("group", { name: "Substeps" })).toContainText(
    "Space cannon offense · no guns",
  );

  // The fighters of the depot move on their own, and no ship with capacity passes there: its
  // ground forces stay, and the movement is allowed.
  const depot = panel(page).getByRole("button", { name: /^Vega Major\/Vega Minor · #74/ });
  await expect(depot).toContainText(
    "⚠ Stay: 2 Spec Ops II (Planet Vega Major), 1 ZS Thunderbolt M2 (Planet Vega Minor) · No ship with capacity starts or passes here",
  );
  await expect(send(page)).toBeEnabled();

  // The far fleet comes through the β wormhole.
  await system("25").click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await fleet.getByRole("button", { name: "Cruiser II 1 of 1" }).click();
  await expect(fleet.getByText(/β wormhole/).first()).toBeVisible();
  await fleet.getByRole("button", { name: "Done with this system" }).click();

  // The dreadnought of the north took a hit.
  await system("63").click();
  const north = page.getByRole("group", { name: /^Move from / });
  await north.getByRole("button", { name: "Dreadnought II (damaged) 1 of 1", exact: true }).click();
  await north.getByRole("button", { name: "Done with this system" }).click();
  // The open step does not scroll at 1920×1080.
  const overflow = await page
    .locator("#step-panel > div.overflow-y-auto")
    .evaluate((element) => element.scrollHeight - element.clientHeight);
  expect(overflow, "the step content needs vertical scroll").toBeLessThanOrEqual(0);
  await page.screenshot({ path: "shots/local/movement-cases.png", animations: "disabled" });
  expect(errors).toEqual([]);
});

test("a draft stops at the rift roll; Live keeps what the rift and the space cannon offense rolled", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  await page.goto("/?scenario=late-game&humans=1");
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const tab = (name: string) => page.getByRole("tab", { name: new RegExp(`^\\S* ?${name}`) });
  // Perimeter (#60) has a PDS of seat C and no ships; the fleet in the gravity rift (#41) reaches it.
  await panel(page).getByRole("button", { name: "Activate a system", exact: true }).click();
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);
  await system("60").click();
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Movement");
  await system("41").click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await fleet.getByRole("button", { name: "Advanced Carrier II 1 of 1" }).click();
  await fleet.getByRole("button", { name: /^Load Spec Ops II, Space area, 2 left/ }).click();
  await fleet
    .getByRole("button", { name: "Load Spec Ops II on Advanced Carrier II 1 of 1" })
    .first()
    .click();
  await fleet
    .getByRole("button", { name: "Load Spec Ops II on Advanced Carrier II 1 of 1" })
    .first()
    .click();
  await fleet.getByRole("button", { name: "Cruiser II 1 of 1" }).click();
  await fleet.getByRole("button", { name: "Done with this system" }).click();
  // The draft holds no dice: its check ends at the first rift roll.
  await expect(panel(page).getByText("Preview ends here")).toBeVisible();
  await expect(page.locator("footer")).toContainText("The draft ends at a roll");
  await expect(panel(page).getByText("2 rift rolls")).toBeVisible();
  // The heading has the parts of the movement; the guns that will fire are in the panel.
  await expect(panel(page).getByRole("group", { name: "Substeps" })).toHaveText(
    /Move ships\s*Gravity rift\s*Space cannon offense/,
  );
  const guns = panel(page).getByRole("region", { name: "Defender" });
  await expect(guns).toContainText(/Seat C\s*Letnev/);
  await expect(guns).toContainText("PDS");
  await expect(guns).toContainText("6+");
  await expect(panel(page).getByRole("region", { name: "Attacker" })).toContainText(
    "Advanced Carrier II",
  );
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/draft-rift.png", animations: "disabled" });

  // Apply sends the carrier, which rolls; the cruiser is staged again in Live.
  await page.getByRole("button", { name: "Apply to Live" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Apply to Live" }).click();
  await expect(page.locator("footer")).toContainText("Staged from your draft");
  // The rolls are the card under the heading of the section.
  const rift = panel(page)
    .getByRole("heading", { name: "Gravity rift" })
    .locator("xpath=following-sibling::*[1]");
  await expect(rift.getByText("Gravity Rift · #41")).toHaveCount(1);
  await expect(rift).toContainText("Advanced Carrier II");
  await expect(rift).toContainText("carries 2 Spec Ops II · survives on 4+");
  await expect(panel(page).getByText("1 rift roll")).toBeVisible();
  await send(page).click();

  // The PDS of seat C fires at the fleet and hits: the carrier sustains the damage.
  await expect(heading(page)).toHaveText(/^Cancel a hit/);
  await panel(page)
    .getByRole("button", { name: /^Sustain damage/ })
    .click();
  await send(page).click();
  await expect(heading(page)).toHaveText("Invasion");

  // The movement of the action is read while the landings are open: both rift rolls and the guns.
  await tab("Movement").click();
  await expect(heading(page)).toHaveText("Movement");
  await expect(panel(page).getByText("Resolved")).toBeVisible();
  await expect(rift.getByText("Gravity Rift · #41")).toHaveCount(2);
  await expect(rift).toContainText("Cruiser II");
  const cannon = panel(page).getByRole("region", { name: "Defender" });
  await expect(cannon).toContainText(/Seat C\s*Letnev/);
  await expect(cannon).toContainText("PDS");
  await expect(cannon).toContainText("6+");
  await expect(cannon).toContainText("1 hit");
  // The side of the ships that the PDS fired at.
  await expect(panel(page).getByRole("region", { name: "Attacker" })).toContainText(
    "Advanced Carrier II",
  );
  await expect(panel(page).getByRole("group", { name: "Substeps" })).toHaveText(
    /✓ Move ships\s*✓ Gravity rift\s*✓ Space cannon offense/,
  );
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/movement-dice.png", animations: "disabled" });
  await expect(send(page)).toHaveText(/Continue to invasion/);
  await send(page).click();
  await expect(heading(page)).toHaveText("Invasion");
  expect(errors).toEqual([]);
});

test("a reload goes on where the game was, and undo takes back one answer @slow", async ({
  page,
}) => {
  await page.goto("/?local=3&players=8&humans=1");
  await expect(send(page)).toBeDisabled();
  await expect(undo(page)).toHaveCount(0);
  const prompts: string[] = [];
  for (let count = 0; count < 20; count++) {
    prompts.push((await heading(page).textContent()) ?? "");
    await answer(page);
  }
  const open = await heading(page).textContent();
  const round = await page.getByText(/^Round \d+ · /).textContent();
  await expect.poll(() => savedLine(page)).toContain("20 answers saved");

  await page.reload();
  await expect(send(page)).toBeDisabled();
  await expect(heading(page)).toHaveText(open ?? "");
  await expect(page.getByText(/^Round \d+ · /)).toHaveText(round ?? "");
  await expect.poll(() => savedLine(page)).toContain("20 answers saved");

  // Undo opens the choice that was answered last, and the same answer leads to the same game.
  await undo(page).click();
  await expect.poll(() => savedLine(page)).toContain("19 answers saved");
  await expect(heading(page)).toHaveText(prompts[19]);
  await expect(send(page)).toBeDisabled();
  await answer(page);
  await expect(heading(page)).toHaveText(open ?? "");

  // Three in a row, without waiting for a replay: the engine goes back to its checkpoints.
  for (const left of [19, 18, 17]) {
    await undo(page).click();
    await expect.poll(() => savedLine(page)).toContain(`${left} answers saved`);
    await expect(heading(page)).toHaveText(prompts[left]);
  }
  await expect(page.getByText(/^Replaying /)).toHaveCount(0);

  await openSettings(page);
  await settings(page).getByRole("button", { name: "New game" }).click();
  await expect(heading(page)).toHaveText(prompts[0]);
  await expect.poll(() => savedLine(page)).toContain("0 answers saved");
  await expect(undo(page)).toHaveCount(0);
});

test("an exported game is imported in another browser and opens at the same choice", async ({
  page,
  browser,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?local=5&players=8&humans=1");
  await expect(send(page)).toBeDisabled();
  for (let count = 0; count < 6; count++) {
    await answer(page);
  }
  const open = await heading(page).textContent();
  // A plan (the pools of Leadership) is sent as several answers, so the count is read.
  const answers = Number(/(\d+) answers saved/.exec(await savedLine(page))?.[1]);
  expect(answers).toBeGreaterThanOrEqual(6);
  await openSettings(page);
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    settings(page).getByRole("button", { name: "Export" }).click(),
  ]);
  await page.keyboard.press("Escape");
  const path = await download.path();

  const other = await (await browser.newContext()).newPage();
  // Another game is open: the imported game is a game of its own, at its id.
  await other.goto("/?local=3&players=8&humans=1");
  await expect(send(other)).toBeDisabled();
  await other.getByLabel("Saved game file").setInputFiles(path);
  await other.waitForURL((url) => url.searchParams.has("save"));
  await expect.poll(() => savedLine(other)).toContain("seed 5");
  await expect.poll(() => savedLine(other)).toContain(`${answers} answers saved`);
  await expect(heading(other)).toHaveText(open ?? "");

  // A file that is no saved game is refused, and says why.
  await other
    .getByLabel("Saved game file")
    .setInputFiles({ name: "x.json", mimeType: "application/json", buffer: Buffer.from("{}") });
  await openSettings(other);
  await expect(settings(other).getByRole("alert")).toHaveText("The file is not a saved game.");
  // The start screen lists both games of that browser: the game of the address and the import.
  await other.goto("/");
  const imported = other
    .getByRole("region", { name: "Saved games" })
    .getByRole("link", { name: /^Imported game/ });
  await expect(imported).toContainText(`${answers} answers`);
  await imported.click();
  await expect(heading(other)).toHaveText(open ?? "");
  await other.context().close();
});

test("a saved game of another engine build is not replayed without a word", async ({ page }) => {
  // Longer than a test of this suite should be (five loads of a game of eight): see "Tests" in
  // AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?local=3&players=8&humans=1");
  await expect(send(page)).toBeDisabled();
  await answer(page);
  // The page is left first: what it saves is written then.
  await page.goto("/");
  const saved = await written(page, "local.3.8.1", 1);
  expect(saved.summary.answers).toBe(1);
  await putRecord(page, { ...saved, engine: { ...saved.engine, wasmSha256: "another" } });
  await page.goto("/?local=3&players=8&humans=1");
  await expect(page.getByText("was played by another build of the engine")).toBeVisible();
  await page.getByRole("button", { name: "Replay anyway" }).click();
  await questionOpen(page);
  await expect.poll(() => savedLine(page)).toContain("1 answers saved");
  // The next answer is saved with this engine.
  await answer(page);
  await expect.poll(() => savedLine(page)).toContain("2 answers saved");
  await page.goto("/");
  await expect
    .poll(async () => (await written(page, "local.3.8.1", 2)).engine.wasmSha256)
    .not.toBe("another");

  // A decision that the game does not offer stops the replay and names it.
  const now = await written(page, "local.3.8.1", 2);
  await putRecord(page, { ...now, save: { ...now.save, decisions: ["no-such"] } });
  await page.goto("/?local=3&players=8&humans=1");
  await expect(page.getByRole("status")).toContainText(
    'The saved game does not fit this engine: decision 1 of 1 was "no-such"',
  );

  // The start screen deletes it, after a dialog that names it.
  await page.goto("/");
  const saves = page.getByRole("region", { name: "Saved games" });
  await expect(saves.getByRole("link")).toHaveText([/^Local game · seed 3/]);
  await saves.getByRole("button", { name: "Delete Local game · seed 3" }).click();
  await expect(page.getByRole("dialog")).toContainText("Delete the saved game?");
  await page.getByRole("dialog").getByRole("button", { name: "Keep" }).click();
  await expect(saves.getByRole("link")).toHaveCount(1);
  await saves.getByRole("button", { name: "Delete Local game · seed 3" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Delete", exact: true }).click();
  await expect(saves).toHaveCount(0);
  expect(await savedGames(page)).toEqual([]);
});

test("a saved game of an older version is not read: the start screen says so and deletes it", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByRole("link", { name: /Single player/ })).toBeVisible();
  // A record of the save store of before (web/ wrote it, or an older page of this app).
  await putRecord(page, { id: "old-game", schema: 1, summary: { humanSeat: 1 } });
  // A save of a page of before the save store, in localStorage.
  await page.evaluate(() => localStorage.setItem("ti4.single", JSON.stringify({ format: 5 })));
  await page.reload();
  const saves = page.getByRole("region", { name: "Saved games" });
  await expect(saves).toContainText(
    "1 saved game is of an older version. They cannot be played again.",
  );
  // The save of localStorage is removed at the start: it is not read.
  expect(await page.evaluate(() => localStorage.getItem("ti4.single"))).toBeNull();
  await saves.getByRole("button", { name: "Delete the saved games of an older version" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Delete", exact: true }).click();
  await expect(saves).toHaveCount(0);
  expect(await savedGames(page)).toEqual([]);
});

test("a draft of a tactical action is private, is kept, and is applied as one request", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  await page.goto("/?local=3&players=8&humans=1");
  const workspace = (name: string) =>
    page.getByRole("navigation", { name: "Workspace" }).getByRole("button", { name });
  const row = (name: string | RegExp) => panel(page).getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const apply = page.getByRole("button", { name: "Apply to Live" });
  // The draft opens in the strategy phase too. It is sent at the turn menu only.
  await workspace("Draft").click();
  await expect(heading(page)).toHaveText("Activation");
  await expect(apply).toBeDisabled();
  await workspace("Live").click();
  await row("8. Imperial").click();
  await send(page).click();
  await expect(row(/^Activate a system$/)).toBeVisible();

  // The draft opens at the choice of a system. The game is not asked anything.
  await workspace("Draft").click();
  await expect(heading(page)).toHaveText("Activation");
  await expect(page.getByText("Private draft").first()).toBeVisible();
  await expect(apply).toBeDisabled();
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);
  await system("23").click();
  await page.keyboard.press("Enter");

  // Every change of the movement of a draft is recorded. The main button records a movement
  // of nothing; after a change it goes on to the next step, when the action has one.
  await expect(heading(page)).toHaveText("Movement");
  await expect(send(page)).toHaveText(/Move nothing/);
  await expect(panel(page).getByText("Private to you until you apply.")).toBeVisible();
  await system("01").click();
  const sheet = page.getByRole("group", { name: /^Move from Jord/ });
  await sheet.getByRole("button", { name: "Advanced Carrier I 1 of 2" }).click();
  await sheet.getByRole("button", { name: "Destroyer I 1 of 1" }).click();
  await expect(sheet.getByText(/· 2 ships$/)).toBeVisible();
  await expect(system("23")).toHaveAttribute("aria-label", /2 Sol ships/);
  // Nothing lands and nothing is produced there: the action ends with the movement.
  await expect(send(page)).toHaveCount(0);
  await expect(page.locator("footer").getByText("The action ends here.")).toBeVisible();
  await page.screenshot({ path: "shots/local/draft-movement.png", animations: "disabled" });
  // Undo and redo are of the draft.
  await undo(page).click();
  await expect(sheet.getByText(/· 1 ship$/)).toBeVisible();
  await page.getByRole("button", { name: "Redo" }).click();
  await expect(sheet.getByText(/· 2 ships$/)).toBeVisible();
  await expect.poll(() => savedLine(page)).toContain("1 answers saved");

  // Live is as it was, and the draft is still there after a reload.
  await page.reload();
  await expect(row(/^Open your draft$/)).toBeVisible();
  await expect(system("23")).not.toHaveAttribute("aria-label", /Sol ship/);
  await workspace("Draft").click();
  await expect(heading(page)).toHaveText("Movement");
  await expect(system("23")).toHaveAttribute("aria-label", /2 Sol ships/);

  // "Apply to Live" says what it sends, and sends it.
  await apply.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText(/^Activate .+ · #23$/)).toBeVisible();
  await expect(dialog.getByText(/^2 ships from .+ · #01$/)).toBeVisible();
  await page.screenshot({ path: "shots/local/draft-apply.png", animations: "disabled" });
  await dialog.getByRole("button", { name: "Apply to Live" }).click();
  await expect(workspace("Live")).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => savedLine(page)).toContain("7 answers saved");
  await expect(system("23")).toHaveAttribute("aria-label", /2 Sol ships/);
  await expect(heading(page)).not.toHaveText("Movement");
  expect(errors).toEqual([]);
});

test("a recorded activation of a draft is edited: the movement follows what still fits", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  await page.goto("/?local=3&players=8&humans=1");
  const row = (name: string | RegExp) => panel(page).getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const tab = (name: string) => page.getByRole("tab", { name: new RegExp(name) });
  const edit = panel(page).getByRole("button", { name: "Edit activation" });
  const apply = page.getByRole("button", { name: "Apply to Live" });
  await row("8. Imperial").click();
  await send(page).click();
  await row(/^Activate a system$/).click();
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);
  await system("23").click();
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Movement");
  await system("01").click();
  const sheet = page.getByRole("group", { name: /^Move from Jord/ });
  await sheet.getByRole("button", { name: "Advanced Carrier I 1 of 2" }).click();
  await sheet.getByRole("button", { name: "Destroyer I 1 of 1" }).click();
  await expect(sheet.getByText(/· 2 ships$/)).toBeVisible();
  // The row of a ship lights its path on the map.
  await page.mouse.move(0, 0);
  await expect(page.locator(".galaxy .route.linked")).toHaveCount(0);
  await sheet.getByRole("button", { name: "Destroyer I 1 of 1" }).hover();
  await expect(page.locator(".galaxy .route.linked")).toHaveCount(1);
  await page.mouse.move(0, 0);

  // The recorded activation opens with its system chosen. Nothing changes until another
  // system is activated.
  await tab("Activation").click();
  await expect(heading(page)).toHaveText("Activation");
  await edit.click();
  await expect(panel(page).getByText("Editing")).toBeVisible();
  await expect(page.locator("g.system.chosen")).toHaveCount(1);
  await expect(apply).toBeDisabled();
  await page.screenshot({ path: "shots/local/draft-edit-activation.png", animations: "disabled" });
  await expect(page.locator("#step-panel > div.overflow-y-auto")).toHaveJSProperty("scrollTop", 0);
  const overflow = await page
    .locator("#step-panel > div.overflow-y-auto")
    .evaluate((element) => element.scrollHeight - element.clientHeight);
  expect(overflow, "the step needs vertical scroll").toBeLessThanOrEqual(0);
  // "Cancel edits" keeps the recorded system.
  await panel(page).getByRole("button", { name: "Cancel edits" }).click();
  await expect(edit).toBeVisible();
  await expect(panel(page).getByText("#23 Saudor")).toBeVisible();
  // The tab of a later step ends the edit too: the recorded steps are still there.
  await edit.click();
  await tab("Movement").click();
  await expect(heading(page)).toHaveText("Movement");
  await expect(system("23")).toHaveAttribute("aria-label", /2 Sol ships/);
  await tab("Activation").click();

  // Another system that the ships reach: the movement stays as it was.
  await edit.click();
  await system("71").click();
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Movement");
  await expect(system("71")).toHaveAttribute("aria-label", /2 Sol ships/);
  await expect(system("23")).not.toHaveAttribute("aria-label", /Sol ship/);
  await expect(apply).toBeEnabled();

  // A system that only the destroyer reaches: the carrier no longer fits, and Apply waits.
  await tab("Activation").click();
  await edit.click();
  await system("27").click();
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Movement");
  await expect(page.locator("footer").getByText(/no longer fits: remove it/)).toBeVisible();
  await expect(apply).toBeDisabled();
  // The step of the carrier needs review, and its row is marked with why, named by the board.
  await expect(tab("Movement")).toHaveAttribute("data-state", "needs-review");
  await expect(panel(page).getByText("Needs review", { exact: true })).toHaveCount(1);
  const review = panel(page).getByRole("group", { name: "Needs review" });
  await expect(review.getByText(/^✗ Advanced Carrier I from .+ · #01$/)).toBeVisible();
  await expect(review.getByText(/^Cannot reach .+ · #27$/)).toBeVisible();
  await expect(review).not.toContainText("sol_");
  await page.screenshot({ path: "shots/local/draft-edit-unfit.png", animations: "disabled" });
  const scroller = page.locator("#step-panel > div.overflow-y-auto");
  expect(
    await scroller.evaluate((element) => element.scrollHeight - element.clientHeight),
    "the step needs vertical scroll",
  ).toBeLessThanOrEqual(0);
  // Remove takes the carrier out: the destroyer still goes, and the draft fits again.
  await review.getByRole("button", { name: "Remove" }).click();
  await expect(review).toHaveCount(0);
  await expect(tab("Movement")).not.toHaveAttribute("data-state", "needs-review");
  await expect(system("27")).toHaveAttribute("aria-label", /1 Sol ship/);
  await expect(apply).toBeEnabled();
  // One undo brings the carrier back, and one more the activation of 71, with its movement.
  await undo(page).click();
  await expect(review).toBeVisible();
  await undo(page).click();
  await expect(system("71")).toHaveAttribute("aria-label", /2 Sol ships/);
  await expect(apply).toBeEnabled();
  expect(errors).toEqual([]);
});

test("a draft goes on to the landings and the production, and ends at a draw", async ({ page }) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  await page.goto("/?scenario=late-game&humans=1");
  const workspace = (name: string) =>
    page.getByRole("navigation", { name: "Workspace" }).getByRole("button", { name });
  const row = (name: string | RegExp) => panel(page).getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const tab = (name: string) => page.getByRole("tab", { name: new RegExp(`^\\S* ?${name}`) });
  const footer = page.locator("footer");
  const apply = page.getByRole("button", { name: "Apply to Live" });
  await expect(row(/^Activate a system$/)).toBeVisible();
  const saved = await savedLine(page);
  await workspace("Draft").click();
  // The systems are the choice once the engine answered the draft.
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);

  // The carrier of the north takes two infantry to the system with two free planets.
  await system("38").click();
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Movement");
  await system("63").click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await fleet.getByRole("button", { name: "Advanced Carrier II 1 of 1" }).click();
  await fleet.getByRole("button", { name: /^Load Spec Ops II, Planet Vorhal, 2 left/ }).click();
  await fleet
    .getByRole("button", { name: "Load Spec Ops II on Advanced Carrier II 1 of 1" })
    .first()
    .click();
  await fleet
    .getByRole("button", { name: "Load Spec Ops II on Advanced Carrier II 1 of 1" })
    .first()
    .click();
  // The game comes to the landings with this movement: the main button goes there.
  await expect(send(page)).toHaveText(/Go to invasion/);
  await expect(tab("Invasion")).toBeEnabled();
  await fleet.getByRole("button", { name: /^Done with/ }).click();
  await send(page).click();

  // The landings of the draft are the screen of the live step, staged in the same sheet.
  await expect(heading(page)).toHaveText("Invasion");
  await expect(page.getByText("Private draft").first()).toBeVisible();
  const landing = page.getByRole("group", { name: /^Land in / });
  await expect(landing).toBeVisible();
  await expect(send(page)).toHaveText(/Land nothing/);
  await page.screenshot({ path: "shots/local/draft-invasion.png", animations: "disabled" });
  // A unit on a planet that nobody explored: the card that it draws ends the draft.
  await mapPlanet(page, "Abyz").click();
  await expect(
    panel(page)
      .getByText(/^Abyz.*1 Spec Ops II/)
      .first(),
  ).toBeVisible();
  await expect(footer.getByText("The draft ends at a roll or a draw.")).toBeVisible();
  await expect(send(page)).toHaveCount(0);
  await expect(apply).toBeEnabled();
  await page.screenshot({ path: "shots/local/draft-boundary.png", animations: "disabled" });
  // The movement is still a step of the draft: its tab opens it, and the landings stay.
  await tab("Movement").click();
  await expect(heading(page)).toHaveText("Movement");
  await expect(system("38")).toHaveAttribute("aria-label", /1 Sol ship/);
  await send(page).click();
  await expect(heading(page)).toHaveText("Invasion");
  await expect(
    panel(page)
      .getByText(/^Abyz.*1 Spec Ops II/)
      .first(),
  ).toBeVisible();
  // Undo takes the landing back: with nothing landed, the action ends here.
  await undo(page).click();
  await expect(send(page)).toHaveText(/Land nothing/);
  await send(page).click();
  await expect(footer.getByText("The action ends here.")).toBeVisible();

  // Another action: the system of the two space docks, where nothing moves.
  await page.getByRole("button", { name: "More draft actions" }).click();
  await page.getByRole("menuitem", { name: "Start over" }).click();
  await expect(heading(page)).toHaveText("Activation");
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);
  await system("72").click();
  await page.keyboard.press("Enter");
  await expect(send(page)).toHaveText(/Move nothing/);
  await send(page).click();
  // AI Development Algorithm asks before the production. In a draft the answer is recorded.
  await row(/^Decline/i).click();
  await expect(send(page)).toHaveText(/Record/);
  await send(page).click();

  // The production of the draft: the build in its sheet, then the payment on the map.
  await expect(heading(page)).toHaveText("Production");
  const build = page.getByRole("group", { name: /^Produce in / });
  await expect(build).toBeVisible();
  await expect(send(page)).toHaveText(/Produce nothing/);
  const ships = build.getByRole("group", { name: "Ships · Space area" });
  await ships.getByRole("button", { name: "Add Cruiser II" }).click();
  await expect(build.getByRole("status").first()).toHaveText("1 unit · 1 resource · discount 1");
  await expect(system("72")).toHaveAttribute("aria-label", /Sol ship/);
  // A build that is not paid is not in the draft yet.
  await expect(footer.getByText("The production is in the draft when it is paid.")).toBeVisible();
  await expect(send(page)).toHaveText(/Pay 1 resource/);
  await page.screenshot({ path: "shots/local/draft-production.png", animations: "disabled" });
  await send(page).click();
  await expect(build).toBeHidden();
  const bar = page.getByRole("group", { name: "Payment" });
  await expect(bar.getByRole("status").first()).toHaveText(/Paying\s*0 \/ 1\s*resource/);
  await mapPlanet(page, "Lisis").click();
  await expect(bar.getByRole("status").first()).toHaveText(/2 \/ 1\s*resource/);
  // Paid: the game played the whole action.
  await expect(footer.getByText("The action ends here. Every step is drafted.")).toBeVisible();
  await page.screenshot({ path: "shots/local/draft-payment.png", animations: "disabled" });

  // Nothing of it is in the game, and a reload keeps it.
  expect(await savedLine(page)).toBe(saved);
  await page.reload();
  await expect(row(/^Open your draft$/)).toBeVisible();
  await workspace("Draft").click();
  await expect(heading(page)).toHaveText("Production");
  await expect(build.getByRole("status").first()).toHaveText("1 unit · 1 resource · discount 1");

  // Apply says what it sends, and the game is where the draft said.
  await apply.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("No ship moves")).toBeVisible();
  await expect(dialog.getByText("1 purchase produced and paid")).toBeVisible();
  await dialog.getByRole("button", { name: "Apply to Live" }).click();
  await expect(workspace("Live")).toHaveAttribute("aria-pressed", "true");
  // The next decision is open before the map is clicked: its update would close the inspector.
  await expect(heading(page)).toHaveText("Place up to 2 infantry");
  await system("72").click();
  const inspector = page
    .locator("section[aria-label='Galaxy board'] > div")
    .filter({ hasText: "Velnor" });
  await expect(inspector.getByText(/2 Cruiser II/)).toBeVisible();
  await expect(inspector.getByText("Exhausted", { exact: true })).toHaveCount(1);
  expect(errors).toEqual([]);
});

test("the other seats wait for a step when the Dev sheet says so", async ({ page }) => {
  await page.goto("/?local=3&players=8&humans=1");
  await expect(send(page)).toBeDisabled();
  const seats = settings(page).getByRole("group", { name: "Other seats" });
  await openSettings(page);
  await expect(seats.getByRole("button", { name: "Run" })).toHaveAttribute("aria-pressed", "true");
  await seats.getByRole("button", { name: "Step" }).click();
  await page.keyboard.press("Escape");
  await panel(page).getByRole("button", { name: "8. Imperial" }).click();
  await send(page).click();
  // The next seat has not chosen: the game waits, and the main button lets it decide.
  await expect(send(page)).toHaveText(/^Step/);
  await expect(panel(page).getByText(/decides next\.$/)).toBeVisible();
  const waiting = await page.locator("header strong").textContent();
  await page.keyboard.press("Enter");
  await expect(page.locator("header strong")).not.toHaveText(waiting ?? "");
  await expect(send(page)).toHaveText(/^Step/);
  // The setting is of the page: a reload keeps it. With Run the seats play on to the next choice.
  await page.reload();
  await expect(send(page)).toHaveText(/^Step/);
  await openSettings(page);
  await expect(seats.getByRole("button", { name: "Step" })).toHaveAttribute("aria-pressed", "true");
  await seats.getByRole("button", { name: "Run" }).click();
  await page.keyboard.press("Escape");
  await expect(page.locator("header strong")).toHaveText("Your decision");
});

test("a secondary is drafted while the primary is resolved, and sent when the seat is reached", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  // The other seats wait for a step, so that the page is shown the game between their decisions.
  await page.addInitScript(() => localStorage.setItem("ti4.local.stepping", "1"));
  // In the game of this seed Leadership and Politics are played before the seat has its turn, and
  // seats follow Politics after this one, so the list is still in view when the draft is sent.
  await page.goto("/?local=4&players=8&humans=1");
  await panel(page).getByRole("button", { name: "8. Imperial" }).click();
  await send(page).click();
  const step = page.getByRole("button", { name: /^Step/ });
  const status = page.locator("header strong");
  /** Lets the other seats decide until `done` says so. */
  const stepUntil = async (done: () => Promise<boolean>) => {
    for (let steps = 0; steps < 200 && !(await done()); steps++) {
      const before = await status.textContent();
      await step.click();
      // The step is taken when the game says something else. A seat that decides twice says
      // the same: then the next step is tried after a moment.
      await expect
        .poll(async () => (await done()) || (await status.textContent()) !== before, {
          timeout: 300,
        })
        .toBe(true)
        .catch(() => {});
    }
    expect(await done()).toBe(true);
  };
  /** Lets the other seats decide until the panel says `text`. */
  const stepTo = (text: string | RegExp) =>
    stepUntil(() => panel(page).getByText(text).first().isVisible());

  // Leadership is played first. It asks this seat nothing, so its row is no draft.
  await stepTo("Nothing to decide as the game is now");
  await expect(panel(page).getByText("✎")).toHaveCount(0);

  // Politics is played: the row of the viewer is the draft, while the primary is resolved.
  await stepTo("✎ Private draft");
  await expect(panel(page).getByRole("heading", { name: "Politics" })).toBeVisible();
  await expect(panel(page).getByText("▶ Deciding")).toHaveCount(1);
  await expect(panel(page).getByText("Strategy pool 2 → 1")).toBeVisible();
  // The question is of the draft: the header says who the game waits for.
  await expect(status).toHaveText(/^Waiting for /);
  await expect(panel(page).getByRole("button", { name: /Draw/ })).toBeVisible();
  await page.screenshot({ path: "shots/local/secondary-draft.png", animations: "disabled" });

  // An answer is recorded, not sent.
  const saved = await savedLine(page);
  await page.keyboard.press("1");
  await expect(send(page)).toHaveText(/^Record/);
  await page.keyboard.press("Enter");
  await expect(send(page)).toHaveText(/^Mark draft ready/);
  await page.keyboard.press("Enter");
  await expect(panel(page).getByText("✎ Draft ready")).toBeVisible();
  await expect(panel(page).getByText(/^Decline · not sent, resolves after /)).toBeVisible();
  await expect(page.getByRole("button", { name: "Change draft" })).toBeVisible();
  expect(await savedLine(page)).toBe(saved);
  await page.screenshot({ path: "shots/local/secondary-ready.png", animations: "disabled" });

  // The game comes to the seat: the draft is its answer, and the seat is not asked.
  // (Another seat has passed before: the row of the viewer is the one that is waited for.)
  const mine = panel(page).getByText("You", { exact: true }).locator("xpath=..");
  await stepUntil(async () => (await mine.getByText("– Passed").count()) > 0);
  await expect(panel(page).getByText("✎")).toHaveCount(0);
  expect(await savedLine(page)).not.toBe(saved);
  await expect(page.getByRole("button", { name: /^Step/ })).toBeVisible();
  await page.screenshot({ path: "shots/local/secondary-sent.png", animations: "disabled" });
  expect(errors).toEqual([]);
});

test("a research is chosen in the technology tree, and the second one can research nothing", async ({
  page,
}) => {
  const errors = pageErrors(page);
  // Sol plays Technology with Antimass Deflectors and Neural Motivator, a ready Biotic skip, and
  // six trade goods. The scenario stops at the free research.
  await page.goto("/?scenario=research_tech_skips&humans=1");
  const tree = panel(page).getByRole("heading", { name: "Research", exact: true }).locator("..");
  await expect(tree).toBeVisible();
  await expect(panel(page).getByRole("heading", { name: "Technology" })).toBeVisible();
  // The columns say what stands for prerequisites; the tree has Sol's upgrades and no Carrier II.
  await expect(panel(page).getByText(/^Blue 1$/)).toBeVisible();
  await expect(panel(page).getByText(/^Green 1 · \+1 /)).toBeVisible();
  await expect(panel(page).getByText("Unit upgrades")).toBeVisible();
  await expect(panel(page).getByRole("button", { name: /^Advanced Carrier II/ })).toBeVisible();
  await expect(panel(page).getByRole("button", { name: /^Carrier II/ })).toHaveCount(0);
  // An owned technology and one with missing prerequisites cannot be chosen.
  await expect(
    panel(page).getByRole("button", { name: "Antimass Deflectors, researched" }),
  ).toBeDisabled();
  await expect(
    panel(page).getByRole("button", { name: /^War Sun, 4 prerequisites missing/ }),
  ).toBeDisabled();
  await expect(send(page)).toBeDisabled();
  await page.screenshot({ path: "shots/local/research-tree.png", animations: "disabled" });

  // A cell stages its technology; the header names it with its price.
  await panel(page).getByRole("button", { name: "Gravity Drive", exact: true }).click();
  await expect(tree).toContainText("Gravity Drive · free");
  await expect(page.locator("footer")).toContainText("Selected: Gravity Drive");
  await page.keyboard.press("Enter");

  // The second research costs six resources and may research nothing.
  await expect(tree).toContainText("Costs 6 resources");
  await expect(
    panel(page).getByRole("button", { name: "Gravity Drive, researched" }),
  ).toBeVisible();
  await expect(send(page)).toHaveText(/Research nothing/);
  await panel(page).getByRole("button", { name: "Sarween Tools", exact: true }).click();
  await expect(tree).toContainText("Sarween Tools · 6 resources");
  await expect(send(page)).toHaveText(/^Send/);
  await page.keyboard.press("Escape");
  await expect(send(page)).toHaveText(/Research nothing/);
  await page.keyboard.press("Enter");
  await expect(tree).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("the status phase gains the command tokens in the one redistribution, in the pool editor", async ({
  page,
}) => {
  // The load of the scenario, an answer, the editor and a second answer.
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol is in the status phase with 2 · 3 · 2 on its sheet. It gains 2 tokens, and 1 more for
  // Versatile.
  await page.goto("/?scenario=score_objective_status&humans=1");
  const seat = seatRow(page, "Seat A");
  await expect(seat).toContainText("2 · 3 · 2");
  await expect(heading(page)).toHaveText("Score an objective");
  await panel(page).getByRole("button", { name: "Decline" }).click();
  await send(page).click();

  // No question of a pool for each gained token: the gain is placed in the pools.
  await expect(heading(page)).toHaveText("Redistribute your command tokens");
  await expect(page.getByText(/into which pool/i)).toHaveCount(0);
  const pools = panel(page).getByRole("heading", { name: "Command pools" }).locator("..");
  await expect(pools).toContainText("7 / 10 placed");
  await expect(page.locator("footer")).toContainText("Place 3 more tokens");
  await expect(send(page)).toBeDisabled();
  await page.screenshot({ path: "shots/local/command-pools.png", animations: "disabled" });
  // The fleet pool keeps the least that the game offers: 2. Esc puts the pools back.
  const fewer = panel(page).getByRole("button", { name: "Remove fleet pool token" });
  await fewer.click();
  await expect(fewer).toBeDisabled();
  await expect(pools).toContainText("6 / 10 placed");
  await page.keyboard.press("Escape");
  await expect(pools).toContainText("7 / 10 placed");

  for (const pool of ["tactic", "fleet", "strategy"]) {
    await panel(page)
      .getByRole("button", { name: `Add ${pool} pool token` })
      .click();
  }
  await expect(pools).toContainText("10 / 10 placed");
  await expect(panel(page).getByRole("button", { name: "Add tactic pool token" })).toBeDisabled();
  await expect(page.locator("footer")).toContainText("Tactic 2 → 3 · Fleet 3 → 4 · Strategy 2 → 3");
  await expect(send(page)).toBeEnabled();
  await send(page).click();
  await expect(seat).toContainText("3 · 4 · 3");
  expect(errors).toEqual([]);
});

test("Leadership: the gained and the bought tokens are placed in the pools, paid on the map and sent as one plan", async ({
  page,
}) => {
  // The load of the scenario, the editor, the payment and an undo.
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol holds Leadership with 3 · 3 · 2 on its sheet, 4 trade goods and three ready planets that
  // pay in influence.
  await page.goto("/?scenario=strategy_leadership&humans=1");
  const seat = seatRow(page, "Seat A");
  await expect(heading(page)).toHaveText("Choose an action");
  await page.keyboard.press("1");
  await page.keyboard.press("Enter");

  // One screen: the purchase, the pools with the gained tokens, and no question of a pool.
  await expect(heading(page)).toHaveText("Leadership");
  await expect(page.getByText(/into which pool/i)).toHaveCount(0);
  const buy = panel(page).getByRole("heading", { name: "Buy command tokens" }).locator("..");
  const pools = panel(page).getByRole("heading", { name: "Command pools" }).locator("..");
  await expect(buy).toContainText("0 bought · 0 influence");
  await expect(pools).toContainText("3 gained · 0 bought · 0 / 3 placed");
  await expect(page.locator("footer")).toContainText("Place 3 more tokens.");
  await expect(send(page)).toBeDisabled();
  // The printed card is behind the hover of the heading.
  await panel(page).getByRole("button", { name: "Rules: Leadership" }).hover();
  await expect(page.getByText(/Gain 3 command tokens/).first()).toBeVisible();
  await page.mouse.move(0, 0);

  // Two bought tokens: the map is the payment.
  const more = panel(page).getByRole("button", { name: "Add bought token" });
  await more.click();
  await more.click();
  await expect(buy).toContainText("2 bought · 6 influence");
  await expect(pools).toContainText("3 gained · 2 bought · 0 / 5 placed");
  for (const pool of ["tactic", "tactic", "fleet", "strategy", "strategy"]) {
    await panel(page)
      .getByRole("button", { name: `Add ${pool} pool token` })
      .click();
  }
  await expect(pools).toContainText("5 / 5 placed");
  await expect(panel(page).getByRole("button", { name: "Add fleet pool token" })).toBeDisabled();
  await expect(page.locator("footer")).toContainText("Pay 6 more influence.");
  const bar = page.getByRole("group", { name: "Payment" });
  await expect(bar.getByRole("status").first()).toHaveText(/0 \/ 6\s*influence\s*6 more/);
  await expect(page.locator("g.planet.payable")).toHaveCount(3);
  await mapPlanet(page, "Rarron").click();
  await bar.getByRole("button", { name: "Pay rest" }).click();
  await expect(bar.getByRole("status").first()).toHaveText(/6 \/ 6\s*influence\s*✓ Paid/);
  await expect(page.locator("footer")).toContainText("Tactic 3 → 5 · Fleet 3 → 4 · Strategy 2 → 4");
  await expect(send(page)).toHaveText(/Buy 2 tokens/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/leadership.png", animations: "disabled" });

  // Esc clears the screen; the main button sends all of it, and the game asks nothing more.
  await page.keyboard.press("Escape");
  await expect(pools).toContainText("0 / 3 placed");
  await expect(bar).toHaveCount(0);
  await more.click();
  await panel(page).getByRole("button", { name: "Add fleet pool token" }).click();
  for (const pool of ["tactic", "tactic", "strategy"]) {
    await panel(page)
      .getByRole("button", { name: `Add ${pool} pool token` })
      .click();
  }
  await bar.getByRole("button", { name: "Auto-pay" }).click();
  await expect(bar.getByRole("status").first()).toHaveText(/3 \/ 3\s*influence\s*✓ Paid/);
  await send(page).click();
  await expect(heading(page)).toHaveText("End your turn");
  await expect(seat).toContainText("5 · 4 · 3");
  expect(errors).toEqual([]);
});

test("Leadership: the secondary buys tokens on the same screen, with no strategy token", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol and Hacan are played here: Sol plays Leadership and gains its tokens, then Hacan follows.
  await page.goto("/?scenario=strategy_leadership&humans=3");
  await expect(heading(page)).toHaveText("Choose an action");
  await page.keyboard.press("1");
  await page.keyboard.press("Enter");
  for (const pool of ["tactic", "fleet", "strategy"]) {
    await panel(page)
      .getByRole("button", { name: `Add ${pool} pool token` })
      .click();
  }
  await expect(send(page)).toHaveText(/^Send/);
  await send(page).click();
  // Hacan's row opens into the purchase: no pool is gained, and nothing bought is a pass.
  const hacan = seatRow(page, "Seat B");
  const pools = panel(page).getByRole("heading", { name: "Command pools" }).locator("..");
  await expect(pools).toContainText("0 bought · 0 / 0 placed");
  await expect(send(page)).toHaveText(/Buy nothing/);
  await panel(page).getByRole("button", { name: "Add bought token" }).click();
  await panel(page).getByRole("button", { name: "Add fleet pool token" }).click();
  const bar = page.getByRole("group", { name: "Payment" });
  await bar.getByRole("button", { name: "Auto-pay" }).click();
  await expect(bar.getByRole("status").first()).toContainText("✓ Paid");
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/leadership-secondary.png", animations: "disabled" });
  await send(page).click();
  await expect(hacan).toContainText("3 · 4 · 2");
  expect(errors).toEqual([]);
});

test("Diplomacy: the system and the planets to ready are chosen on the map and sent at once", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol holds Diplomacy; Jord and both planets of #37 are exhausted (1 of 6 resources ready).
  await page.goto("/?scenario=strategy_diplomacy&humans=1");
  const seat = seatRow(page, "Seat A");
  await expect(heading(page)).toHaveText("Choose an action");
  await expect(seat).toContainText("1/6");
  await panel(page)
    .getByRole("button", { name: /^2 Diplomacy/ })
    .click();
  await page.keyboard.press("Enter");

  // One screen: the system first, then the planets, and no question of a planet.
  await expect(heading(page)).toHaveText("Diplomacy");
  const system = panel(page).getByRole("heading", { name: "System" }).locator("../..");
  const ready = panel(page).getByRole("heading", { name: "Ready planets" }).locator("../..");
  await expect(system).toContainText("No system chosen");
  await expect(ready).toContainText("0 / 2 · after the system");
  await expect(page.locator("footer")).toContainText("Choose a system on the map.");
  await expect(send(page)).toBeDisabled();
  // The printed card of this set is behind the hover of the heading.
  await panel(page).getByRole("button", { name: "Rules: Diplomacy" }).hover();
  await expect(page.getByText(/ready up to 2 exhausted planets you control/).first()).toBeVisible();
  await page.mouse.move(0, 0);

  // The systems with a planet of Sol, but Mecatol Rex: Jord, #29 and #37.
  await expect(page.locator("g.system.target")).toHaveCount(3);
  await page.locator("g.system.target").filter({ hasText: "#37" }).click();
  await expect(system).toContainText("#37");
  await expect(system).toContainText("Hacan and Letnev place a command token here");
  await page.getByRole("button", { name: "Close system details" }).click();
  await expect(page.locator("g.planet.payable")).toHaveCount(3);
  await mapPlanet(page, "Jord").click();
  await mapPlanet(page, "Meer").click();
  // No more than the game readies: a third planet is not taken.
  await mapPlanet(page, "Arinam").click();
  await expect(ready).toContainText("2 / 2");
  await expect(ready).not.toContainText("Arinam");
  await expect(ready).toContainText("Exhausted → ready · 4 resources / 2 influence");
  // "Change" gives a planet back; Esc clears the last choice.
  await ready.getByRole("button", { name: "Change" }).last().click();
  await expect(ready).toContainText("1 / 2");
  await page.keyboard.press("Escape");
  await expect(ready).toContainText("0 / 2");
  await expect(system).toContainText("#37");
  await mapPlanet(page, "Jord").click();
  await mapPlanet(page, "Arinam").click();
  await expect(page.locator("footer")).toContainText("#37 · Ready Jord, Arinam");
  await expect(send(page)).toHaveText(/Resolve primary/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/diplomacy.png", animations: "disabled" });

  // The main button answers the system and both planets; the game asks nothing more of them.
  await send(page).click();
  await expect(heading(page)).toHaveText("End your turn");
  await expect(seat).toContainText("6/6");
  expect(errors).toEqual([]);
});

test("Diplomacy: the secondary readies on the map; nothing chosen is a pass", async ({ page }) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol and Hacan are played here; the three planets of Hacan's home are exhausted.
  await page.goto("/?scenario=strategy_diplomacy&humans=3");
  await playStrategyCard(page, "2 Diplomacy");
  await page.locator("g.system.target").filter({ hasText: "#29" }).click();
  await page.getByRole("button", { name: "Close system details" }).click();
  await mapPlanet(page, "Jord").click();
  await mapPlanet(page, "Arinam").click();
  await send(page).click();

  // Hacan's row opens into the planets, with what the secondary costs.
  const hacan = seatRow(page, "Seat B");
  const ready = panel(page).getByRole("heading", { name: "Ready planets" }).locator("../..");
  await expect(ready).toContainText("0 / 2 · Choose on the map · Strategy pool 2 → 1");
  await expect(panel(page).getByRole("heading", { name: "System" })).toHaveCount(0);
  await expect(send(page)).toHaveText(/^Pass/);
  await expect(page.locator("g.planet.payable")).toHaveCount(3);
  await mapPlanet(page, "Arretze").click();
  await expect(page.locator("footer")).toContainText("Choose 1 more planet on the map.");
  await expect(send(page)).toBeDisabled();
  await mapPlanet(page, "Hercant").click();
  await expect(send(page)).toHaveText(/Follow · ready 2 planets/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/diplomacy-secondary.png", animations: "disabled" });
  await send(page).click();
  await expect(hacan).toContainText("3/3");
  await expect(hacan).toContainText("3 · 3 · 1");
  expect(errors).toEqual([]);
});

test("Diplomacy of the base game has its own printed text and the same screen", async ({
  page,
}) => {
  const errors = pageErrors(page);
  await page.goto("/?scenario=strategy_diplomacy_base&humans=1");
  await playStrategyCard(page, "2 Diplomacy");
  await expect(heading(page)).toHaveText("Diplomacy");
  await panel(page).getByRole("button", { name: "Rules: Diplomacy" }).hover();
  await expect(
    page.getByText(/ready each exhausted planet you control in that system/).first(),
  ).toBeVisible();
  await page.mouse.move(0, 0);
  // The engine asks what it asks for the Codex I card (`web2/NEXT.md`): the same screen.
  await expect(panel(page).getByRole("heading", { name: "System" })).toBeVisible();
  await expect(
    panel(page).getByRole("heading", { name: "Ready planets" }).locator(".."),
  ).toContainText("0 / 2 · after the system");
  expect(errors).toEqual([]);
});

test("Politics: the speaker is chosen in the player table, then the agenda deck is placed on one screen", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol holds Politics and is the speaker; it has no action cards.
  await page.goto("/?scenario=strategy_politics&humans=1");
  const row = (name: string) =>
    page.getByRole("region", { name: "Players" }).getByRole("row").filter({ hasText: name });
  await expect(heading(page)).toHaveText("Choose an action");
  await expect(row("Seat A")).toContainText("Speaker");
  await panel(page)
    .getByRole("button", { name: /^3 Politics/ })
    .click();
  await page.keyboard.press("Enter");

  // The speaker in the player table, and the action cards that are drawn after it.
  await expect(heading(page)).toHaveText("Politics");
  const speaker = panel(page).getByRole("heading", { name: "Speaker" }).locator("../..");
  await expect(speaker).toContainText("No player chosen");
  await expect(panel(page)).toContainText("Action cards 0 → 2");
  await expect(send(page)).toHaveText(/Send · look at the agenda deck/);
  await expect(send(page)).toBeDisabled();
  await panel(page).getByRole("button", { name: "Rules: Politics" }).hover();
  await expect(page.getByText(/Choose a player other than the speaker/).first()).toBeVisible();
  await page.mouse.move(0, 0);
  await row("Seat C").click();
  await expect(speaker).toContainText("Speaker: Sol → Letnev");
  await fitsHeight(page);
  await send(page).click();

  // The agenda cards as printed, each on top or on the bottom, and the deck as the game builds it.
  const deck = panel(page).getByRole("heading", { name: "Agenda deck after" }).locator("../..");
  await expect(deck).toContainText("0 / 2 placed");
  await expect(row("Seat C")).toContainText("Speaker");
  await expect(panel(page).getByText(/^Agenda · /)).toHaveCount(2);
  await expect(send(page)).toHaveText(/Place 2 agenda cards/);
  await expect(send(page)).toBeDisabled();
  await page.keyboard.press("1");
  await page.keyboard.press("3");
  await expect(deck).toContainText("2 / 2 placed");
  // Two agendas on top: the one asked second is the next agenda.
  const note = (await page.locator("footer").textContent()) ?? "";
  const [first, second] = [...note.matchAll(/(?:^|· )([^·]+?) on top/g)].map((found) => found[1]);
  await expect(deck).toContainText(
    `Top of the deck · the next agenda${second}Second from the top${first}`,
  );
  await page.keyboard.press("Escape");
  await expect(deck).toContainText("1 / 2 placed");
  await page.keyboard.press("4");
  await expect(deck).toContainText("Bottom of the deck");
  await expect(page.locator("footer")).toContainText(/on top · .* on the bottom/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/politics.png", animations: "disabled" });

  // The main button answers both agendas; the game asks nothing more of the primary.
  await send(page).click();
  await expect(heading(page)).toHaveText("End your turn");
  expect(errors).toEqual([]);
});

test("Politics: another seat follows as the game asks it, and does not see the agenda cards", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol and Hacan are played here: Sol plays Politics, then Hacan follows.
  await page.goto("/?scenario=strategy_politics&humans=3");
  await playStrategyCard(page, "3 Politics");
  await page
    .getByRole("region", { name: "Players" })
    .getByRole("row")
    .filter({
      hasText: "Seat B",
    })
    .click();
  await send(page).click();
  const deck = panel(page).getByRole("heading", { name: "Agenda deck after" });
  await expect(deck).toBeVisible();
  await page.keyboard.press("2");
  await page.keyboard.press("4");
  const note = (await page.locator("footer").textContent()) ?? "";
  const agendas = [...note.matchAll(/(?:^|· )([^·]+?) on the bottom/g)].map((found) => found[1]);
  expect(agendas).toHaveLength(2);
  await send(page).click();

  // Hacan's row opens into the question of the game, with what it costs.
  await expect(panel(page).getByText("Strategy pool 2 → 1")).toBeVisible();
  await expect(deck).toHaveCount(0);
  for (const agenda of agendas) {
    await expect(page.locator("body")).not.toContainText(agenda);
  }
  await expect(panel(page).getByRole("button", { name: /Draw/ })).toBeVisible();
  await fitsHeight(page);
  await page.keyboard.press("2");
  await send(page).click();
  const hacan = seatRow(page, "Seat B");
  await expect(hacan).toContainText("3 · 3 · 1");
  expect(errors).toEqual([]);
});

test("Construction: the structure in the panel, the planet on the map; both are sent at once", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol holds Construction; Jord has Sol's space dock, Qucen'n (#29) a PDS of Sol.
  await page.goto("/?scenario=strategy_construction&humans=1");
  await playStrategyCard(page, "4 Construction");

  // One screen: the gauges, the kind of the first structure, and both structures.
  await expect(heading(page)).toHaveText("Construction");
  const structures = panel(page).getByRole("heading", { name: "Structures" }).locator("../..");
  await expect(panel(page)).toContainText("PDS on the board");
  await expect(panel(page)).toContainText("1 / 6");
  await expect(panel(page).getByText("Structure 1 of 2")).toBeVisible();
  await expect(structures).toContainText("1 of 2 · space dock or PDS");
  await expect(structures).toContainText("2 of 2 · PDS only");
  await expect(page.locator("footer")).toContainText("Choose a structure, then a planet");
  // The printed card of this set is behind the hover of the heading.
  await panel(page).getByRole("button", { name: "Rules: Construction" }).hover();
  await expect(
    page.getByText(/Spend 1 token from your strategy pool and place it/).first(),
  ).toBeVisible();
  await page.mouse.move(0, 0);

  // A PDS on Qucen'n fills it: the PDS of the second structure goes elsewhere.
  await page.keyboard.press("2");
  await mapPlanet(page, "Qucen'n").click();
  await expect(structures).toContainText("Qucen'n · #29PDS 1 → 2");
  // Jord, Rarron and the two planets of #37, with the chosen Qucen'n marked.
  await expect(page.locator("g.planet.payable")).toHaveCount(5);
  await expect(page.locator("g.planet.chosen")).toHaveCount(1);
  await mapPlanet(page, "Qucen'n").click();
  await expect(structures).toContainText("2 of 2 · PDS only");
  // "Change" gives the planet back, and the kind is chosen again.
  await structures.getByRole("button", { name: "Change" }).click();
  await page.keyboard.press("1");
  await expect(panel(page)).toContainText("Space dock");
  await mapPlanet(page, "Rarron").click();
  await mapPlanet(page, "Qucen'n").click();
  await expect(structures).toContainText("Rarron · #29Space dock 0 → 1");
  await expect(structures).toContainText("Qucen'n · #29PDS 1 → 2");
  await expect(panel(page)).toContainText("2 / 3");
  await expect(page.locator("footer")).toContainText("Space dock on Rarron · PDS on Qucen'n");
  await expect(send(page)).toHaveText(/Resolve primary/);
  // Esc clears the last planet.
  await page.keyboard.press("Escape");
  await expect(send(page)).toHaveText(/Resolve primary · 1 structure/);
  await mapPlanet(page, "Qucen'n").click();
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/construction.png", animations: "disabled" });

  // The main button answers both structures; the game asks nothing more of them.
  await send(page).click();
  await expect(heading(page)).toHaveText("End your turn");
  expect(errors).toEqual([]);
});

test("Construction: the secondary places one structure on the map; nothing chosen is a pass", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol and Hacan are played here.
  await page.goto("/?scenario=strategy_construction&humans=3");
  await playStrategyCard(page, "4 Construction");
  await page.keyboard.press("1");
  await mapPlanet(page, "Rarron").click();
  await send(page).click();

  // Hacan's row opens into the structure, with what the secondary costs.
  const planet = panel(page).getByRole("heading", { name: "Planet" }).locator("../..");
  await expect(planet).toContainText("Choose on the map · Strategy pool 2 → 1");
  await expect(planet).toContainText("Space dock or PDS");
  await expect(send(page)).toHaveText(/^Pass/);
  await page.keyboard.press("2");
  await expect(page.locator("g.planet.payable")).toHaveCount(3);
  await mapPlanet(page, "Hercant").click();
  await expect(planet).toContainText("PDS 0 → 1");
  await expect(send(page)).toHaveText(/Follow · place PDS/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/construction-secondary.png", animations: "disabled" });
  await send(page).click();
  const hacan = seatRow(page, "Seat B");
  await expect(hacan).toContainText("3 · 3 · 1");
  expect(errors).toEqual([]);
});

test("Construction of Thunder's Edge: the ability first, then two structures of any kind", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  await page.goto("/?scenario=strategy_construction_te&humans=1");
  await playStrategyCard(page, "4 Construction");
  await expect(heading(page)).toHaveText("Construction");
  await panel(page).getByRole("button", { name: "Rules: Construction" }).hover();
  await expect(
    page.getByText(/use the PRODUCTION ability of 1 of your space docks/).first(),
  ).toBeVisible();
  await page.mouse.move(0, 0);
  await expect(page.locator("footer")).toContainText("Choose an ability");
  await expect(send(page)).toBeDisabled();
  // A production is sent alone: the second structure is asked after it.
  await page.keyboard.press("2");
  await expect(send(page)).toHaveText(/Resolve primary · use PRODUCTION/);
  await expect(panel(page)).toContainText("After the production");
  // A structure places two, each of any kind.
  await page.keyboard.press("1");
  const structures = panel(page).getByRole("heading", { name: "Structures" }).locator("../..");
  await expect(structures).toContainText("1 of 2 · space dock or PDS");
  await expect(structures).toContainText("2 of 2 · space dock or PDS");
  await page.keyboard.press("3");
  await mapPlanet(page, "Rarron").click();
  await expect(panel(page).getByText("Structure 2 of 2")).toBeVisible();
  await page.keyboard.press("4");
  await mapPlanet(page, "Jord").click();
  await expect(page.locator("footer")).toContainText("Space dock on Rarron · PDS on Jord");
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/construction-te.png", animations: "disabled" });
  await send(page).click();
  await expect(heading(page)).toHaveText("End your turn");
  expect(errors).toEqual([]);
});

test("Construction of the base game has its own printed text and the same screen", async ({
  page,
}) => {
  const errors = pageErrors(page);
  await page.goto("/?scenario=strategy_construction_base&humans=1");
  await playStrategyCard(page, "4 Construction");
  await expect(heading(page)).toHaveText("Construction");
  await panel(page).getByRole("button", { name: "Rules: Construction" }).hover();
  await expect(
    page.getByText(/Place 1 token from your strategy pool in any system/).first(),
  ).toBeVisible();
  await page.mouse.move(0, 0);
  const structures = panel(page).getByRole("heading", { name: "Structures" }).locator("../..");
  await expect(structures).toContainText("2 of 2 · PDS only");
  expect(errors).toEqual([]);
});

test("Trade: the own gain is a value, the other players are chosen in the player table and sent at once", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol holds Trade with 4 trade goods and 1 commodity; Hacan and Letnev have no commodities.
  await page.goto("/?scenario=strategy_trade&humans=1");
  const row = (name: string) =>
    page.getByRole("region", { name: "Players" }).getByRole("row").filter({ hasText: name });
  await playStrategyCard(page, "5 Trade");

  // One screen: what Sol gained, and the other players.
  await expect(heading(page)).toHaveText("Trade");
  const own = panel(page).getByRole("heading", { name: "Your seat" }).locator("../..");
  const others = panel(page).getByRole("heading", { name: "Other players" }).locator("../..");
  await expect(own).toContainText("Trade goods 4 → 7");
  await expect(own).toContainText("Commodities 1 → 4");
  await expect(others).toContainText("0 / 2 chosen · Choose in the player table");
  await expect(send(page)).toHaveText(/Resolve primary · nobody else/);
  await panel(page).getByRole("button", { name: "Rules: Trade" }).hover();
  await expect(page.getByText(/Gain 3 trade goods/).first()).toBeVisible();
  await page.mouse.move(0, 0);

  // Each row of the player table says what the choice gives that player.
  await expect(row("Seat B")).toContainText("Commodities 0 → 6");
  await expect(row("Seat C")).toContainText("Commodities 0 → 2");
  await row("Seat B").click();
  await row("Seat C").click();
  await expect(others).toContainText("2 / 2 chosen");
  await expect(others).toContainText("Commodities 0 → 6 · no strategy token");
  // "Change" gives a player back; Esc clears the last one.
  await others.getByRole("button", { name: "Change" }).first().click();
  await expect(others).toContainText("1 / 2 chosen");
  await page.keyboard.press("Escape");
  await expect(others).toContainText("No player chosen");
  await row("Seat C").click();
  await expect(page.locator("footer")).toContainText("replenishes commodities");
  await expect(send(page)).toHaveText(/^Resolve primary/);
  await expect(send(page)).not.toHaveText(/nobody/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/trade.png", animations: "disabled" });

  // The main button answers Letnev and then nobody more; Hacan is not replenished.
  await send(page).click();
  await expect(heading(page)).toHaveText("End your turn");
  await expect(row("Seat C")).toContainText("2/2");
  await expect(row("Seat B")).toContainText("0/6");
  expect(errors).toEqual([]);
});

test("Trade: the secondary replenishes or passes in the row of the seat", async ({ page }) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol and Hacan are played here.
  await page.goto("/?scenario=strategy_trade&humans=3");
  await playStrategyCard(page, "5 Trade");
  await expect(send(page)).toHaveText(/Resolve primary · nobody else/);
  await send(page).click();

  // Hacan's row opens into Follow and Pass, with the commodities and what it costs: nothing, by
  // Hacan's Masters of Trade.
  const hacan = seatRow(page, "Seat B");
  await expect(panel(page).getByRole("button", { name: /Replenish commodities/ })).toBeVisible();
  await expect(panel(page)).toContainText("Commodities 0 → 6 · no strategy token");
  await expect(send(page)).toBeDisabled();
  await page.keyboard.press("2");
  await expect(send(page)).toHaveText(/^Pass/);
  await page.keyboard.press("1");
  await expect(send(page)).toHaveText(/Follow · replenish commodities/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/trade-secondary.png", animations: "disabled" });
  await send(page).click();
  await expect(hacan).toContainText("6/6");
  await expect(hacan).toContainText("3 · 3 · 2");
  expect(errors).toEqual([]);
});

test("Imperial: an objective from the list, then the secret objective, sent at once", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol holds Imperial with 2 victory points, can score Lead From the Front and Negotiate Trade
  // Routes, and does not control Mecatol Rex.
  await page.goto("/?scenario=strategy_imperial&humans=1");
  const sol = seatRow(page, "Seat A");
  await playStrategyCard(page, "8 Imperial");

  // One screen: the objectives that can be scored, as printed, and what follows.
  await expect(heading(page)).toHaveText("Imperial");
  const objectives = panel(page)
    .getByRole("heading", { name: "Public objectives" })
    .locator("../..");
  const then = panel(page).getByRole("heading", { name: "Then" }).locator("../..");
  await expect(objectives).toContainText("Lead From the Front");
  await expect(objectives).toContainText("Spend 5 trade goods.");
  await expect(objectives).toContainText("Victory points 2 → 3");
  await expect(objectives).not.toContainText("Corner the Market");
  await expect(then).toContainText(
    "Draw 1 secret objectiveYou do not control Mecatol Rex · Secret objectives 1 → 2 of 3",
  );
  await expect(send(page)).toHaveText(/Resolve primary · score nothing/);
  await panel(page).getByRole("button", { name: "Rules: Imperial" }).hover();
  await expect(page.getByText(/Immediately score 1 public objective/).first()).toBeVisible();
  await page.mouse.move(0, 0);

  // A key chooses an objective; the same key again, or Esc, takes it back.
  await page.keyboard.press("2");
  await expect(page.locator("footer")).toContainText(
    "Score Negotiate Trade Routes, then draw 1 secret objective",
  );
  await page.keyboard.press("2");
  await expect(send(page)).toHaveText(/score nothing/);
  await page.keyboard.press("1");
  await page.keyboard.press("Escape");
  await expect(send(page)).toHaveText(/score nothing/);
  await page.keyboard.press("1");
  await expect(send(page)).toHaveText(/^Resolve primary↵?$/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/imperial.png", animations: "disabled" });

  // Lead From the Front is scored and a secret objective drawn.
  await send(page).click();
  await expect(heading(page)).toHaveText("End your turn");
  await expect(sol.getByRole("cell").nth(1)).toHaveText("3");
  await expect(sol).toContainText("0/2");
  expect(errors).toEqual([]);
});

test("Imperial with Mecatol Rex: the victory point follows the objective", async ({ page }) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol controls Mecatol Rex and has 3 victory points.
  await page.goto("/?scenario=strategy_imperial_mecatol&humans=1");
  await playStrategyCard(page, "8 Imperial");
  const then = panel(page).getByRole("heading", { name: "Then" }).locator("../..");
  await expect(then).toContainText(
    "Gain 1 victory pointYou control Mecatol Rex · Victory points 3 → 4",
  );
  await page.keyboard.press("1");
  await expect(then).toContainText("Victory points 4 → 5");
  await expect(page.locator("footer")).toContainText(
    "Score Lead From the Front, then gain 1 victory point",
  );
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/imperial-mecatol.png", animations: "disabled" });
  await send(page).click();
  await expect(heading(page)).toHaveText("End your turn");
  const sol = seatRow(page, "Seat A");
  await expect(sol.getByRole("cell").nth(1)).toHaveText("5");
  expect(errors).toEqual([]);
});

test("Imperial: the secondary draws a secret objective or passes in the row of the seat", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol and Hacan are played here.
  await page.goto("/?scenario=strategy_imperial&humans=3");
  await playStrategyCard(page, "8 Imperial");
  await send(page).click();

  // Hacan's row opens into Follow and Pass, with the secret objectives and what it costs.
  const hacan = seatRow(page, "Seat B");
  await expect(panel(page).getByRole("button", { name: /Draw 1 secret objective/ })).toBeVisible();
  await expect(panel(page)).toContainText("Secret objectives 1 → 2 of 3 · Strategy pool 2 → 1");
  await expect(send(page)).toBeDisabled();
  await page.keyboard.press("2");
  await expect(send(page)).toHaveText(/^Pass/);
  await page.keyboard.press("1");
  await expect(send(page)).toHaveText(/Follow · draw 1 secret objective/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/imperial-secondary.png", animations: "disabled" });
  await send(page).click();
  await expect(hacan).toContainText("0/2");
  await expect(hacan).toContainText("3 · 3 · 1");
  expect(errors).toEqual([]);
});

test("Technology: the second research of the primary is paid on the map and sent with it", async ({
  page,
}) => {
  // The load of the scenario, two researches and the payment.
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol holds Technology with 4 trade goods, Jord and the ready planets of a system next to it.
  await page.goto("/?scenario=strategy_technology&humans=1");
  await expect(heading(page)).toHaveText("Choose an action");
  await page.keyboard.press("7");
  await page.keyboard.press("Enter");
  // The printed card opens on hover and stays on click.
  const hint = panel(page).getByRole("button", { name: "Rules: Technology" });
  await expect(page.getByRole("note")).toHaveCount(0);
  await hint.hover();
  await expect(page.getByRole("note")).toContainText("Research 1 technology");
  await hint.click();
  await page.mouse.move(10, 10);
  await expect(page.getByRole("note")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.mouse.click(10, 10);
  // The first research is free: no payment.
  const tree = panel(page).getByRole("heading", { name: "Research", exact: true }).locator("..");
  await expect(tree).toContainText("Free");
  await panel(page).getByRole("button", { name: "Gravity Drive", exact: true }).click();
  const bar = page.getByRole("group", { name: "Payment" });
  await expect(bar).toHaveCount(0);
  await page.keyboard.press("Enter");

  // The second costs six resources: a chosen technology opens the payment on the map.
  await expect(tree).toContainText("Costs 6 resources");
  await panel(page).getByRole("button", { name: "Sarween Tools", exact: true }).click();
  await expect(tree).toContainText("Sarween Tools · 6 resources");
  await expect(bar.getByRole("status").first()).toHaveText(/0 \/ 6\s*resources\s*6 more/);
  await expect(page.locator("footer")).toContainText("Pay 6 more resources.");
  await expect(send(page)).toBeDisabled();
  await mapPlanet(page, "Jord").click();
  await bar.getByRole("button", { name: "Pay rest" }).click();
  await expect(bar.getByRole("status").first()).toHaveText(/6 \/ 6\s*resources\s*✓ Paid/);
  await expect(page.locator("footer")).toContainText(
    /Selected: Sarween Tools · Technologies \d+ → \d+ · Trade goods 4 → 2/,
  );
  await expect(send(page)).toBeEnabled();
  // More than the price is allowed, and says what is wasted.
  await bar.getByRole("button", { name: "Add trade good" }).click();
  await expect(bar.getByRole("status").first()).toHaveText(/7 \/ 6\s*resources\s*⚠ 1 wasted/);
  await expect(send(page)).toBeEnabled();
  await bar.getByRole("button", { name: "Remove trade good" }).click();
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/technology-pay.png", animations: "disabled" });

  // One button sends the research and its payment; the game asks nothing more of it.
  await send(page).click();
  await expect(heading(page)).toHaveText("End your turn");
  await expect(page.locator("g.planet.exhausted").filter({ hasText: "Jord" })).toHaveCount(1);
  expect(errors).toEqual([]);
});

test("Technology: the secondary researches in the row of the seat and pays on the map", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol and Hacan are played here: Sol researches, then Hacan follows for 4 resources.
  await page.goto("/?scenario=strategy_technology&humans=3");
  await expect(heading(page)).toHaveText("Choose an action");
  await page.keyboard.press("7");
  await page.keyboard.press("Enter");
  await panel(page).getByRole("button", { name: "Gravity Drive", exact: true }).click();
  await page.keyboard.press("Enter");
  await expect(send(page)).toHaveText(/Research nothing/);
  await page.keyboard.press("Enter");

  // One list shows what every seat did: Sol's primary is resolved, Hacan decides, Letnev waits.
  await expect(panel(page)).toContainText("✓ Resolved");
  await expect(panel(page)).toContainText("▶ Your decision");
  await expect(panel(page)).toContainText("· Waits for you");
  // Hacan's window is a list; following opens the tree in its row.
  await expect(panel(page)).toContainText("Strategy pool 2 → 1");
  await panel(page).getByRole("button", { name: "Spend", exact: true }).click();
  await send(page).click();
  const tree = panel(page).getByRole("heading", { name: "Research", exact: true }).locator("..");
  await expect(tree).toContainText("Costs 4 resources");
  await panel(page).getByRole("button", { name: "Gravity Drive", exact: true }).click();
  const bar = page.getByRole("group", { name: "Payment" });
  await expect(bar.getByRole("status").first()).toHaveText(/0 \/ 4\s*resources\s*4 more/);
  await bar.getByRole("button", { name: "Auto-pay" }).click();
  await expect(bar.getByRole("status").first()).toContainText("✓ Paid");
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/technology-secondary.png", animations: "disabled" });
  await send(page).click();
  await expect(tree).toHaveCount(0);
  // The strategic action is over: Sol ends its turn.
  await expect(heading(page)).toHaveText("End your turn");
  expect(errors).toEqual([]);
});

test("the Dev sheet leads to a game on the server, on the seed of the page", async ({ page }) => {
  await page.goto("/?local=3&players=8&humans=1");
  await expect(send(page)).toBeDisabled();
  await openSettings(page);
  const row = settings(page).getByRole("region", { name: "Multiplayer" });
  await expect(row.getByRole("button", { name: "Multiplayer seed 3 · 8 seats" })).toBeVisible();
  await row.getByRole("button", { name: "About Multiplayer" }).click();
  await expect(page.getByText("A new game of eight on the server")).toBeVisible();
  await page.screenshot({ path: "shots/local/dev-multiplayer.png", animations: "disabled" });
  await page.keyboard.press("Escape");
  const asked = page.waitForRequest(
    (request) => request.method() === "POST" && new URL(request.url()).pathname === "/api/games",
  );
  await row.getByRole("button", { name: /^Multiplayer/ }).click();
  expect((await asked).postDataJSON()).toMatchObject({ seed: 3, fill_seats: "random" });
  // The server made the game, and the page is at its address, as its player, with the way to
  // the start screen in the header.
  await expect(page).toHaveURL(/\?online=[^&]+&seat=[^&]+$/);
  await expect(page.getByText("Round 1 · Strategy phase")).toBeVisible();
  await expect(page.getByRole("button", { name: "Start", exact: true })).toBeVisible();
});

test("the turn menu is the action picker: a key for each group, the lists of cards, and Esc back", async ({
  page,
}) => {
  const errors = pageErrors(page, { console: true });
  await page.goto("/?scenario=late-game&humans=1");
  const row = (name: string | RegExp) => panel(page).getByRole("button", { name });
  // The fixed groups in their fixed order; a row that cannot be used says why.
  await expect(heading(page)).toHaveText("Choose an action");
  await expect(
    panel(page)
      .locator("button[aria-keyshortcuts]")
      .evaluateAll((all) => all.map((button) => button.getAttribute("aria-keyshortcuts"))),
  ).resolves.toEqual(["t", "1", "a", "c", "r", "p"]);
  await expect(panel(page).getByText("7 tactic tokens")).toBeVisible();
  await expect(row(/^Take no more actions in this round$/)).toBeDisabled();
  await expect(panel(page).getByText("Use 1 Leadership first")).toBeVisible();
  // The printed primary of the strategy card is behind "?".
  await row("Rules: 1 Leadership").hover();
  await expect(page.getByRole("note")).toContainText("command tokens");
  await page.mouse.move(10, 10);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/picker.png", animations: "disabled" });

  // A: the action cards that can be played now, each with its printed text.
  await page.keyboard.press("a");
  await expect(heading(page)).toHaveText("Play an action card");
  await expect(
    panel(page).getByText("Choose 1 planet. Destroy each PDS on that planet."),
  ).toBeVisible();
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/picker-cards.png", animations: "disabled" });
  await page.keyboard.press("Escape");
  await expect(heading(page)).toHaveText("Choose an action");

  // C: the other components. A key stages one, as printed; nothing is sent.
  await page.keyboard.press("c");
  await expect(heading(page)).toHaveText("Use a component");
  await page.keyboard.press("3");
  await expect(heading(page)).toHaveText("Orbital Drop");
  await expect(page.getByText("Not sent")).toBeVisible();
  await expect(panel(page).getByText(/^Spend 1 token from your strategy pool/)).toBeVisible();
  await expect(send(page)).toHaveText(/Use Orbital Drop/);
  await page.screenshot({ path: "shots/local/picker-staged.png", animations: "disabled" });
  // Esc goes one level back at a time: the list, then the picker.
  await page.keyboard.press("Escape");
  await expect(heading(page)).toHaveText("Use a component");
  await page.keyboard.press("Escape");
  await expect(heading(page)).toHaveText("Choose an action");

  // R: who can trade now, and why another seat cannot.
  await page.keyboard.press("r");
  await expect(heading(page)).toHaveText("Transaction");
  await expect(row(/^Hacan$/)).toBeEnabled();
  await expect(row(/^Xxcha$/)).toBeDisabled();
  await expect(panel(page).getByText("No contact: not neighbours").first()).toBeVisible();
  await fitsHeight(page);
  await page.keyboard.press("Escape");

  // A strategy card has its number as key. Its action is staged, and the main button sends it.
  await page.keyboard.press("1");
  await expect(heading(page)).toHaveText("1 Leadership");
  await expect(send(page)).toHaveText(/Use Leadership/);
  await page.keyboard.press("Escape");
  await expect(heading(page)).toHaveText("Choose an action");
  await expect.poll(() => savedLine(page)).toContain("0 answers saved");
  expect(errors).toEqual([]);
});

test("a shortcut does nothing in a text field or under an open rule text or menu, and a row that is not allowed has none", async ({
  page,
}) => {
  await page.goto("/?scenario=late-game&humans=1");
  const picker = panel(page).getByRole("heading", { name: "Choose an action" });
  const activation = panel(page).getByRole("heading", { name: "Activation", exact: true });
  await expect(picker).toBeVisible();
  // A row that is not allowed has no key.
  await page.keyboard.press("p");
  await expect(picker).toBeVisible();
  // A pinned rule text takes the keys. Esc closes it and nothing else.
  await panel(page)
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

test("the inspector activates a system: the tactical action opens as the draft", async ({
  page,
}) => {
  await page.goto("/?scenario=late-game&humans=1");
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const workspace = (name: string) =>
    page.getByRole("navigation", { name: "Workspace" }).getByRole("button", { name });
  await expect(heading(page)).toHaveText("Choose an action");
  // A system with the command token of the seat says so, with no button.
  await system("70").click();
  await expect(page.getByText("You cannot activate this system again.")).toBeVisible();
  await system("72").click();
  await page.getByRole("button", { name: "Activate this system" }).click();
  await expect(workspace("Draft")).toHaveAttribute("aria-pressed", "true");
  await expect(heading(page)).toHaveText("Activation");
  await expect(panel(page).getByText("Production 13")).toBeVisible();
  await expect(send(page)).toHaveText(/Activate system/);
  // Esc clears the system, then goes back to the actions. The draft stays.
  await page.keyboard.press("Escape");
  await expect(panel(page).getByText("Production 13")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(workspace("Live")).toHaveAttribute("aria-pressed", "true");
  await expect(heading(page)).toHaveText("Choose an action");
});

test("a component action: the planet of Orbital Drop is chosen on the map, then the turn ends", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  await page.goto("/?scenario=late-game&humans=1");
  await expect(heading(page)).toHaveText("Choose an action");
  await page.keyboard.press("c");
  await page.keyboard.press("3");
  await page.keyboard.press("Enter");

  // The ability asks for a planet: the map is the choice, the panel has what is chosen.
  await expect(heading(page)).toHaveText("Orbital Drop");
  await expect(panel(page).getByText("Your decision · Choose a planet")).toBeVisible();
  await expect(panel(page).getByText("No planet chosen")).toBeVisible();
  expect(await page.locator("g.planet.payable").count()).toBeGreaterThan(3);
  // The printed ability is behind "?" of the heading.
  await panel(page).getByRole("button", { name: "Rules: Orbital Drop" }).hover();
  await expect(page.getByRole("note")).toContainText("place 2 infantry");
  await page.mouse.move(10, 10);
  const jord = page.getByRole("button", { name: /^Choose Jord,/ });
  await jord.click();
  await expect(panel(page).getByText("Jord · #01")).toBeVisible();
  await expect(panel(page).getByText("Selected", { exact: true })).toBeVisible();
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/component-planet.png", animations: "disabled" });
  // Esc clears the choice.
  await page.keyboard.press("Escape");
  await expect(panel(page).getByText("No planet chosen")).toBeVisible();
  await jord.click();
  await page.keyboard.press("Enter");

  // After the action: what uses no action, and End turn.
  await expect(heading(page)).toHaveText("End your turn");
  await expect(panel(page).getByText("Before you end your turn")).toBeVisible();
  await expect(send(page)).toHaveText(/End turn/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/turn-closing.png", animations: "disabled" });
  await page.keyboard.press("Enter");
  await expect(heading(page)).not.toHaveText("End your turn");
  await expect.poll(() => savedLine(page)).toContain("3 answers saved");
  expect(errors).toEqual([]);
});

test("Warfare: the token is recalled on the map and the pools are placed, sent at once", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol holds Warfare with 3 · 3 · 2 on its sheet and its command token on #29.
  await page.goto("/?scenario=strategy_warfare&humans=1");
  const seat = seatRow(page, "Seat A");
  await expect(heading(page)).toHaveText("Choose an action");
  await expect(seat).toContainText("3 · 3 · 2");
  await panel(page)
    .getByRole("button", { name: /^6 Warfare/ })
    .click();
  await page.keyboard.press("Enter");

  // One screen: the token on the map, then the pools with the gained token; no question of a pool.
  await expect(heading(page)).toHaveText("Warfare");
  await expect(page.getByText(/into which pool/i)).toHaveCount(0);
  const token = panel(page).getByRole("heading", { name: "Command token" }).locator("../..");
  const pools = panel(page).getByRole("heading", { name: "Command pools" }).locator("..");
  await expect(token).toContainText("No system chosen");
  await expect(pools).toContainText("8 / 9 placed");
  await expect(page.locator("footer")).toContainText("Choose a system on the map.");
  await expect(send(page)).toBeDisabled();
  await panel(page).getByRole("button", { name: "Rules: Warfare" }).hover();
  await expect(
    page.getByText(/Remove 1 of your command tokens from the game board/).first(),
  ).toBeVisible();
  await page.mouse.move(0, 0);

  // Only the system with Sol's token is a target.
  await expect(page.locator("g.system.target")).toHaveCount(1);
  await page.locator("g.system.target").filter({ hasText: "#29" }).click();
  await expect(token).toContainText(
    "The token returns to your reinforcements · you gain 1 command token",
  );
  await page.getByRole("button", { name: "Close system details" }).click();
  await expect(page.locator("footer")).toContainText("Place 1 more token.");
  await expect(send(page)).toBeDisabled();
  // The gained token, and one moved from the strategy pool to the fleet pool.
  await panel(page).getByRole("button", { name: "Add tactic pool token" }).click();
  await expect(pools).toContainText("9 / 9 placed");
  await panel(page).getByRole("button", { name: "Remove strategy pool token" }).click();
  await panel(page).getByRole("button", { name: "Add fleet pool token" }).click();
  await expect(page.locator("footer")).toContainText("Tactic 3 → 4 · Fleet 3 → 4 · Strategy 2 → 1");
  // Esc puts the pools back, then the system; "Change" gives the system back.
  await page.keyboard.press("Escape");
  await expect(pools).toContainText("8 / 9 placed");
  await expect(token).toContainText("#29");
  await token.getByRole("button", { name: "Change" }).click();
  await expect(token).toContainText("No system chosen");
  await page.locator("g.system.target").filter({ hasText: "#29" }).click();
  await page.getByRole("button", { name: "Close system details" }).click();
  await panel(page).getByRole("button", { name: "Add tactic pool token" }).click();
  await panel(page).getByRole("button", { name: "Remove strategy pool token" }).click();
  await panel(page).getByRole("button", { name: "Add fleet pool token" }).click();
  await expect(send(page)).toHaveText(/Resolve primary/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/warfare.png", animations: "disabled" });

  // The main button answers the recall, the gain and the redistribution.
  await send(page).click();
  await expect(heading(page)).toHaveText("End your turn");
  await expect(seat).toContainText("4 · 4 · 1");
  expect(errors).toEqual([]);
});

test("Warfare: the secondary produces in the home system, paid on the map", async ({ page }) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol and Hacan are played here.
  await page.goto("/?scenario=strategy_warfare&humans=3");
  await playStrategyCard(page, "6 Warfare");
  await page.locator("g.system.target").filter({ hasText: "#29" }).click();
  await page.getByRole("button", { name: "Close system details" }).click();
  await panel(page).getByRole("button", { name: "Add tactic pool token" }).click();
  await send(page).click();

  // Hacan's row opens into the window, with what it costs.
  const hacan = seatRow(page, "Seat B");
  await expect(panel(page)).toContainText("Strategy pool 2 → 1");
  await expect(panel(page).getByRole("button", { name: "Produce" })).toBeVisible();
  await page.screenshot({ path: "shots/local/warfare-window.png", animations: "disabled" });
  await panel(page).getByRole("button", { name: "Produce" }).click();
  await send(page).click();

  // The production of the home system, with no steps of a tactical action.
  await expect(page.getByRole("group", { name: /^Produce in / })).toBeVisible();
  await expect(heading(page)).toHaveText("Warfare");
  await expect(page.getByRole("tablist")).toHaveCount(0);
  const sheet = page.getByRole("group", { name: /^Produce in / });
  await panel(page).getByRole("button", { name: "Rules: Warfare" }).hover();
  await expect(
    page.getByText(/PRODUCTION ability of 1 of your space docks in your home/).first(),
  ).toBeVisible();
  await page.mouse.move(0, 0);
  await sheet
    .getByRole("group", { name: "Ships · Space area" })
    .getByRole("button", { name: "Add Fighter" })
    .click();
  await expect(send(page)).toHaveText(/Pay 1 resource/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/warfare-secondary.png", animations: "disabled" });
  await send(page).click();
  const bar = page.getByRole("group", { name: "Payment" });
  await bar.getByRole("button", { name: "Auto-pay" }).click();
  await expect(send(page)).toHaveText(/Produce units/);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/warfare-secondary-pay.png", animations: "disabled" });
  await send(page).click();
  await expect(page.getByRole("group", { name: /^Produce in / })).toHaveCount(0);
  await expect(hacan).toContainText("3 · 3 · 1");
  await expect(hacan).toContainText("2/3");
  expect(errors).toEqual([]);
});

test("Warfare of Thunder's Edge: the pools before, a tactical action in any system with no token, the pools after", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page);
  // Sol holds the Warfare of Thunder's Edge, with its command token on #29.
  await page.goto("/?scenario=strategy_warfare_te&humans=1");
  const seat = seatRow(page, "Seat A");
  await playStrategyCard(page, "6 Warfare");

  // The redistribution before the action is the pool editor.
  await expect(heading(page)).toHaveText("Warfare");
  await panel(page).getByRole("button", { name: "Rules: Warfare" }).hover();
  await expect(page.getByText(/Perform a tactical action in any system/).first()).toBeVisible();
  await page.mouse.move(0, 0);
  await expect(panel(page)).toContainText("before the tactical action");
  const pools = panel(page).getByRole("heading", { name: "Command pools" }).locator("..");
  await expect(pools).toContainText("8 / 8 placed");
  await panel(page).getByRole("button", { name: "Remove strategy pool token" }).click();
  await panel(page).getByRole("button", { name: "Add tactic pool token" }).click();
  await send(page).click();
  await expect(seat).toContainText("4 · 3 · 1");

  // The activation: any system, with no command token, also #29 with Sol's token.
  await expect(heading(page)).toHaveText("Activation");
  await expect(panel(page)).toContainText("No command token · Choose on the map");
  await expect(page.getByText("Tactical action").first()).toBeVisible();
  await expect(page.locator("aside").getByText(/^Warfare · /)).toBeVisible();
  await page.locator("g.system.target").filter({ hasText: "#29" }).click();
  await expect(page.locator("footer")).toContainText("Selected:");
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/warfare-te.png", animations: "disabled" });
  await send(page).click();

  // The tactical action goes on in Live; no token is placed.
  await expect(heading(page)).toHaveText("Movement");
  await expect(seat).toContainText("4 · 3 · 1");
  await send(page).click();
  // The redistribution after the action is the pool editor again.
  await expect(panel(page)).toContainText("after the tactical action");
  await expect(pools).toContainText("8 / 8 placed");
  await send(page).click();
  await expect(heading(page)).toHaveText("End your turn");
  expect(errors).toEqual([]);
});

test("Trade: in a late game only the seats below their commodity limit can be chosen", async ({
  page,
}) => {
  await page.goto("/?scenario=late-game&humans=16");
  const players = page.getByRole("region", { name: "Players" });
  await expect(heading(page)).toHaveText("Choose an action");
  await page.keyboard.press("5");
  await page.keyboard.press("Enter");
  // The engine names the seat of each answer: the player table is the choice.
  const others = panel(page).getByRole("heading", { name: "Other players" }).locator("../..");
  await expect(others).toContainText("Choose in the player table");
  await expect(others).toContainText("No player chosen");
  await expect(players.getByRole("button", { name: /^Choose Seat B, Hacan/ })).toBeEnabled();
  await expect(players.getByRole("button", { name: /^Choose Seat E,/ })).toBeDisabled();
  // A row says what the choice gives that seat.
  await expect(players.getByText("Commodities 0 → 6")).toBeVisible();
  await players.getByRole("button", { name: /^Choose Seat B, Hacan/ }).click();
  await expect(players.getByRole("button", { name: /^Choose Seat B, Hacan/ })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.locator("footer")).toContainText("Hacan replenishes commodities");
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/pick-player.png", animations: "disabled" });
  await page.keyboard.press("Enter");
  await expect(others).toHaveCount(0);
});

test("Warfare in a late game: the token is chosen on the map, with eight seats in view", async ({
  page,
}) => {
  await page.goto("/?scenario=late-game&humans=32");
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  // Seat F is asked other questions before its turn: each is answered with its first row.
  for (let count = 0; count < 8; count++) {
    await expect(send(page).or(panel(page).getByText("Choose an action"))).toBeVisible();
    if ((await heading(page).textContent()) === "Choose an action") {
      break;
    }
    const saved = await savedLine(page);
    await page.keyboard.press("1");
    await send(page).click();
    await expect.poll(() => savedLine(page)).not.toBe(saved);
  }
  await expect(heading(page)).toHaveText("Choose an action");
  await page.keyboard.press("6");
  await page.keyboard.press("Enter");
  await expect(panel(page).getByText("No system chosen")).toBeVisible();
  await expect(system("60")).toHaveClass(/\btarget\b/);
  await system("60").click();
  await expect(
    panel(page).getByRole("heading", { name: "Command token" }).locator("../.."),
  ).toContainText("#60");
  await expect(panel(page).getByRole("heading", { name: "Command pools" })).toBeVisible();
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/pick-system.png", animations: "disabled" });
});

test("a scenario is chosen in the Dev sheet and starts at the turn menu of its board", async ({
  page,
}) => {
  const errors = pageErrors(page, { console: true });
  await page.goto("/?local=3&players=8&humans=1");
  await expect(send(page)).toBeDisabled();
  await openSettings(page);
  const start = settings(page).getByRole("region", { name: "Start from" });
  await expect(start.getByRole("button", { name: /^Round 1/ })).toHaveAttribute(
    "aria-current",
    "true",
  );
  await start.getByRole("button", { name: /^Late game$/ }).click();
  await expect(page).toHaveURL(/\?scenario=late-game&humans=1$/);

  // Round 4, and seat A has the turn: no strategy phase is played first.
  const row = (name: string | RegExp) => panel(page).getByRole("button", { name });
  await expect(row(/^Activate a system$/)).toBeVisible();
  await openSettings(page);
  await expect(
    settings(page).getByText(
      "Local game · scenario late-game · seed 42 · 8 seats · 0 answers saved",
    ),
  ).toBeVisible();
  await expect(start.getByRole("button", { name: /^Late game ✓ Selected$/ })).toHaveAttribute(
    "aria-current",
    "true",
  );
  await page.screenshot({ path: "shots/local/scenario-dev.png", animations: "disabled" });
  await page.keyboard.press("Escape");

  // The draft of the turn: many systems are in reach, and the fleet in the gravity rift rolls.
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  await page
    .getByRole("navigation", { name: "Workspace" })
    .getByRole("button", { name: "Draft" })
    .click();
  await expect(heading(page)).toHaveText("Activation");
  expect(await page.locator("g.system.target:not(.plain)").count()).toBeGreaterThan(30);
  await expect(system("70")).not.toHaveClass(/target/);
  await page.screenshot({ path: "shots/local/scenario-activation.png", animations: "disabled" });
  await system("42").click();
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Movement");
  await system("41").click();
  const sheet = page.getByRole("group", { name: /^Move from / });
  await expect(sheet).toBeVisible();
  await sheet.getByRole("button", { name: "Advanced Carrier II 1 of 1" }).click();
  await expect(sheet.getByText("Rift roll")).toBeVisible();
  await page.screenshot({ path: "shots/local/scenario-movement.png", animations: "disabled" });

  // A reload goes on in the scenario, with the draft.
  await page.reload();
  await expect(row(/^Open your draft$/)).toBeVisible();
  await page
    .getByRole("navigation", { name: "Workspace" })
    .getByRole("button", { name: "Draft" })
    .click();
  await expect(heading(page)).toHaveText("Movement");
  expect(errors).toEqual([]);
});

test("the Dev sheet plays another seat, several times, and undo stays with the seat", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  await page.goto("/?local=3&players=8&humans=1");
  await expect(send(page)).toBeDisabled();
  const seats = settings(page).getByRole("region", { name: "Play as" });
  const seat = (letter: string) => seats.getByRole("button", { name: `Seat ${letter}` });
  /** The seat that the page is asked as: in the strategy phase it is the active player. */
  const asked = page
    .getByRole("region", { name: "Players" })
    .locator('tbody tr:has([title="Active player"]) th');
  const playAs = async (letter: string) => {
    await openSettings(page);
    await seat(letter).click();
    await expect(settings(page)).toBeHidden();
  };
  /** The rows of the seats that are played here. */
  const playing = async () => {
    await openSettings(page);
    const rows = await seats.locator('button[aria-current="true"]').allTextContents();
    await page.keyboard.press("Escape");
    await expect(settings(page)).toBeHidden();
    return rows.join();
  };

  await openSettings(page);
  await expect(seats.getByRole("button", { name: /^Seat / })).toHaveCount(8);
  await expect(seat("A")).toBeDisabled();
  await expect(seat("A")).toContainText("✓ Playing");
  await expect(seat("C")).toContainText("Random");
  await page.screenshot({ path: "shots/local/dev-play-as.png", animations: "disabled" });
  await page.keyboard.press("Escape");

  // Seat A chooses its strategy card at random, and seat C is asked for its own.
  await playAs("C");
  await expect(asked.first()).toContainText("Seat C");
  await expect(send(page)).toBeDisabled();
  expect(await playing()).toMatch(/^Seat C.*✓ Playing$/);
  expect(await savedLine(page)).toContain("0 answers saved");
  // Nothing of seat C can be taken back yet, and the choice of seat A stays.
  await expect(undo(page)).toHaveCount(0);
  const first = await heading(page).textContent();
  await answer(page);
  await undo(page).click();
  await expect.poll(() => savedLine(page)).toContain("0 answers saved");
  await expect(heading(page)).toHaveText(first ?? "");
  await expect(undo(page)).toHaveCount(0);
  expect(await playing()).toMatch(/^Seat C/);
  await answer(page);

  // On to seat E, and a reload plays the game again with both changes.
  await playAs("E");
  await expect.poll(playing).toMatch(/^Seat E/);
  await questionOpen(page);
  const open = await heading(page).textContent();
  await page.reload();
  await questionOpen(page);
  await expect(heading(page)).toHaveText(open ?? "");
  expect(await playing()).toMatch(/^Seat E/);
  expect(await savedLine(page)).toContain("1 answers saved");
  await expect(undo(page)).toHaveCount(0);

  // Back to the seat of the start, which was a random seat in between.
  await playAs("A");
  await expect.poll(playing).toMatch(/^Seat A/);
  await answer(page);
  await expect(undo(page)).toBeVisible();
});

test("an invasion: the landings are staged on the map and sent as one plan", async ({ page }) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  await page.goto("/?scenario=late-game&humans=1");
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  const board = page.locator("section[aria-label='Galaxy board']");
  const decks = page.getByRole("complementary", { name: "Exploration" });
  // The inspector says what the exploration left on a planet: its type and what is attached.
  await system("74").click();
  const mapView = (name: string) =>
    page.getByRole("group", { name: "Map view" }).getByRole("button", { name });
  const depot = board
    .locator("> div")
    .filter({ has: page.getByRole("button", { name: "Close system details" }) });
  await expect(depot.getByText("Dyson Sphere", { exact: true })).toBeVisible();
  await expect(depot.getByText("+2 / +1")).toBeVisible();
  await expect(depot.getByText("Unexplored", { exact: true })).toHaveCount(0);
  await expect(depot.getByRole("button", { name: /^Cultural planet Vega Major/ })).toBeVisible();
  await page.screenshot({ path: "shots/local/inspector-attachment.png", animations: "disabled" });
  await page.getByRole("button", { name: "Close system details" }).click();
  // The carrier of the north takes two infantry and a mech to a system with two free planets.
  await activateInLive(page, system("38"));
  await expect(heading(page)).toHaveText("Movement");
  await system("63").click();
  const fleet = page.getByRole("group", { name: /^Move from / });
  await fleet.getByRole("button", { name: "Advanced Carrier II 1 of 1" }).click();
  await fleet.getByRole("button", { name: /^Load Spec Ops II, Planet Vorhal, 2 left/ }).click();
  await fleet
    .getByRole("button", { name: "Load Spec Ops II on Advanced Carrier II 1 of 1" })
    .first()
    .click();
  await fleet
    .getByRole("button", { name: "Load Spec Ops II on Advanced Carrier II 1 of 1" })
    .first()
    .click();
  await fleet
    .getByRole("button", { name: /^Load ZS Thunderbolt M2, Planet Vorhal, 1 left/ })
    .click();
  await fleet
    .getByRole("button", { name: "Load ZS Thunderbolt M2 on Advanced Carrier II 1 of 1" })
    .first()
    .click();
  await expect(fleet.getByText("1 ship · 3 cargo")).toBeVisible();
  await send(page).click();

  // Invasion: the landings open at the active system, and the panel has no controls.
  await expect(heading(page)).toHaveText("Invasion");
  await expect(send(page)).toHaveText(/Land nothing/);
  const sheet = page.getByRole("group", { name: /^Land in / });
  await expect(sheet).toBeVisible();
  await expect(
    sheet.getByRole("button", { name: "Land Spec Ops II, Space area, 2 left" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(
    sheet.getByRole("button", { name: "Land ZS Thunderbolt M2, Space area, 1 left" }),
  ).toBeVisible();
  await expect(
    panel(page).getByText("2 Spec Ops II, 1 ZS Thunderbolt M2 · not landed"),
  ).toBeVisible();
  await expect(panel(page).getByRole("spinbutton")).toHaveCount(0);
  await expect(page.locator("g.planet.payable")).toHaveCount(2);
  await page.screenshot({ path: "shots/local/invasion-open.png", animations: "disabled" });
  const saved = await savedLine(page);

  // Nobody has these planets: a landing explores them. The type of a planet opens its deck.
  await expect(sheet.getByText("Unexplored", { exact: true })).toHaveCount(2);
  await sheet.getByRole("button", { name: /^Hazardous planet Abyz/ }).click();
  await expect(decks).toBeVisible();
  const hazardous = decks.getByRole("region", { name: "Hazardous deck" });
  await expect(hazardous).toBeInViewport();
  await expect(hazardous.getByText("19 of 20 cards left")).toBeVisible();
  // A card has its printed text, how many are left, and who drew one.
  await expect(hazardous.getByText("Mining World")).toBeVisible();
  await expect(
    hazardous.getByText("This planet's resource value is increased by 2."),
  ).toBeVisible();
  await expect(hazardous.getByText("Drawn · Round 2 · Sol · Mehar Xull")).toBeVisible();
  await expect(hazardous.getByText(/^Relic fragments/)).toContainText("Sol 1");
  await page.screenshot({ path: "shots/local/exploration-deck.png", animations: "disabled" });
  await page.getByRole("button", { name: "Close Exploration" }).click();
  await expect(sheet).toBeVisible();
  // The map view of the exploration: the type of each planet, "?" while it is still explored.
  await mapView("Exploration").click();
  // (A planet of the open choice keeps its values in every map view.)
  await expect(mapPlanet(page, "Siig")).toContainText("H?");
  await expect(mapPlanet(page, "Vega Major")).toContainText("C+");
  await page.screenshot({ path: "shots/local/exploration-map.png", animations: "disabled" });
  await mapView("Standard").click();

  // A slot of the sheet lands the chosen kind; a tap on a planet of the map lands one more.
  await sheet.getByRole("button", { name: "Land Spec Ops II on Abyz" }).click();
  await mapPlanet(page, "Abyz").click();
  await sheet.getByRole("button", { name: "Land ZS Thunderbolt M2, Space area, 1 left" }).click();
  await sheet.getByRole("button", { name: "Land ZS Thunderbolt M2 on Fria" }).click();
  await expect(sheet.getByText("3 units land")).toBeVisible();
  await expect(panel(page).getByRole("button", { name: /^Abyz 2 Spec Ops II$/ })).toBeVisible();
  await expect(
    panel(page).getByRole("button", { name: /^Fria 1 ZS Thunderbolt M2$/ }),
  ).toBeVisible();
  await expect(panel(page).getByText("Every ground force lands")).toBeVisible();
  await expect(page.locator("g.planet.chosen")).toHaveCount(2);
  await expect(send(page)).toHaveText(/Commit ground forces/);
  await page.screenshot({ path: "shots/local/invasion.png", animations: "disabled" });
  // A token takes its unit back, and Reset everything.
  await sheet.getByRole("button", { name: "Take back Spec Ops II from Abyz" }).first().click();
  await expect(
    sheet.getByRole("button", { name: "Land Spec Ops II, Space area, 1 left" }),
  ).toBeVisible();
  await sheet.getByRole("button", { name: "Land Spec Ops II, Space area, 1 left" }).click();
  await sheet.getByRole("button", { name: "Land Spec Ops II on Fria" }).click();
  await expect(
    panel(page).getByRole("button", { name: /^Fria 1 Spec Ops II, 1 ZS Thunderbolt M2$/ }),
  ).toBeVisible();
  // Nothing was sent yet, and a reload keeps what is staged.
  expect(await savedLine(page)).toBe(saved);
  await page.reload();
  await expect(send(page)).toHaveText(/Commit ground forces/);
  await expect(sheet.getByText("3 units land")).toBeVisible();

  // The main button sends every landing, and the game goes on: the planets are Sol's.
  await send(page).click();
  await expect(heading(page)).not.toHaveText("Invasion");
  await expect(sheet).toBeHidden();
  await system("38").click();
  const inspector = page
    .locator("section[aria-label='Galaxy board'] > div")
    .filter({ hasText: "Abyz" });
  await expect(inspector.getByText("1 Spec Ops II").first()).toBeVisible();
  await expect(inspector.getByText("1 ZS Thunderbolt M2 · 1 Spec Ops II")).toBeVisible();
  // Abyz was explored, and its card asks: the decision has the card on top, as printed. Fria
  // is explored after it.
  await expect(panel(page).getByText("Hazardous exploration · Abyz")).toBeVisible();
  await expect(panel(page).getByText(/If you have at least 1 mech on this planet/)).toBeVisible();
  await expect(inspector.getByRole("button", { name: /^Hazardous planet Abyz/ })).toBeVisible();
  await expect(inspector.getByText("Unexplored", { exact: true })).toHaveCount(1);
  await page.screenshot({ path: "shots/local/after-invasion.png", animations: "disabled" });
  await page.locator("header").getByRole("button", { name: "Exploration" }).click();
  await expect(decks.getByText("Drawn · Round 4 · Sol · Abyz")).toBeVisible();
  await expect(decks.getByText("18 of 20 cards left").first()).toBeVisible();
  await page.getByRole("button", { name: "Close Exploration" }).click();

  // One undo takes the landings back whole, and they are staged again.
  await undo(page).click();
  await expect(heading(page)).toHaveText("Invasion");
  await expect(send(page)).toHaveText(/Commit ground forces/);
  expect(await savedLine(page)).toBe(saved);
  await expect(
    panel(page).getByRole("button", { name: /^Fria 1 Spec Ops II, 1 ZS Thunderbolt M2$/ }),
  ).toBeVisible();
  // The check mark closes the sheet; the active system opens it again.
  await system("38").click();
  await expect(sheet).toBeVisible();
  await sheet.getByRole("button", { name: "Reset" }).click();
  await expect(send(page)).toHaveText(/Land nothing/);
  await sheet.getByRole("button", { name: "Done with the landings" }).click();
  await expect(sheet).toBeHidden();
  await panel(page)
    .getByRole("button", { name: /^Abyz Nothing lands$/ })
    .click();
  await expect(sheet).toBeVisible();
  expect(errors).toEqual([]);
});

test("the landings on a defended planet have the odds of the ground combat", async ({ page }) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  // Sol's carriers are in a system of two planets of Letnev, each with two infantry and a PDS.
  await page.goto("/?scenario=ongoing_invasion_coexistence&humans=1");
  // Blitz is offered at the start of the invasion: passed.
  await expect(panel(page).getByText("Blitz", { exact: true })).toBeVisible();
  await send(page).click();
  await expect(heading(page)).toHaveText("Invasion");
  const sheet = page.getByRole("group", { name: /^Land in / });
  await expect(sheet).toBeVisible();
  await expect(send(page)).toHaveText(/Land nothing/);
  // Nothing lands: no odds.
  const regions = sheet.getByRole("region", { name: "Ground combat" });
  // The odds of Qucen'n, the first planet.
  const odds = regions.first();
  await expect(regions).toHaveCount(0);

  await sheet.getByRole("button", { name: /^Land Infantry I on Qucen/ }).click();
  await expect(regions).toHaveCount(1);
  await expect(odds).toHaveAttribute("aria-busy", "false");
  await expect(odds).toContainText(/Ground combat\s*·\s*Letnev/);
  await expect(odds).toContainText(/Win \d+% · Draw \d+% · Loss \d+%/);
  // The survivors of each side by kind, of the units that fight.
  await expect(odds.getByRole("group", { name: "Survivors of Sol" })).toHaveText(
    /^\s*\d\.\d \/ 1\s*$/,
  );
  await expect(odds.getByRole("group", { name: "Survivors of Letnev" })).toHaveText(
    /^\s*\d\.\d \/ 2\s*$/,
  );
  const one = Number(/Win (\d+)%/.exec((await odds.textContent()) ?? "")![1]);
  // The panel has the first round of the combat, with the same number in the header of Sol.
  const table = panel(page).getByRole("region", { name: "Qucen'n · Ground combat · Round 1" });
  await expect(table).toHaveAttribute("aria-busy", "false");
  await expect(table.getByRole("region", { name: "Attacker" })).toContainText(
    new RegExp(`${one}% win · ~\\d\\.\\d of 1 survive`),
  );
  await expect(table.getByRole("region", { name: "Defender" })).toContainText(/Infantry I×28\+/);
  await expect(table).toContainText(/Avg \d\.\d rounds/);
  // More units that land: new odds, better than with one.
  await sheet.getByRole("button", { name: /^Land Infantry I on Qucen/ }).click();
  await sheet.getByRole("button", { name: /^Land Infantry I on Qucen/ }).click();
  await expect(odds).toHaveAttribute("aria-busy", "false");
  await expect
    .poll(async () => Number(/Win (\d+)%/.exec((await odds.textContent()) ?? "")?.[1] ?? 0))
    .toBeGreaterThan(one);
  // The other planet gets its own odds.
  await sheet.getByRole("button", { name: /^Land Infantry I on Rarron/ }).click();
  await expect(regions).toHaveCount(2);
  await odds.getByRole("button", { name: "About the odds" }).click();
  await expect(page.getByText(/^Simulated · 2000 rollouts/)).toBeVisible();
  await page.keyboard.press("Escape");
  const overflow = await page
    .locator("#step-panel > div.overflow-y-auto")
    .evaluate((element) => element.scrollHeight - element.clientHeight);
  expect(overflow, "the step content needs vertical scroll").toBeLessThanOrEqual(0);
  await page.screenshot({ path: "shots/local/invasion-odds.png", animations: "disabled" });
  // A unit taken back: the odds follow.
  await sheet.getByRole("button", { name: "Take back Infantry I from Rarron" }).click();
  await expect(regions).toHaveCount(1);
  expect(errors).toEqual([]);
});

test("a draft reaches the landings on a defended planet, with the first round of the ground combat", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  // Sol's carriers wait in a system of two planets of Letnev, each with two infantry; no PDS.
  await page.goto("/?scenario=invasion_draft&humans=1");
  const tab = (name: string) => page.getByRole("tab", { name: new RegExp(`^\\S* ?${name}`) });
  await expect(panel(page).getByRole("button", { name: /^Activate a system$/ })).toBeVisible();
  await page
    .getByRole("navigation", { name: "Workspace" })
    .getByRole("button", { name: "Draft" })
    .click();
  await expect(page.locator("g.system.target:not(.plain)")).not.toHaveCount(0);
  await page.locator("g.system").filter({ hasText: "#29" }).click();
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Movement");
  // The fleet is already there: nothing moves, and the game comes to the landings.
  await expect(send(page)).toHaveText(/Move nothing/);
  await send(page).click();
  await expect(heading(page)).toHaveText("Invasion");
  await expect(page.getByText("Private draft").first()).toBeVisible();
  const sheet = page.getByRole("group", { name: /^Land in / });
  await sheet.getByRole("button", { name: /^Land Infantry I on Qucen/ }).click();
  await sheet.getByRole("button", { name: /^Land Infantry I on Qucen/ }).click();
  const odds = sheet.getByRole("region", { name: "Ground combat" });
  await expect(odds).toHaveAttribute("aria-busy", "false");
  await expect(odds).toContainText(/Win \d+% · Draw \d+% · Loss \d+%/);
  // The panel has the first round of the combat, with the odds in the headers.
  const table = panel(page).getByRole("region", { name: "Qucen'n · Ground combat · Round 1" });
  await expect(table).toHaveAttribute("aria-busy", "false");
  await expect(table.getByRole("region", { name: "Attacker" })).toContainText(/Infantry I×28\+/);
  await expect(table.getByRole("region", { name: "Attacker" })).toContainText(
    /\d+% win · ~\d\.\d of 2 survive/,
  );
  await expect(tab("Invasion")).toBeVisible();
  const overflow = await page
    .locator("#step-panel > div.overflow-y-auto")
    .evaluate((element) => element.scrollHeight - element.clientHeight);
  expect(overflow, "the step content needs vertical scroll").toBeLessThanOrEqual(0);
  await page.screenshot({ path: "shots/local/draft-invasion-odds.png", animations: "disabled" });
  expect(errors).toEqual([]);
});

test("a production: the build is staged on the map, then paid on the map, and sent as one plan", async ({
  page,
}) => {
  // Longer than a test of this suite should be: see "Tests" in AGENTS.md.
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  await page.goto("/?scenario=late-game&humans=1");
  const row = (name: string | RegExp) => panel(page).getByRole("button", { name });
  const system = (id: string) => page.locator("g.system").filter({ hasText: `#${id}` });
  // The system with two space docks. The destroyer of the nebula moves there.
  await activateInLive(page, system("72"));
  await expect(heading(page)).toHaveText("Movement");
  await system("42").click();
  await page
    .getByRole("group", { name: /^Move from / })
    .getByRole("button", { name: "Destroyer II 1 of 1" })
    .click();
  await send(page).click();
  // AI Development Algorithm asks before the production: a decision in the list.
  await row(/^Decline/i).click();
  await send(page).click();

  // Production: the build opens at the system that produces, and the panel has no controls.
  await expect(heading(page)).toHaveText("Production");
  await expect(send(page)).toHaveText(/Produce nothing/);
  const sheet = page.getByRole("group", { name: /^Produce in / });
  await expect(sheet).toBeVisible();
  await expect(sheet.getByRole("status").first()).toHaveText("Nothing is produced");
  // Every kind of unit has its row, as printed. One that is not produced here says why.
  const ships = sheet.getByRole("group", { name: "Ships · Space area" });
  const forces = sheet.getByRole("group", { name: "Ground forces" });
  for (const name of [
    "Advanced Carrier II",
    "Cruiser II",
    "Destroyer II",
    "Dreadnought II",
    "Genesis",
    "War Sun",
  ]) {
    await expect(ships.getByText(name, { exact: true })).toBeVisible();
  }
  await expect(forces.getByText("ZS Thunderbolt M2", { exact: true })).toBeVisible();
  await expect(sheet.getByText("None left")).toHaveCount(3);
  // The printed values are behind the name: hover shows the card, a click keeps it, Esc closes.
  const card = page.getByRole("note");
  await ships.getByRole("button", { name: "War Sun, printed values" }).hover();
  await expect(card).toContainText("BOMBARDMENT 3+ (x3)");
  await expect(card).toContainText("lose PLANETARY SHIELD");
  await ships.getByRole("button", { name: "War Sun, printed values" }).click();
  await page.mouse.move(0, 0);
  await expect(card).toBeVisible();
  await page.screenshot({ path: "shots/local/production-card.png", animations: "disabled" });
  await page.keyboard.press("Escape");
  await expect(card).toHaveCount(0);
  expect(errors).toEqual([]);
  await expect(ships.getByRole("button", { name: "Add Advanced Carrier II" })).toHaveCount(0);
  await expect(ships.getByRole("button", { name: "Add War Sun" })).toBeEnabled();
  // The limits, with the resources that the seat has.
  for (const limit of [
    "Production capacity",
    "Resources",
    "Fleet supply after",
    "Fighters and cargo in space",
  ]) {
    await expect(sheet.getByText(limit, { exact: true })).toBeVisible();
  }
  const gauge = (label: string) =>
    sheet.getByText(label, { exact: true }).locator("xpath=following-sibling::strong");
  await expect(gauge("Resources")).toHaveText("0 / 26");
  const cargo = Number((await gauge("Fighters and cargo in space").innerText()).split("/")[1]);
  expect(await sheet.evaluate((node) => node.scrollHeight <= node.clientHeight)).toBe(true);
  await expect(panel(page).getByRole("spinbutton")).toHaveCount(0);
  // No planet pays while the build is chosen, and there is no payment bar.
  await expect(page.locator("g.planet.payable")).toHaveCount(0);
  await expect(page.getByRole("group", { name: "Payment" })).toHaveCount(0);
  await page.screenshot({ path: "shots/local/production-open.png", animations: "disabled" });
  const saved = await savedLine(page);

  // A counter buys in its place: two fighters, a cruiser, infantry on each planet.
  await ships.getByRole("button", { name: "Add Fighter II" }).click();
  // One purchase is one token, with the two fighters that it makes.
  await expect(ships.getByRole("button", { name: "Remove Fighter II" })).toHaveCount(1);
  await expect(ships.getByRole("button", { name: "Remove Fighter II" }).locator("svg")).toHaveCount(
    2,
  );
  await ships.getByRole("button", { name: "Add Cruiser II" }).click();
  await forces.getByRole("button", { name: "Add Spec Ops II, Planet Velnor" }).click();
  await forces.getByRole("button", { name: "Add Spec Ops II, Planet Lisis" }).click();
  await expect(sheet.getByRole("status").first()).toHaveText("7 units · 4 resources · discount 1");
  await expect(gauge("Resources")).toHaveText("4 / 26");
  await expect(row(/^Space area 1 Cruiser II, 2 Fighter II$/)).toBeVisible();
  await expect(row(/^Planet Velnor 2 Spec Ops II$/)).toBeVisible();
  // A staged ship adds its capacity to the limit of the cargo, and takes it back with it.
  await expect(gauge("Fighters and cargo in space")).toHaveText(new RegExp(`/ ${cargo + 1}$`));
  await ships.getByRole("button", { name: "Add War Sun" }).click();
  await expect(gauge("Fighters and cargo in space")).toHaveText(new RegExp(`/ ${cargo + 7}$`));
  // The supply has one war sun: the slot is gone.
  await expect(ships.getByRole("button", { name: "Add War Sun" })).toHaveCount(0);
  await ships.getByRole("button", { name: "Remove War Sun" }).click();
  await expect(gauge("Fighters and cargo in space")).toHaveText(new RegExp(`/ ${cargo + 1}$`));
  // The "−" of a counter takes a purchase back.
  await forces.getByRole("button", { name: "Remove Spec Ops II, Planet Lisis" }).click();
  await expect(sheet.getByRole("status").first()).toHaveText("5 units · 3 resources · discount 1");
  await expect(row(/^Planet Lisis Nothing is produced$/)).toBeVisible();
  await expect(send(page)).toHaveText(/Pay 3 resources/);
  await page.screenshot({ path: "shots/local/production-build.png", animations: "disabled" });

  // Then the payment, on the map: the sheet closes, the bar opens, the ready planets pay.
  await send(page).click();
  await expect(sheet).toBeHidden();
  const bar = page.getByRole("group", { name: "Payment" });
  await expect(bar.getByRole("status").first()).toHaveText(/Paying\s*0 \/ 3\s*resources\s*3 more/);
  await expect(send(page)).toHaveText(/Produce units/);
  await expect(send(page)).toBeDisabled();
  await expect(page.locator("g.planet.payable")).toHaveCount(8);
  await mapPlanet(page, "Lisis").click();
  await expect(bar.getByRole("status").first()).toHaveText(/2 \/ 3\s*resources\s*1 more/);
  await bar.getByRole("button", { name: "Pay rest" }).click();
  await expect(bar.getByRole("status").first()).toHaveText(/3 \/ 3\s*resources\s*✓ Paid/);
  await expect(page.locator("footer").getByText("Lisis 2 + 1 trade good")).toBeVisible();
  await expect(send(page)).toBeEnabled();
  await page.screenshot({ path: "shots/local/production-pay.png", animations: "disabled" });
  // Auto-pay finds the payment with the least waste: one planet of 3.
  await bar.getByRole("button", { name: "Clear payment" }).click();
  await expect(send(page)).toBeDisabled();
  await bar.getByRole("button", { name: "Auto-pay" }).click();
  await expect(bar.getByRole("status").first()).toHaveText(/3 \/ 3\s*resources\s*✓ Paid/);
  await expect(page.locator("g.planet.chosen")).toHaveCount(1);
  // Esc goes back to the build, with the payment still staged; the main button pays again.
  await page.keyboard.press("Escape");
  await expect(sheet).toBeVisible();
  await expect(bar).toHaveCount(0);
  await send(page).click();
  await bar.getByRole("button", { name: "Clear payment" }).click();
  await mapPlanet(page, "Lisis").click();
  await bar.getByRole("button", { name: "Pay rest" }).click();
  // Nothing was sent yet, and a reload keeps what is staged.
  expect(await savedLine(page)).toBe(saved);
  await page.reload();
  await expect(sheet.getByRole("status").first()).toHaveText("5 units · 3 resources · discount 1");
  await send(page).click();
  await expect(bar.getByRole("status").first()).toHaveText(/3 \/ 3\s*resources\s*✓ Paid/);

  // The main button sends the build and its payment, and the game goes on.
  await send(page).click();
  // The next decision is open before the map is clicked: its update would close the inspector.
  await expect(heading(page)).toHaveText("Place up to 2 infantry");
  await expect(bar).toHaveCount(0);
  await system("72").click();
  const inspector = page
    .locator("section[aria-label='Galaxy board'] > div")
    .filter({ hasText: "Velnor" });
  await expect(inspector.getByText(/2 Cruiser II/)).toBeVisible();
  await expect(inspector.getByText(/4 Fighter II/)).toBeVisible();
  await expect(inspector.getByText("Exhausted", { exact: true })).toHaveCount(1);
  await page.screenshot({ path: "shots/local/after-production.png", animations: "disabled" });

  // One undo takes the production back whole, and it is staged again.
  await undo(page).click();
  await expect(heading(page)).toHaveText("Production");
  expect(await savedLine(page)).toBe(saved);
  await expect(sheet.getByRole("status").first()).toHaveText("5 units · 3 resources · discount 1");
  await expect(send(page)).toHaveText(/Pay 3 resources/);
  // Reset takes the build back; the check mark closes the sheet, and the system opens it again.
  await sheet.getByRole("button", { name: "Reset" }).click();
  await expect(sheet.getByRole("status").first()).toHaveText("Nothing is produced");
  await sheet.getByRole("button", { name: "Done with the build" }).click();
  await expect(sheet).toBeHidden();
  await expect(send(page)).toHaveText(/Produce nothing/);
  await system("72").click();
  await expect(sheet).toBeVisible();
  expect(errors).toEqual([]);
});

test("the Secrets sheet of the late game has scored and unscored objectives of every seat", async ({
  page,
}) => {
  const errors = pageErrors(page);
  await page.goto("/?scenario=late-game&humans=1");
  await expect(heading(page)).toBeVisible();
  await page.getByRole("button", { name: "Secrets", exact: true }).click();
  const secrets = page.getByRole("complementary", { name: "Secrets" });
  // Seat A has three: the hand first, one of the action phase and one of the agenda phase.
  const mine = secrets.getByRole("region", { name: "Your secret objectives" });
  await expect(mine.getByText("1 scored · 3 / 3")).toBeVisible();
  await expect(mine.getByRole("article")).toHaveCount(3);
  await expect(mine.getByRole("article").nth(0)).toContainText("Not scored");
  await expect(mine.getByRole("article").nth(0)).toContainText("Action phase");
  await expect(mine.getByRole("article").nth(1)).toContainText("Agenda phase");
  await expect(mine.getByRole("article").nth(2)).toContainText("✓ Scored");
  await expect(mine.getByRole("article").nth(2)).toContainText("Status phase");
  // Twelve are scored at the table and two are in the hand of seat A.
  const deck = secrets.getByRole("region", { name: "Secret objective deck" });
  await expect(deck.getByText("26 of 40 unseen · 16 in the deck")).toBeVisible();
  // Every group is opened: a closed one is gone from this list when it is clicked.
  const closed = deck.getByRole("button", { expanded: false });
  for (let left = await closed.count(); left > 0; left--) {
    await closed.first().click();
  }
  await expect(deck.getByText("Unseen", { exact: true })).toHaveCount(26);
  await expect(deck.getByText("In your hand")).toHaveCount(2);
  await expect(deck.getByText("✓ Scored by you")).toHaveCount(1);
  await expect(deck.getByText(/^✓ Scored by(?! you)/)).toHaveCount(11);
  // The player table counts the same: scored / held.
  const row = page.getByRole("region", { name: "Players" }).locator("tbody tr");
  await expect(row.nth(0)).toContainText("1/2");
  await expect(row.nth(1)).toContainText("2/1");
  await page.screenshot({ path: "shots/local/secrets-late-game.png", animations: "disabled" });
  expect(errors).toEqual([]);
});

test("the Objectives sheet of the late game says where the points of every seat come from", async ({
  page,
}) => {
  const errors = pageErrors(page);
  await page.goto("/?scenario=late-game&humans=1");
  await expect(heading(page)).toBeVisible();
  await page.getByRole("button", { name: "Objectives", exact: true }).click();
  const sheet = page.getByRole("complementary", { name: "Objectives" });
  const seats = sheet.getByRole("group", { name: "Victory points" }).getByRole("button");
  await expect(seats).toHaveCount(8);
  // The viewer is first, then the seats by their points: Jol-Nar has 7, L1Z1X has 2.
  await expect(seats.nth(0)).toHaveAccessibleName(/\(you\): 5 points$/);
  await expect(seats.nth(1)).toHaveAccessibleName(/: 7 points$/);
  await expect(seats.nth(7)).toHaveAccessibleName(/: 2 points$/);
  const first = sheet
    .getByRole("region", { name: "Public objectives · stage I", exact: true })
    .getByRole("article");
  await expect(first).toHaveCount(5);
  await expect(
    sheet.getByRole("region", { name: "Public objectives · stage II" }).getByRole("article"),
  ).toHaveCount(2);
  // Of the other sources, the sheet has what is still to get or to lose: the notes and the
  // agendas. The custodians are taken, and what is done (Imperial) is in the sheet of a player.
  const other = sheet.getByRole("region", { name: "Other points" });
  await expect(other.getByRole("article")).toHaveCount(3);
  await expect(other.getByRole("article", { name: "Agenda · Mutiny" })).toContainText("−1");
  await expect(
    other.getByRole("article", { name: "Support for the Throne" }).getByRole("listitem").first(),
  ).toHaveAccessibleName(/: 1 point from /);
  // Nothing is cut at the side of the sheet.
  const scroller = sheet.locator("div.overflow-auto");
  expect(await scroller.evaluate((element) => element.scrollWidth - element.clientWidth)).toBe(0);
  await page.screenshot({ path: "shots/local/objectives-late-game.png", animations: "disabled" });
  // A seat is the filter: the sheet shows what it can still score. Sol scored one objective, of
  // stage II.
  await seats.nth(0).click();
  await expect(first).toHaveCount(5);
  await expect(
    sheet.getByRole("region", { name: "Public objectives · stage II" }).getByRole("article"),
  ).toHaveCount(1);
  await expect(sheet.getByText("1 of 2 open for Sol")).toBeVisible();
  await seats.nth(0).click();
  // The deck: the public objectives that the game can still reveal, in groups by what they ask
  // for. The late game has two of stage II on the table, so 18 of stage II are left and none of
  // stage I.
  const deck = sheet.getByRole("region", { name: "Public objective deck" });
  await expect(deck).toContainText("18 not revealed");
  await deck.getByRole("button", { name: /^Spend/ }).click();
  await expect(deck.getByRole("region", { name: "Spend" }).getByText("2 VP")).toHaveCount(5);
  await expect(deck.getByText("1 VP")).toHaveCount(0);
  // The standings stay in view when the sheet scrolls.
  await scroller.evaluate((element) => element.scrollTo(0, element.scrollHeight));
  await expect(seats.nth(0)).toBeInViewport();
  // The sheet of a player says why the seat has its points: Sol has 5.
  await sheet.getByRole("button", { name: "Close Objectives" }).click();
  await page.getByRole("button", { name: /^Seat A, Sol.*Open player sheet/ }).click();
  const points = page
    .getByRole("complementary", { name: "Seat A · Sol" })
    .getByRole("region", { name: "Victory points" });
  await expect(points).toContainText("5 of 10");
  // The printed text of an objective is behind its name, as the values of a unit.
  await points.getByRole("button", { name: "Patrol Vast Territories, printed text" }).click();
  await expect(
    points.getByText("Have units in 5 systems that do not contain planets."),
  ).toBeVisible();
  await expect(
    points.getByRole("listitem").filter({ hasText: "Imperial · Mecatol Rex" }),
  ).toHaveText(/2$/);
  expect(errors).toEqual([]);
});

test("the sheet of a player has its abilities, leaders and used cards as printed", async ({
  page,
}) => {
  await page.goto("/?scenario=late-game&humans=1");
  await page.getByRole("button", { name: /^Seat A, Sol.*Open player sheet/ }).click();
  const sheet = page.getByRole("complementary", { name: "Seat A · Sol" });
  const cards = (section: string) => sheet.getByRole("region", { name: section });
  const card = (name: string) => sheet.getByRole("article", { name });
  await expect(cards("Faction abilities").getByRole("article")).toHaveCount(2);
  await expect(card("Orbital Drop")).toContainText("ACTION: Spend 1 token from your strategy pool");
  // A leader says its state in a word, and a locked one how it is unlocked.
  await expect(cards("Leaders").getByRole("article")).toHaveCount(3);
  await expect(card("Evelyn DeLouis")).toContainText("Exhausted");
  await expect(card("Evelyn DeLouis")).toContainText("1 additional die");
  await expect(card("Claire Gibson")).toContainText("Ready");
  await expect(card("Jace X, 4th Air Legion")).toContainText("Locked");
  await expect(card("Jace X, 4th Air Legion")).toContainText("Unlock: Have 3 scored objectives.");
  // Only the cards that are used: Gravity Drive always holds, so it stays a line of the list.
  await expect(cards("Action components").getByRole("article")).toHaveCount(4);
  await expect(card("AI Development Algorithm")).toContainText("Ready");
  await expect(card("Bio-Stims")).toContainText("Exhausted");
  await expect(card("Dynamis Core")).toContainText("Ready");
  await expect(card("The Prophet's Tears")).toContainText("Exhausted");
  await expect(sheet.getByRole("article", { name: "Gravity Drive" })).toHaveCount(0);
  await expect(sheet.getByText("GD", { exact: true })).toBeVisible();
  await page.screenshot({ path: "shots/local/player-sheet.png", animations: "disabled" });
  // Another seat: its cards are public too, and nothing of its hand is here.
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: /^Seat B, .*Open player sheet/ }).click();
  const other = page.getByRole("complementary", { name: /^Seat B · / });
  await expect(other.getByRole("region", { name: "Leaders" }).getByRole("article")).toHaveCount(3);
});

test("a reaction window is on top of the action of another seat, has its cards as printed, and each has its setting", async ({
  page,
}) => {
  const errors = pageErrors(page);
  // Seat G is Sardakk: it holds Reverse Engineer, and the first seat of the round discards a card.
  await page.goto("/?scenario=late-game&humans=64");
  const window = panel(page).getByRole("group", { name: /^Reaction · After .+ discards .+$/ });
  await expect(window).toBeVisible();
  // The action of the other seat stays under the window: the card that it played, as printed.
  const played = /^Reaction · After .+ discards (.+)$/.exec(
    (await window.getAttribute("aria-label")) ?? "",
  )![1];
  await expect(heading(page)).toHaveText(played);
  await expect(panel(page).getByText("Action card", { exact: true }).first()).toBeVisible();
  // No id of the engine is on the screen.
  await expect(panel(page)).not.toContainText("ACTION_CARD_DISCARDED");
  const row = window.getByRole("button", { name: "Reverse Engineer" });
  await expect(row).toBeVisible();
  await expect(window).toContainText("Take that action card from the discard pile");
  // The decline is the main button, and no row.
  await expect(send(page)).toHaveText(/Pass/);
  await expect(panel(page).getByRole("button", { name: "Decline" })).toHaveCount(0);
  const setting = window.getByRole("combobox", { name: "Offer Reverse Engineer" });
  await expect(setting.getByRole("option")).toHaveText(["Offered", "Only on my turns", "Never"]);
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/reaction-window.png", animations: "disabled" });
  // A row stages its card: the main button plays it, Esc clears it.
  await page.keyboard.press("1");
  await expect(send(page)).toHaveText(/Play Reverse Engineer/);
  await expect(page.getByText("Not sent", { exact: true })).toBeVisible();
  await fitsHeight(page);
  await page.screenshot({ path: "shots/local/reaction-staged.png", animations: "disabled" });
  await page.keyboard.press("Escape");
  await expect(send(page)).toHaveText(/Pass/);

  // The setting is of the windows after this one: the window stays open, with what is set.
  await setting.selectOption("ownturn");
  await expect(page.getByRole("status").filter({ hasText: "Reverse Engineer" })).toContainText(
    "offered on your turns only",
  );
  await expect(window).toBeVisible();
  await expect(setting).toHaveValue("ownturn");
  await expect(send(page)).toHaveText(/Pass/);

  // The sheets are where it is set back: the Cards sheet for a card, the sheet of the seat for
  // the card of an ability.
  await page.getByRole("button", { name: "Cards", exact: true }).click();
  const cards = page.getByRole("complementary", { name: "Cards" });
  const kept = cards.getByRole("combobox", { name: "Offer Reverse Engineer" });
  await expect(kept).toHaveValue("ownturn");
  await kept.selectOption("always");
  await expect(kept).toHaveValue("always");
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: /^Seat G, .*Open player sheet/ }).click();
  const sheet = page.getByRole("complementary", { name: /^Seat G · / });
  const agent = sheet.getByRole("combobox", { name: "Offer T'ro" });
  await expect(agent).toHaveValue("always");
  await agent.selectOption("never");
  await expect(agent).toHaveValue("never");
  // The sheet of another seat has no setting.
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: /^Seat A, .*Open player sheet/ }).click();
  await expect(
    page.getByRole("complementary", { name: /^Seat A · / }).getByRole("combobox"),
  ).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("the Agendas sheet of the late game has the laws in play and the deck in groups", async ({
  page,
}) => {
  const errors = pageErrors(page);
  await page.goto("/?scenario=late-game&humans=1");
  await expect(heading(page)).toBeVisible();
  await page.getByRole("button", { name: "Agendas", exact: true }).click();
  const agendas = page.getByRole("complementary", { name: "Agendas" });
  // A law with an elected seat and a law that passed "For", each with the text of its outcome.
  const laws = agendas.getByRole("region", { name: "Laws in play" });
  await expect(laws.getByText("2 laws")).toBeVisible();
  await expect(laws.getByRole("article")).toHaveCount(2);
  const act = laws.getByRole("article", { name: "Homeland Defense Act" });
  await expect(act).toContainText("Law · For/Against");
  await expect(act).toContainText("For: Each player can have any number of PDS units");
  await expect(act).not.toContainText("Against:");
  const censure = laws.getByRole("article", { name: "Political Censure" });
  await expect(censure).toContainText("Law · Elect Player");
  await expect(censure).toContainText("The elected player gains this card and 1 victory point.");
  // The two laws and Mutiny are gone from the deck of fifty.
  const deck = agendas.getByRole("region", { name: "Agenda deck" });
  await expect(deck.getByText("48 not in play · 47 in the deck")).toBeVisible();
  await expect(deck.getByRole("heading", { level: 4 })).toHaveCount(8);
  await expect(deck.getByText("Minister of War")).toBeHidden();
  await deck.getByRole("button", { name: /^Ministers and offices/ }).click();
  await expect(deck.getByText("Minister of War")).toBeVisible();
  // A law in play is not listed again.
  await deck.getByRole("button", { name: /^Victory points and objectives/ }).click();
  await expect(deck.getByText("Mutiny", { exact: true })).toBeVisible();
  await expect(deck.getByText("Political Censure")).toHaveCount(0);
  await page.screenshot({ path: "shots/local/agendas-late-game.png", animations: "disabled" });
  expect(errors).toEqual([]);
});

// Single player: the setup of a table in the page (`?single`), on the screen of the lobby.
const lobbyPlayers = (page: Page) => page.getByRole("region", { name: "Players" });
const lobbyRows = (page: Page) => lobbyPlayers(page).locator("ul.overflow-y-auto > li");
const lobbySetup = (page: Page) => page.getByRole("region", { name: "Setup" });
const setupValue = (page: Page, label: string) =>
  lobbySetup(page).getByLabel(label, { exact: true });
const lobbyTitle = (page: Page) => page.locator("header strong").first();
const lobbyMain = (page: Page) => page.locator('button[aria-keyshortcuts="Enter"]');

test("single player: a table is set up in the page, started, kept and set up again", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  // With no game of this browser, "Single player" of the start screen is the setup.
  await page.goto("/");
  await expect(page.getByRole("link", { name: /Single player/ })).toHaveAttribute(
    "href",
    "/?single",
  );
  await page.goto("/?single&seed=3&players=5");
  await expect(lobbyTitle(page)).toHaveText("Single player · 5 players");
  // The table is of the page: the other seats are rule bots, and nobody is given a link.
  await expect(lobbyRows(page)).toHaveCount(5);
  await expect(lobbyRows(page).nth(0)).toContainText("Player (you)");
  await expect(lobbyRows(page).nth(4)).toContainText("Rule bot 4");
  await expect(page.getByRole("button", { name: "Copy link" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Join in a new tab" })).toHaveCount(0);
  // The board has a place for each seat.
  await expect(page.locator("g.system.home")).toHaveCount(5);

  // The setup of the lobby: the seats, the cards, the kind of a seat and the faction.
  await lobbySetup(page).getByRole("button", { name: "One player fewer" }).click();
  await expect(lobbyTitle(page)).toHaveText("Single player · 4 players");
  await expect(lobbyRows(page)).toHaveCount(4);
  await expect(page.locator("g.system.home")).toHaveCount(4);
  await setupValue(page, "Strategy cards").selectOption({ label: "Prophecy of Kings" });
  await expect(setupValue(page, "Strategy cards")).toHaveValue("pok");
  await lobbyPlayers(page).getByRole("button", { name: "Remove Rule bot 3" }).click();
  await expect(lobbyMain(page)).toBeDisabled();
  await expect(page.locator("#lobby-note")).toHaveText("Waiting for 1 open seat");
  await lobbyRows(page).nth(3).getByRole("button", { name: "Random", exact: true }).click();
  await expect(lobbyRows(page).nth(3)).toContainText("Random 1");
  await lobbyPlayers(page)
    .getByRole("button", { name: /^Faction of Player: / })
    .click();
  await lobbyPlayers(page)
    .getByRole("group", { name: "Faction of Player" })
    .getByRole("button", { name: "Hacan", exact: true })
    .click();
  await expect(lobbyPlayers(page).getByRole("button", { name: /^Faction of Player: / })).toHaveText(
    /Hacan/,
  );
  await page.keyboard.press("Escape");
  await page.screenshot({ path: "shots/local/single-setup.png", animations: "disabled" });

  // Enter is the main button: the game of this table, with the faction that was chosen.
  await expect(lobbyMain(page)).toHaveText(/^Start game/);
  await page.keyboard.press("Enter");
  await expect(send(page)).toBeVisible();
  await expect(page.getByRole("region", { name: "Players" }).locator("tbody tr")).toHaveCount(4);
  await expect(
    page.getByRole("region", { name: "Players" }).locator('tbody tr[class*="bg-cyan"]'),
  ).toContainText("Hacan");
  await expect(page.getByRole("button", { name: "Start", exact: true })).toBeVisible();
  const line = () => settings(page).getByText(/^Single player · /);
  await openSettings(page);
  await expect(line()).toHaveText(
    /^Single player · seed 3 · 4 seats · 0 answers saved · \d+ decisions of all seats$/,
  );
  await expect(settings(page).getByRole("region", { name: "Start from" })).toHaveCount(0);
  await page.keyboard.press("Escape");

  // An answer is kept, and a reload is this game, not another setup.
  await panel(page).getByRole("button", { pressed: false }).first().click();
  await send(page).click();
  await expect.poll(() => savedLine(page, /^Single player · /)).toContain("1 answers saved");
  await page.goto("/");
  // The game is kept in the save store under the slot of single player, with its table.
  const saved = await written(page, "single", 1);
  expect(saved.save.format).toBe(2);
  // The decisions of every seat are kept, in their order: the other seats decided before the page.
  expect(saved.save.decisions.length).toBeGreaterThan(1);
  const table = saved.save.game.table!;
  expect(table.setup).toMatchObject({ player_count: 4, seed: 3, strategy_cards: "pok" });
  expect(table.seats.map((seat) => seat.kind)).toEqual(["human", "smart", "smart", "random"]);
  // The start screen lists it.
  await expect(page.getByRole("region", { name: "Saved games" }).getByRole("link")).toHaveText([
    /^Single player.*4 seats · Hacan · 1 answers/,
  ]);
  await page
    .getByRole("link", { name: /Single player/ })
    .first()
    .click();
  await expect(page).toHaveURL("/?single");
  await expect(send(page)).toBeVisible();
  await expect(
    page.getByRole("region", { name: "Players" }).locator('tbody tr[class*="bg-cyan"]'),
  ).toContainText("Hacan");

  // "New game" of the Dev sheet drops the game and opens the setup.
  await openSettings(page);
  await settings(page).getByRole("button", { name: "New game" }).click();
  await expect(lobbyTitle(page)).toHaveText(/^Single player · /);
  expect(await savedGame(page, "single")).toBeUndefined();
  expect(errors).toEqual([]);
});

test("single player: the other seats take the wait of the table, and the page says who decides", async ({
  page,
}) => {
  test.setTimeout(10_000);
  await page.goto("/?single&seed=3&players=4");
  await lobbySetup(page).getByRole("button", { name: "Wait more" }).click();
  await expect(setupValue(page, "Bots wait")).toHaveText("0.5 s");
  await lobbyMain(page).click();
  // On this seed three seats choose a strategy card before the page: each takes its time, the
  // toolbar says who the game waits for, and then the page is asked.
  const status = page.locator("header strong").first();
  await expect(status).toHaveText(/^Waiting for /);
  const first = await status.textContent();
  // A reload while the second seat takes its time: the decision of the first seat is kept, so
  // the game does not go back to it.
  await expect(status).not.toHaveText(first!);
  await page.reload();
  await expect(status).toHaveText(/^Waiting for |^Your decision$/);
  expect(await status.textContent()).not.toBe(first);
  await expect(send(page)).toBeVisible();
  await expect(page.locator("header strong").first()).toHaveText("Your decision");
  await expect(heading(page)).toHaveText("Choose a strategy card");
  // The wait is kept next to the saved game.
  const { id } = await written(page, "single");
  expect(await page.evaluate((key) => localStorage.getItem(key), `ti4.save.${id}.wait`)).toBe(
    "500",
  );
});

test("single player: a model plays a seat, its file is fetched once, and the game is played to the action phase", async ({
  page,
}) => {
  test.setTimeout(10_000);
  const errors = pageErrors(page, { console: true });
  /** The files of the models that the page fetched from the server (not from the Cache API). */
  const fetched: string[] = [];
  page.on("requestfinished", (request) => {
    if (/\/api\/models\/[0-9a-f]{64}\.ti4m$/.test(request.url())) {
      fetched.push(request.url());
    }
  });
  await page.goto("/?single&seed=3&players=8");
  await expect(lobbyRows(page)).toHaveCount(8);
  // The e2e server offers three models: each is a kind of an open seat after "Random".
  await lobbyPlayers(page).getByRole("button", { name: "Remove Rule bot 7" }).click();
  const open = lobbyRows(page).nth(7);
  await expect(open.getByRole("button")).toHaveText([
    "Rule bot",
    "Random",
    "Synthetic model 1",
    "Synthetic model 2",
    "Synthetic model 3",
  ]);
  // The row of the open seat fits the panel, and the page does not scroll.
  const list = lobbyPlayers(page).getByRole("list");
  expect(await list.evaluate((node) => node.scrollWidth <= node.clientWidth)).toBe(true);
  expect(
    await page.evaluate(() => ({
      width: document.documentElement.scrollWidth,
      height: document.documentElement.scrollHeight,
    })),
  ).toEqual({ width: 1920, height: 1080 });
  await page.screenshot({ path: "shots/local/single-models.png", animations: "disabled" });

  // Two seats of models: the sign "✦" says the model on hover, and the start waits for the files.
  await open.getByRole("button", { name: "Synthetic model 2" }).click();
  await expect(open).toContainText("Synthetic model 2 1");
  await expect(open.getByRole("img", { name: "Synthetic model 2: played by the page" })).toHaveText(
    "✦",
  );
  await lobbyPlayers(page).getByRole("button", { name: "Remove Rule bot 6" }).click();
  await lobbyRows(page).nth(6).getByRole("button", { name: "Synthetic model 1" }).click();
  await expect(lobbyRows(page).nth(6)).toContainText("Synthetic model 1 1");
  await page.mouse.move(0, 0);
  await page.screenshot({ path: "shots/local/single-models-seated.png", animations: "disabled" });
  await expect(lobbyMain(page)).toBeEnabled();
  await lobbyMain(page).click();

  // The game: the seats of the models play, and the player table has their sign.
  await expect(page.getByText("Round 1 · Strategy phase")).toBeVisible();
  const table = page.getByRole("region", { name: "Players" });
  await expect(
    table.getByRole("img", { name: "Synthetic model 1: no person plays this seat" }),
  ).toHaveText("✦");
  // Each of eight seats takes one strategy card.
  await expect(heading(page)).toHaveText("Choose a strategy card");
  await panel(page).getByRole("button", { pressed: false }).first().click();
  await send(page).click();
  await expect(page.getByText("Round 1 · Action phase")).toBeVisible();
  expect(new Set(fetched).size).toBe(2);
  expect(fetched).toHaveLength(2);

  // A reload plays the saved game again: the files come from the Cache API, not the server.
  await page.reload();
  await expect(page.getByText("Round 1 · Action phase")).toBeVisible();
  expect(fetched).toHaveLength(2);
  expect(errors).toEqual([]);
});

test("single player: a saved game whose model the server no longer offers says so in place of the game", async ({
  page,
}) => {
  await page.goto("/?single&seed=3&players=3");
  await lobbyPlayers(page).getByRole("button", { name: "Remove Rule bot 2" }).click();
  await lobbyRows(page).nth(2).getByRole("button", { name: "Synthetic model 1" }).click();
  await expect(lobbyMain(page)).toBeEnabled();
  await lobbyMain(page).click();
  await expect(send(page)).toBeVisible();
  await written(page, "single");
  // The server dropped the model: the game cannot go on, and "New game" opens the setup.
  await page.route("**/api/models", (route) => route.fulfill({ json: [] }));
  await page.reload();
  await expect(
    page.getByText("The server no longer offers the model “synthetic-1” that this game seats."),
  ).toBeVisible();
  await page.getByRole("button", { name: "New game" }).click();
  await expect(lobbyTitle(page)).toHaveText(/^Single player · /);
});
