import { existsSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  type TestServer,
  spawnTestServer,
  testServerBinary,
} from "../../../web-shared/test/server.ts";
import { localTable } from "./localTable";
import type { ServedModel } from "./models";
import type { LocalGame } from "./savedGame";
import { openSocket } from "@ti4/shared/net/socket.ts";
import { type Table, type TableState, createTable, serverTable } from "./table";

// What a table promises, checked on both: the table in this page (single player, over the
// engine), and the table of a lobby on a real server. The screen of the lobby is one code, so
// this is what it relies on. Not here, because only one has it: the other people of a table (a
// join, a name, a tab that leaves) and its socket.
//
// Both are built outside this suite, and a case is skipped without its build:
// `scripts/build-wasm.sh`, and `cargo b-web`.
const WASM = fileURLToPath(new URL("./ti4.wasm", import.meta.url));
const SERVER = testServerBinary();
/** The models of the servers of the tests: three copies of the synthetic model (`cargo wire-web`). */
const MODEL_DIR = fileURLToPath(new URL("../../../fixtures/models", import.meta.url));
const MODELS: ServedModel[] = (
  JSON.parse(readFileSync(`${MODEL_DIR}/index.json`, "utf8")) as {
    models: { name: string; label: string; sha256: string }[];
  }
).models.map((model) => ({
  id: model.name,
  label: model.label,
  sha256: model.sha256,
  bytes: statSync(`${MODEL_DIR}/${model.name}.ti4m`).size,
}));
const KINDS = ["smart", "random", ...MODELS.map((model) => `mlp:${model.id}`)];

/** One table, as a test uses it. */
interface Side {
  table: Table;
  /** The next table that `wanted` holds for. Tables before it are passed. */
  until(wanted: (state: TableState) => boolean): Promise<TableState>;
  /** The reason of the next refusal. */
  refused(): Promise<string>;
  /** Whether the game of the table was given to the page (a table in the page only). */
  game(): LocalGame | null;
}

function side(table: Table, game: () => LocalGame | null = () => null): Side {
  const states: TableState[] = [];
  const refusals: string[] = [];
  let failed: string | null = null;
  let wake: (() => void) | null = null;
  table.subscribe((event) => {
    if (event.kind === "table") {
      states.push(event);
    } else if (event.kind === "rejected") {
      refusals.push(event.reason);
    } else if (event.kind === "error") {
      failed = event.message;
    }
    wake?.();
  });
  const wait = async (ready: () => boolean) => {
    while (!ready()) {
      if (failed !== null) {
        throw new Error(failed);
      }
      await new Promise<void>((resume) => {
        wake = resume;
      });
    }
  };
  let read = 0;
  let heard = 0;
  return {
    table,
    async until(wanted) {
      for (;;) {
        await wait(() => states.length > read);
        const state = states[read++];
        if (wanted(state)) {
          return state;
        }
      }
    },
    async refused() {
      await wait(() => refusals.length > heard);
      return refusals[heard++];
    },
    game,
  };
}

const others = (state: TableState) =>
  state.lobby.slots.filter((slot) => slot.occupant !== state.viewer);
const open = (state: TableState) => state.lobby.slots.filter((slot) => slot.occupant === null);
/** The table with every seat but the host's open: a table begins with other seats or with none. */
async function cleared(at: Side): Promise<TableState> {
  const first = await at.until((state) => state.options !== null && state.preview !== null);
  for (const slot of others(first)) {
    if (slot.occupant !== null) {
      at.table.removeSeat(slot.occupant);
    }
  }
  return others(first).every((slot) => slot.occupant === null)
    ? first
    : at.until((state) => others(state).every((slot) => slot.occupant === null));
}

