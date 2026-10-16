import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type APIRequestContext, type Page } from "@playwright/test";

// The games of a table of the server: every faction that a lobby can seat, every set of
// strategy cards that it offers, and a speaker that the seed draws. Three tables of eight seat
// the 24 factions once. The names come from the catalogue of the engine of the page
// (`scripts/build-wasm.sh`): no screen shows an id.
const WASM = fileURLToPath(new URL("../src/session/ti4.wasm", import.meta.url));
test.skip(!existsSync(WASM), "the engine is not built: run web2/scripts/build-wasm.sh");
test.use({ viewport: { width: 1920, height: 1080 } });

/** `ti4_engine::seating::LOBBY_FACTIONS`, with the name that a table calls each by. */
const FACTIONS: [id: string, name: string][] = [
  ["sol", "Sol"],
  ["hacan", "Hacan"],
  ["letnev", "Letnev"],
  ["xxcha", "Xxcha"],
  ["jolnar", "Jol-Nar"],
  ["l1z1x", "L1Z1X"],
  ["arborec", "Arborec"],
  ["mentak", "Mentak"],
  ["muaat", "Muaat"],
  ["naalu", "Naalu"],
  ["nekro", "Nekro"],
  ["saar", "Saar"],
  ["sardakk", "Sardakk N'orr"],
  ["winnu", "Winnu"],
  ["yin", "Yin"],
  ["yssaril", "Yssaril"],
  ["ghost", "Creuss"],
  ["argent", "Argent"],
  ["cabal", "Vuil'raith"],
  ["empyrean", "Empyrean"],
  ["mahact", "Mahact"],
  ["naaz", "Naaz-Rokha"],
  ["nomad", "Nomad"],
  ["titans", "Titans"],
];

/**
 * The tables. The page plays the first faction of each: the Ghosts of Creuss have their home
 * off the map, and the mech of the Naaz-Rokha has an id of its own.
 */
const TABLES: { set: string; cards: string[]; factions: [string, string][] }[] = [
  {
    set: "te",
    cards: ["4 Construction", "6 Warfare"],
    factions: [FACTIONS[16], ...FACTIONS.slice(0, 7)],
  },
  {
    set: "pok",
    cards: ["4 Construction", "6 Warfare"],
    factions: [FACTIONS[21], ...FACTIONS.slice(7, 14)],
  },
  {
    set: "base_game_codex1",
    cards: ["4 Construction", "2 Diplomacy"],
    factions: [FACTIONS[14], FACTIONS[15], ...FACTIONS.slice(17, 21), FACTIONS[22], FACTIONS[23]],
  },
];

const panel = (page: Page) => page.locator("#step-panel");
const send = (page: Page) => page.locator('footer button[aria-keyshortcuts="Enter"]');
const status = (page: Page) => page.locator("header strong").first();
const players = (page: Page) => page.getByRole("region", { name: "Players" });

/** A table of eight on the server: the page has the first faction, random seats the others. */
async function start(request: APIRequestContext, table: (typeof TABLES)[number], seed: number) {
  const created = await request.post("/api/games", {
    data: { player_count: 8, seed, nickname: "Ada", strategy_card_set: table.set },
  });
  expect(created.ok(), await created.text()).toBe(true);
  const made = await created.json();
  const lobby = `/api/games/${made.game_id}/lobby`;
  const call = async (path: string, data: unknown) => {
    const response = await request.post(`${lobby}/${path}`, {
      data,
      headers: { "x-ti4-player-session": made.player_session },
    });
    expect(response.ok(), `${path}: ${await response.text()}`).toBe(true);
  };
  const [[mine], ...others] = table.factions;
  await call("seat-faction", { player_id: made.player.id, faction: mine });
  for (const [faction] of others) {
    await call("bots", { kind: "random", faction });
  }
  await call("start", {});
  return made as { game_id: string; player_session: string };
}

for (const [index, table] of TABLES.entries()) {
  const [, mine] = table.factions[0];
  test(`a table of the set ${table.set}, played as ${mine}: every seat and every card has its name`, async ({
    page,
    request,
  }) => {
    const errors: string[] = [];
    page.on("console", (message) => message.type() === "error" && errors.push(message.text()));
    page.on("pageerror", (error) => errors.push(error.message));
    const made = await start(request, table, 11 + index);
    await page.addInitScript(
      ([key, value]) => localStorage.setItem(key, value),
      [`ti4.online.${made.game_id}.session`, made.player_session],
    );
    await page.goto(`/?online=${made.game_id}`);
    await expect(page.getByText("Round 1 · Strategy phase")).toBeVisible();

    // Every seat has the name of its faction, and the seat of the page is found by its player.
    const rows = players(page).locator("tbody tr");
    await expect(rows).toHaveCount(8);
    const names = table.factions.map(([, name]) => name);
    for (const name of names) {
      await expect(rows.filter({ has: page.getByRole("img", { name, exact: true }) })).toHaveCount(
        1,
      );
    }
    // The fleet of the page is on the board, under its name. The home of the Ghosts of Creuss
    // is off the map, and the board has no place for such a system yet (as for the wormhole
    // nexus): the page shows the gate, and not the home with its fleet.
    if (mine === "Creuss") {
      await expect(page.locator('g.system[aria-label*="Creuss Gate"]')).toBeVisible();
    } else {
      await expect(
        page.locator(`g.system[aria-label*="${mine} ship"]`).first(),
        `the ships of ${mine}`,
      ).toBeVisible();
    }

    // The speaker is drawn from the seed: the page is asked, or it says which seat it waits for.
    const waits = new RegExp(
      `^(Your decision|(Waiting for|Turn of) (${names.map((name) => name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")}))$`,
    );
    await expect(status(page)).toHaveText(waits);

    // The seat answers what it is asked, up to the menu of its first turn.
    for (let count = 0; ; count++) {
      expect(count).toBeLessThan(40);
      await expect(status(page)).toHaveText("Your decision");
      if (
        await panel(page)
          .getByRole("button", { name: /^Activate a system$/ })
          .count()
      ) {
        break;
      }
      await panel(page).getByRole("button", { pressed: false }).first().click();
      await send(page).click();
      await expect(status(page)).not.toHaveText("Your decision");
    }
    await expect(page.getByText("Round 1 · Action phase")).toBeVisible();

    // The eight cards of the set are with the seats, each with its number and its printed name.
    const column = await players(page)
      .locator("thead tr > *")
      .evaluateAll((cells) => cells.findIndex((cell) => cell.textContent === "Strategy cards"));
    // A seat that had its turn before this one may have played its card: the card has "✓" then.
    const held = (
      await rows.evaluateAll(
        (all, at) => all.map((row) => row.children[at]?.textContent?.trim() ?? ""),
        column,
      )
    )
      .map((card) => card.replace(/ ✓$/, ""))
      .sort();
    expect(held).toHaveLength(8);
    for (const card of held) {
      expect(card).toMatch(/^[1-8] [A-Z][a-z]+$/);
    }
    for (const card of table.cards) {
      expect(held).toContain(card);
    }
    await page.screenshot({ path: `shots/online/table-${table.set}.png`, animations: "disabled" });
    expect(errors).toEqual([]);
  });
}
