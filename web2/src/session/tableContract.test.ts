import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { localTable } from "./localTable";
import type { LocalGame } from "./savedGame";
import { openSocket } from "./socket";
import { type Table, type TableState, createTable, serverTable } from "./table";

// What a table promises, checked on both: the table in this page (single player, over the
// engine), and the table of a lobby on a real server. The screen of the lobby is one code, so
// this is what it relies on. Not here, because only one has it: the other people of a table (a
// join, a name, a tab that leaves) and its socket.
//
// Both are built outside this suite, and a case is skipped without its build:
// `scripts/build-wasm.sh`, and `cargo b-web`.
const WASM = fileURLToPath(new URL("./ti4.wasm", import.meta.url));
const SERVER = fileURLToPath(new URL("../../../target/debug/server", import.meta.url));

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
    expect(lobby.seat_order).toBe("random");
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
    expect(options!.bot_kinds.map((kind) => kind.id)).toEqual(["smart", "random"]);
    expect(lobby.bot_kinds).toEqual(expect.arrayContaining(["smart", "random"]));
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
    });
    return side(table, () => game);
  };
  contract(make);

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
    expect(set.lobby.seat_order).toBe("join");
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

describe.skipIf(!existsSync(SERVER))("the table of a lobby on a server", () => {
  const port = 20_000 + Math.floor(Math.random() * 30_000);
  const data = mkdtempSync(join(tmpdir(), "ti4-table-contract-"));
  let server: ChildProcess;
  const fetcher: typeof fetch = (input, init) => fetch(`http://127.0.0.1:${port}${input}`, init);
  beforeAll(async () => {
    server = spawn(SERVER, [], {
      env: { ...process.env, HOST: "127.0.0.1", PORT: String(port), TI4_DATA_DIR: data },
      stdio: ["ignore", "pipe", "inherit"],
    });
    await new Promise<void>((ready, failed) => {
      server.once("exit", (code) => failed(new Error(`The server stopped with ${code}.`)));
      server.stdout?.on("data", (text) => String(text).includes("Listening on") && ready());
    });
  }, 60_000);
  afterAll(() => {
    server?.kill();
    rmSync(data, { recursive: true, force: true });
  });
  contract(async () => {
    const made = await createTable({ playerCount: 4, nickname: "Contract", seed: 3 }, fetcher);
    return side(
      serverTable(made.gameId, made, {
        open: (id) => openSocket(`ws://127.0.0.1:${port}/ws/tables/${id}`),
        fetch: fetcher,
      }),
    );
  });
});