/** The cases. `make` gives a new table of four seats on seed 3, with the page as its host. */
function contract(make: () => Promise<Side>) {
  it("says the table, its board and what a host can choose", async () => {
    const at = await make();
    const { lobby, viewer, preview, options } = await at.until(
      (state) => state.options !== null && state.preview !== null,
    );
    expect(viewer).not.toBeNull();
    expect(lobby.host_player_id).toBe(viewer);
    expect(lobby.phase).toBe("lobby");
    expect(lobby.slots).toHaveLength(4);
    expect(lobby.slots[0].occupant).toBe(viewer);
    // The board has a place for each seat, and the table says which map it is.
    expect(preview!.player_count).toBe(4);
    expect(preview!.seats).toHaveLength(4);
    expect(lobby.map.systems).toBeGreaterThan(0);
    expect(options!.strategy_card_sets.map((set) => set.id)).toEqual([
      "te",
      "pok",
      "base_game_codex1",
    ]);
    expect(lobby.strategy_card_set).toBe("te");
    // The models of the host are kinds after the rule bot and the random seat, named by their labels.
    expect(options!.bot_kinds.map((kind) => kind.id)).toEqual(KINDS);
    expect(options!.bot_kinds.slice(2).map((kind) => kind.name)).toEqual(
      MODELS.map((model) => model.label),
    );
    expect(lobby.bot_kinds).toEqual(expect.arrayContaining(KINDS));
    const factions = options!.faction_groups.flatMap((group) => group.factions);
    expect(factions).toHaveLength(24);
    expect([...lobby.factions].sort()).toEqual(factions.map((faction) => faction.id).sort());
    at.table.close();
  }, 30_000);

  it("the number of seats changes the seats and the board, in the limits of a table", async () => {
    const at = await make();
    await at.until((state) => state.preview !== null);
    at.table.setSettings({ players: 5 });
    const five = await at.until(
      (state) => state.lobby.slots.length === 5 && state.preview?.player_count === 5,
    );
    expect(five.preview!.seats).toHaveLength(5);
    at.table.setSettings({ players: 3 });
    const three = await at.until(
      (state) => state.lobby.slots.length === 3 && state.preview?.player_count === 3,
    );
    // The host is never the seat that goes.
    expect(three.lobby.slots.some((slot) => slot.occupant === three.viewer)).toBe(true);
    at.table.setSettings({ players: 9 });
    expect(await at.refused()).not.toBe("");
    at.table.close();
  }, 30_000);

  it("a seat that no person plays is added, given a faction and removed", async () => {
    const at = await make();
    const { viewer } = await cleared(at);
    at.table.addBots("smart");
    const added = await at.until((state) => open(state).length === 2);
    const bot = added.lobby.slots.find((slot) => slot.bot)!;
    expect(bot.bot).toBe("smart");
    expect(bot.nickname).toBeTruthy();
    expect(bot.faction ?? null).toBeNull();

    at.table.setFaction(bot.occupant!, "hacan");
    await at.until((state) => state.lobby.slots.some((slot) => slot.faction === "hacan"));
    // A faction is at the table once.
    at.table.setFaction(viewer!, "hacan");
    expect(await at.refused()).toMatch(/already taken/);
    at.table.setFaction(viewer!, "sol");
    const chosen = await at.until((state) =>
      state.lobby.slots.some((slot) => slot.faction === "sol"),
    );
    expect(chosen.lobby.slots.find((slot) => slot.occupant === viewer)?.faction).toBe("sol");
    // Null gives the faction back to the draw of the start.
    at.table.setFaction(viewer!, null);
    await at.until((state) => !state.lobby.slots.some((slot) => slot.faction === "sol"));

    at.table.removeSeat(bot.occupant!);
    const removed = await at.until((state) => open(state).length === 3);
    expect(removed.lobby.slots.some((slot) => slot.faction === "hacan")).toBe(false);
    at.table.close();
  }, 30_000);

  it("the strategy cards and the wait are values of the table", async () => {
    const at = await make();
    await at.until((state) => state.options !== null);
    at.table.setSettings({ cards: "pok", waitMs: 500 });
    const set = await at.until((state) => state.lobby.strategy_card_set === "pok");
    expect(set.lobby.bot_delay_ms).toBe(500);
    at.table.setSettings({ cards: "base_game" });
    expect(await at.refused()).not.toBe("");
    at.table.close();
  }, 30_000);

  it("a table has the maps of its number of seats, and the host chooses one", async () => {
    const at = await make();
    const first = await at.until((state) => state.preview !== null);
    const maps = await at.table.maps(4);
    expect(maps.length).toBeGreaterThan(1);
    expect(maps.every((map) => map.player_count === 4 && map.buildable)).toBe(true);
    // A table that chose none plays the map that a table of its size starts with.
    const usual = maps.find((map) => map.recommended)!;
    expect(first.lobby.map.alias).toBe(usual.alias);
    const other = maps.find((map) => !map.recommended)!;
    at.table.setMap({ kind: "template", alias: other.alias });
    const chosen = await at.until((state) => state.lobby.map.alias === other.alias);
    expect(chosen.lobby.map_revision).toBeGreaterThan(first.lobby.map_revision);
    at.table.setMap({ kind: "template", alias: "no-such-map" });
    expect(await at.refused()).not.toBe("");
    at.table.close();
  }, 30_000);

  it("the host shuffles the rows, and the game seats the players in their order", async () => {
    const at = await make();
    const first = await at.until((state) => state.preview !== null);
    const names = (state: TableState) => state.lobby.slots.map((slot) => slot.nickname).join();
    let full = first;
    if (open(first).length) {
      at.table.addBots("smart", true);
      full = await at.until((state) => open(state).length === 0);
    }
    at.table.shuffleSeats();
    const shuffled = await at.until((state) => names(state) !== names(full));
    expect([...names(shuffled).split(",")].sort()).toEqual([...names(full).split(",")].sort());
    // The host is the person, wherever its row is.
    const host = shuffled.lobby.slots.find((slot) => slot.occupant === shuffled.viewer);
    expect(host?.bot).toBeUndefined();
    expect(shuffled.lobby.host_player_id).toBe(shuffled.viewer);
    at.table.start();
    const running = await at.until((state) => state.lobby.phase === "running");
    expect(names(running)).toBe(names(shuffled));
    // The page plays the seat of the person, where its row is.
    const game = at.game();
    if (game) {
      const place = shuffled.lobby.slots.findIndex((slot) => slot.occupant === shuffled.viewer);
      expect(game.humans).toBe(1 << place);
    }
    at.table.close();
  }, 30_000);

  it("a model plays a seat: the seat is named by its label, and the table starts with it", async () => {
    const at = await make();
    await cleared(at);
    const kind = `mlp:${MODELS[0].id}`;
    at.table.addBots(kind);
    const added = await at.until((state) => open(state).length === 2);
    const seat = added.lobby.slots.find((slot) => slot.bot)!;
    expect(seat.bot).toBe(kind);
    expect(seat.nickname).toMatch(new RegExp(`^${MODELS[0].label} \\d+$`));
    at.table.addBots("smart", true);
    await at.until((state) => open(state).length === 0 && !state.loading);
    at.table.start();
    await at.until((state) => state.lobby.phase === "running");
    const game = at.game();
    if (game) {
      expect(game.table?.seats.map((each) => each.kind)).toContain(kind);
    }
    at.table.close();
  }, 30_000);

  it("the start waits for every seat, and then the table says that the game runs", async () => {
    const at = await make();
    await cleared(at);
    at.table.start();
    expect(await at.refused()).toBe("Every seat must be filled before starting");
    at.table.addBots("smart", true);
    const full = await at.until((state) => open(state).length === 0);
    expect(others(full).every((slot) => slot.bot === "smart")).toBe(true);
    at.table.start();
    await at.until((state) => state.lobby.phase === "running");
    at.table.close();
  }, 30_000);
}

describe.skipIf(!existsSync(WASM))("the table in this page", () => {
  let module: WebAssembly.Module;
  beforeAll(async () => {
    module = await WebAssembly.compile(readFileSync(WASM));
  });
  const make = async () => {
    let game: LocalGame | null = null;
    const table = localTable(module, {
      nickname: "Contract",
      players: 4,
      seed: 3,
      onStart: (made) => {
        game = made;
      },
      models: MODELS,
      loadModel: async () => undefined,
    });
    return side(table, () => game);
  };
  contract(make);

  it("the start waits for the file of a model, and the table says how far it is", async () => {
    let finish: () => void = () => undefined;
    const table = localTable(module, {
      nickname: "Contract",
      players: 2,
      seed: 3,
      onStart: () => undefined,
      models: MODELS,
      loadModel: (model, progress) =>
        new Promise<void>((resolve) => {
          progress(1_000, model.bytes);
          finish = resolve;
        }),
    });
    const at = side(table);
    await at.until((state) => state.preview !== null);
    at.table.removeSeat("b");
    at.table.addBots(`mlp:${MODELS[1].id}`);
    const waiting = await at.until((state) => state.loading?.done === 1_000);
    expect(waiting.loading).toEqual({
      label: MODELS[1].label,
      done: 1_000,
      total: MODELS[1].bytes,
    });
    at.table.start();
    expect(await at.refused()).toBe(`${MODELS[1].label} is still loading`);
    finish();
    await at.until((state) => !state.loading);
    at.table.start();
    await at.until((state) => state.lobby.phase === "running");
    at.table.close();
  });

  it("begins with a rule bot in every other seat, and has no other person", async () => {
    const at = await make();
    const state = await at.until((have) => have.preview !== null);
    expect(state.local).toBe(true);
    expect(
      state.lobby.slots.map((slot) => [slot.occupant, slot.bot ?? null, slot.nickname]),
    ).toEqual([
      ["a", null, "Contract"],
      ["b", "smart", "Rule bot 1"],
      ["c", "smart", "Rule bot 2"],
      ["d", "smart", "Rule bot 3"],
    ]);
    // A seat more is a rule bot more.
    at.table.setSettings({ players: 5 });
    const five = await at.until((have) => have.lobby.slots.length === 5);
    expect(five.lobby.slots[4].bot).toBe("smart");
    expect(await at.table.join("Bob")).toBeNull();
    expect(await at.refused()).toBe("A table in the page has one person.");
    at.table.close();
  });

  it("the start gives the page the game of the table: its setup, its seats and its wait", async () => {
    const at = await make();
    const first = await at.until((state) => state.preview !== null);
    at.table.setFaction("a", "hacan");
    at.table.removeSeat("c");
    at.table.addBots("random");
    at.table.setSettings({ cards: "pok", waitMs: 1000 });
    at.table.start();
    await at.until((state) => state.lobby.phase === "running");
    expect(at.game()).toEqual({
      seed: 3,
      players: 4,
      humans: 1,
      waitMs: 1000,
      table: {
        setup: {
          player_count: 4,
          seed: 3,
          map: { kind: "template", alias: first.lobby.map.alias },
          strategy_cards: "pok",
        },
        // The names of the players: the game says who sits in each seat by them.
        seats: [
          { kind: "human", faction: "hacan", name: "Contract" },
          { kind: "smart", name: "Rule bot 1" },
          { kind: "random", name: "Random 1" },
          { kind: "smart", name: "Rule bot 2" },
        ],
      },
    });
    // After the start nothing changes.
    at.table.setSettings({ players: 5 });
    at.table.close();
  });

  it("a scenario is a game of its own: the table has its seats and its factions", async () => {
    const at = await make();
    const first = await at.until((state) => state.options !== null);
    const scenario = first.options!.scenarios[0];
    at.table.setSettings({ startFrom: scenario.id });
    const set = await at.until((state) => state.lobby.start_from === scenario.id);
    expect(set.lobby.slots).toHaveLength(scenario.players);
    expect(set.preview!.player_count).toBe(scenario.players);
    expect(set.lobby.slots.every((slot) => !!slot.faction)).toBe(true);
    expect(new Set(set.lobby.slots.map((slot) => slot.faction)).size).toBe(scenario.players);
    at.table.start();
    await at.until((state) => state.lobby.phase === "running");
    expect(at.game()?.table?.setup.start_from).toBe(scenario.id);
    expect(at.game()?.table?.seats.every((seat) => seat.faction === undefined)).toBe(true);
    at.table.close();
  });
});

describe.skipIf(!SERVER)("the table of a lobby on a server", () => {
  let server: TestServer;
  const fetcher: typeof fetch = (input, init) => fetch(`${server.url}${input}`, init);
  beforeAll(async () => {
    server = await spawnTestServer({ env: { TI4_MODEL_DIR: MODEL_DIR } });
  }, 60_000);
  afterAll(() => server?.stop());
  contract(async () => {
    const made = await createTable({ playerCount: 4, nickname: "Contract", seed: 3 }, fetcher);
    return side(
      serverTable(made.gameId, made, {
        open: (id) => openSocket(`ws://127.0.0.1:${server.port}/ws/tables/${id}`),
        fetch: fetcher,
      }),
    );
  });
});
