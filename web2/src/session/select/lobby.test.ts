import { expect, it } from "vitest";
import type { TableState } from "../table";
import type { BoardTileView, Catalog, LobbySlot, LobbyState, SetupOptions } from "../wire";
import { type LobbyLocal, hostChange, nextNickname, selectLobby } from "./lobby";

const LONG = "Bartholomew Featherstonehaugh the Third of Mecatol";

const slot = (position: number, change: Partial<LobbySlot> = {}): LobbySlot => ({
  slot_id: `slot_${position}`,
  position,
  occupant: null,
  nickname: null,
  connected: false,
  can_take_over: false,
  ...change,
});
const person = (position: number, id: string, nickname: string, change: Partial<LobbySlot> = {}) =>
  slot(position, { occupant: id, nickname, connected: true, ...change });

/** A table of six: the host, a player with a long name, a rule bot, and three open seats. */
const lobby = (change: Partial<LobbyState> = {}): LobbyState => ({
  game_id: "g1",
  phase: "lobby",
  host_player_id: "ada",
  slots: [
    person(1, "ada", "Ada", { faction: "hacan" }),
    person(2, "bart", LONG),
    slot(3, {
      occupant: "bot1",
      nickname: "Rule bot 1",
      bot: "smart",
      faction: "sol",
    }),
    slot(4),
    slot(5),
    slot(6),
  ],
  lobby_version: 5,
  bot_kinds: ["random", "smart"],
  map: {
    kind: "template",
    alias: "std6",
    name: "Standard",
    systems: 37,
    hyperlanes: false,
    recommended: true,
  },
  map_revision: 1,
  strategy_card_set: "te",
  speaker_order: "random",
  factions: ["sol", "hacan"],
  bot_delay_ms: 600,
  ...change,
});
const card = (id: string, name: string, initiative: number) => ({ id, name, initiative });
/** What the server says a host can choose (`GET /api/setup-options`), cut to two cards a set. */
const OPTIONS: SetupOptions = {
  min_players: 2,
  max_players: 8,
  strategy_card_sets: [
    {
      id: "te",
      name: "Thunder's Edge",
      default: true,
      cards: [card("leadership", "Leadership", 1), card("te4construction", "Construction", 4)],
    },
    {
      id: "pok",
      name: "Prophecy of Kings",
      default: false,
      cards: [card("leadership", "Leadership", 1), card("pok4construction", "Construction", 4)],
    },
    {
      id: "base_game_codex1",
      name: "Base game with Codex I",
      default: false,
      cards: [card("leadership", "Leadership", 1), card("base4", "Construction", 4)],
    },
  ],
  faction_groups: [],
  bot_kinds: [
    { id: "smart", name: "Rule bot", description: "The server plays this seat by rules." },
    { id: "random", name: "Random", description: "The server plays this seat at random." },
  ],
  max_bot_delay_ms: 30_000,
  default_bot_delay_ms: 600,
  scenarios: [
    {
      id: "late-game",
      title: "Late game",
      description: "Round 4.",
      category: "Strategy",
      seed: 42,
      players: 8,
      humans: [0],
      any_seed: false,
      map: "recommended",
    },
  ],
};
const of = (
  state: LobbyState,
  viewer: string | null,
  options: SetupOptions | null = OPTIONS,
): TableState => ({ lobby: state, viewer, preview: null, options });
const table = (viewer: string | null, change: Partial<LobbyState> = {}): TableState =>
  of(lobby(change), viewer);
const local = (change: Partial<LobbyLocal> = {}): LobbyLocal => ({
  mine: [],
  inspected: null,
  offline: false,
  ...change,
});
const catalog = {
  components: {
    abilities: [],
    factions: {},
    leaders: [],
    technologies: [],
    faction_sheets: { hacan: { name: "Hacan", title: "The Emirates of Hacan", commodities: 6 } },
  },
} as unknown as Catalog;

it("the host sees the rows with their signs, the bots under them and the removal", () => {
  const view = selectLobby(table("ada"), local({ mine: ["ada"] }), catalog);
  expect(view.title).toBe("Lobby · 6 players");
  expect(view.filled).toBe("3 of 6");
  expect(view.rows.map((row) => [row.sign?.kind ?? null, row.name, row.faction])).toEqual([
    ["host", "Ada", "Hacan"],
    [null, LONG, "Random"],
    // Without the catalogue entry a faction is its id in words, never the bare id of a seat.
    ["rule", "Rule bot 1", "Sol"],
    ["open", "Open", ""],
    ["open", "Open", ""],
    ["open", "Open", ""],
  ]);
  expect(view.rows[0].you).toBe(true);
  expect(view.rows[0].remove).toBeNull();
  expect(view.rows[1].remove?.intent).toEqual({ type: "remove", player: "bart" });
  expect(view.rows[2].remove?.label).toBe("Remove Rule bot 1");
  // An open seat has no button of a kind: the row under the seats has the bot, the rule bot first.
  expect(view.rows[3].buttons.map((button) => [button.label, button.intent])).toEqual([
    ["Join in a new tab", { type: "joinTab" }],
  ]);
  expect(view.rows[4].buttons).toEqual([]);
  expect(view.bots?.options.map((option) => option.id)).toEqual(["smart", "random"]);
  expect(view.bots?.enabled).toBe(true);
  expect(view.bots?.add.intent).toEqual({ type: "addBot", kind: "smart" });
  expect(view.bots?.fill.intent).toEqual({ type: "fillBots", kind: "smart" });
  // The rows are the seats of the game: the host puts them in another order.
  expect(view.shuffle).toMatchObject({ label: "Shuffle seats", intent: { type: "shuffleSeats" } });
  expect(view.help.map((item) => item.title)).toEqual(["Rule bot", "Random"]);
});

it("the start waits for the open seats, and says so", () => {
  const view = selectLobby(table("ada"), local(), null);
  expect(view.footer).toEqual({
    note: "Waiting for 3 open seats",
    main: { label: "Start game", disabled: true, intent: { type: "start" } },
    nickname: false,
  });
  const one = lobby();
  one.slots[3] = person(4, "c", "Cy");
  one.slots[4] = person(5, "d", "Di");
  expect(selectLobby(of(one, "ada"), local(), null).footer.note).toBe("Waiting for 1 open seat");
});

it("a full table starts: nobody says Ready", () => {
  const full = lobby();
  full.slots[3] = person(4, "c", "Cy");
  full.slots[4] = person(5, "d", "Di", { connected: false });
  full.slots[5] = slot(6, { occupant: "bot2", nickname: "Random bot 1", bot: "random" });
  const view = selectLobby(of(full, "ada"), local(), null);
  expect(view.footer).toEqual({
    note: "",
    main: { label: "Start game", disabled: false, intent: { type: "start" } },
    nickname: false,
  });
  // With no open seat the bots are there, disabled, and say why.
  expect(view.bots?.enabled).toBe(false);
  expect(view.bots?.add).toEqual({ label: "+ Bot", title: "No seat is open", intent: null });
  expect(view.bots?.fill.intent).toBeNull();
  expect(view.rows[5].sign).toEqual({ kind: "random", word: "Random: played by the server" });
});

it("the names of the kinds of a seat are those of the server: none is offered before it said them", () => {
  const view = selectLobby(of(lobby(), "ada", null), local(), null);
  expect(view.rows[3].buttons.map((button) => button.label)).toEqual(["Join in a new tab"]);
  expect(view.bots).toBeNull();
  expect(view.help).toEqual([]);
  expect(view.rows[2].sign).toEqual({ kind: "rule", word: "Played by the server" });
  // A kind with no sign (a bare "mlp" is no kind) is not offered.
  const mlp = selectLobby(table("ada", { bot_kinds: ["random", "smart", "mlp"] }), local(), null);
  expect(mlp.bots?.options.map((option) => option.label)).toEqual(["Rule bot", "Random"]);
});

it("every model of the options is a bot of the list after the rule bot and Random, with its sign", () => {
  const models = ["Reviewer 473312", "Stage 2 r5", "Champion 9"].map((label, at) => ({
    id: `mlp:m${at}` as const,
    name: label,
    description: "A trained model plays this seat.",
  }));
  const options = { ...OPTIONS, bot_kinds: [...OPTIONS.bot_kinds, ...models] };
  const state = lobby({
    bot_kinds: ["smart", "random", "mlp:m0", "mlp:m1", "mlp:m2"],
    slots: [
      ...lobby().slots.slice(0, 3),
      slot(4, { occupant: "bot2", nickname: "Reviewer 473312 1", bot: "mlp:m0" }),
      slot(5),
      slot(6),
    ],
  });
  const view = selectLobby(of(state, "ada", options), local(), null);
  // An open seat has no button of a kind: the row under the seats has the one list.
  expect(view.rows[5].buttons).toEqual([]);
  expect(view.bots?.options.map((option) => [option.label, option.intent])).toEqual([
    ["Rule bot", { type: "chooseBot", kind: "smart" }],
    ["Random", { type: "chooseBot", kind: "random" }],
    ["Reviewer 473312", { type: "chooseBot", kind: "mlp:m0" }],
    ["Stage 2 r5", { type: "chooseBot", kind: "mlp:m1" }],
    ["Champion 9", { type: "chooseBot", kind: "mlp:m2" }],
  ]);
  expect(view.rows[3].sign).toEqual({
    kind: "model",
    word: "Reviewer 473312: played by the server",
  });
  // The rule bot until the page chose another; the choice of the page is kept.
  expect(view.bots?.fill.intent).toEqual({ type: "fillBots", kind: "smart" });
  expect(view.bots?.add.intent).toEqual({ type: "addBot", kind: "smart" });
  const chosen = selectLobby(of(state, "ada", options), local({ bot: "mlp:m1" }), null);
  expect(chosen.bots?.chosen).toBe("mlp:m1");
  expect(chosen.bots?.sign.kind).toBe("model");
  expect(chosen.bots?.fill.intent).toEqual({ type: "fillBots", kind: "mlp:m1" });
});

it("the start of a table in the page waits for the file of a model, and the footer says how far it is", () => {
  const full = lobby({ slots: lobby().slots.slice(0, 3) });
  const view = selectLobby(
    {
      ...of(full, "ada"),
      local: true,
      loading: { label: "Reviewer 473312", done: 12e6, total: 23e6 },
    },
    local(),
    null,
  );
  expect(view.footer.note).toBe("Loading Reviewer 473312 · 12 / 23 MB");
  expect(view.footer.main?.disabled).toBe(true);
});

it("a player who is not the host has the same rows, no controls of the table and no main button", () => {
  const view = selectLobby(table("bart"), local({ mine: ["bart"] }), null);
  expect(view.rows.map((row) => row.name)).toEqual(
    selectLobby(table("ada"), local(), null).rows.map((row) => row.name),
  );
  expect(view.rows[1].you).toBe(true);
  expect(view.rows.every((row) => row.remove === null)).toBe(true);
  // Several seats of one browser: the first open seat can be joined from another tab.
  expect(view.rows.map((row) => row.buttons.map((button) => button.label))).toEqual([
    [],
    [],
    [],
    ["Join in a new tab"],
    [],
    [],
  ]);
  expect(view.bots).toBeNull();
  expect(view.shuffle).toBeNull();
  expect(view.help).toHaveLength(2);
  expect(view.setup.every((row) => row.control === null)).toBe(true);
  expect(view.rows.map((row) => row.rename)).toEqual([false, true, false, false, false, false]);
  expect(view.footer).toEqual({ note: "Waiting for 3 open seats", main: null, nickname: false });

  const full = lobby();
  for (const index of [3, 4, 5]) {
    full.slots[index] = person(index + 1, `p${index}`, `Player ${index}`);
  }
  expect(selectLobby(of(full, "bart"), local(), null).footer.note).toBe("Waiting for Ada to start");
});

it("a visitor joins with a name, while a seat is open", () => {
  const view = selectLobby(table(null), local(), null);
  expect(view.rows.every((row) => !row.you && row.buttons.length === 0 && !row.remove)).toBe(true);
  expect(view.footer).toEqual({
    note: "3 open seats",
    main: { label: "Join", disabled: false, intent: { type: "join" } },
    nickname: true,
  });
  const full = lobby();
  for (const index of [3, 4, 5]) {
    full.slots[index] = person(index + 1, `p${index}`, `Player ${index}`);
  }
  expect(selectLobby(of(full, null), local(), null).footer).toEqual({
    note: "Every seat is taken",
    main: { label: "Join", disabled: true, intent: { type: "join" } },
    nickname: false,
  });
});

it("a tab with no seat marks the players of this browser, and a row makes the tab that player", () => {
  const view = selectLobby(table(null), local({ mine: ["ada", "bart"] }), null);
  expect(view.rows.map((row) => row.become)).toEqual([
    { type: "become", player: "ada" },
    { type: "become", player: "bart" },
    null,
    null,
    null,
    null,
  ]);
  // The row of the tab itself is not such a control.
  const own = selectLobby(table("ada"), local({ mine: ["ada", "bart"] }), null);
  expect(own.rows.map((row) => row.become?.type ?? null).slice(0, 2)).toEqual([null, "become"]);
});

it("a player whose socket is closed has the sign of it, and the viewer never has", () => {
  const away = lobby();
  away.slots[0] = person(1, "ada", "Ada", { connected: false });
  away.slots[1] = person(2, "bart", LONG, { connected: false });
  const seen = selectLobby(of(away, "bart"), local(), null);
  expect(seen.rows[0].sign).toEqual({ kind: "away", word: "Host · not connected" });
  expect(seen.rows[1].sign).toBeNull();
  const visitor = selectLobby(of(away, null), local(), null);
  expect(visitor.rows[1].sign).toEqual({ kind: "away", word: "Not connected" });
});

it("the setup says the players, the map, the strategy cards, the wait and where the game starts", () => {
  const view = selectLobby(table("bart"), local(), null);
  expect(view.setup.map((row) => [row.label, row.value])).toEqual([
    ["Players", "6"],
    ["Map", "Standard · 37 systems"],
    ["Strategy cards", "Thunder's Edge"],
    ["Bots wait", "0.6 s"],
    ["Start from", "Round 1"],
  ]);
  const random = selectLobby(
    table("bart", {
      map: { kind: "random", systems: 1, hyperlanes: true, recommended: false },
      strategy_card_set: "base_game_codex1",
      bot_delay_ms: 0,
    }),
    local(),
    null,
  );
  expect(random.setup.map((row) => row.value)).toEqual([
    "6",
    "Random · 1 system · hyperlanes",
    "Base game with Codex I",
    "Off",
    "Round 1",
  ]);
  // Before the server said the names, a set is its id in words; a server with no scenario has no row for it.
  const bare = selectLobby(of(lobby(), "ada", null), local(), null);
  expect(bare.setup.map((row) => [row.label, row.value])).toEqual([
    ["Players", "6"],
    ["Map", "Standard · 37 systems"],
    ["Strategy cards", "Te"],
    ["Bots wait", "0.6 s"],
  ]);
  expect(bare.setup.find((row) => row.id === "cards")?.control).toBeNull();
});

const row = (state: TableState, id: string) =>
  selectLobby(state, local(), null).setup.find((item) => item.id === id)!;
const steps = (state: TableState, id: string) => {
  const control = row(state, id).control;
  return control?.kind === "counter"
    ? [control.less, control.more].map((step) => step.intent ?? step.reason)
    : null;
};

it("the host changes the players in steps of one, unless a person is in the way", () => {
  expect(steps(table("ada"), "players")).toEqual([
    { type: "set", change: { players: 5 } },
    { type: "set", change: { players: 7 } },
  ]);
  // Two people and a bot: the bot gives its seat, a person does not.
  const three = lobby();
  three.slots.length = 3;
  expect(steps(of(three, "ada"), "players")?.[0]).toEqual({ type: "set", change: { players: 2 } });
  three.slots[2] = person(3, "c", "Cy");
  expect(steps(of(three, "ada"), "players")?.[0]).toBe("A person is in every seat");
  const two = lobby();
  two.slots.length = 2;
  expect(steps(of(two, "ada"), "players")?.[0]).toBe("A table has 2 seats at least");
  const eight = lobby();
  eight.slots.push(slot(7), slot(8));
  expect(steps(of(eight, "ada"), "players")?.[1]).toBe("A table has 8 seats at most");
});

it("the host chooses the set of strategy cards from the list of the server", () => {
  const cards = row(table("ada", { strategy_card_set: "pok" }), "cards");
  expect(cards.control).toEqual({
    kind: "list",
    chosen: "pok",
    options: [
      { id: "te", label: "Thunder's Edge", intent: { type: "set", change: { cards: "te" } } },
      { id: "pok", label: "Prophecy of Kings", intent: { type: "set", change: { cards: "pok" } } },
      {
        id: "base_game_codex1",
        label: "Base game with Codex I",
        intent: { type: "set", change: { cards: "base_game_codex1" } },
      },
    ],
  });
  // Behind "?": the cards of a set that the usual set does not have.
  expect(cards.help).toEqual([
    { title: "Prophecy of Kings", text: "4 Construction" },
    { title: "Base game with Codex I", text: "4 Construction" },
  ]);
});

it("the host changes the wait in steps of half a second", () => {
  expect(steps(table("ada", { bot_delay_ms: 1000 }), "wait")).toEqual([
    { type: "set", change: { waitMs: 500 } },
    { type: "set", change: { waitMs: 1500 } },
  ]);
  // The value of the server is between two steps: a step goes to the next one.
  expect(steps(table("ada"), "wait")).toEqual([
    { type: "set", change: { waitMs: 500 } },
    { type: "set", change: { waitMs: 1000 } },
  ]);
  expect(steps(table("ada", { bot_delay_ms: 0 }), "wait")?.[0]).toBe("The seats answer at once");
  expect(steps(table("ada", { bot_delay_ms: 30_000 }), "wait")?.[1]).toBe("30 s at most");
  expect(row(table("ada", { bot_delay_ms: 1500 }), "wait").value).toBe("1.5 s");
});

it("a scenario is a game of its own: its players and its cards are not chosen", () => {
  const start = row(table("ada"), "start");
  expect(start.control).toEqual({
    kind: "list",
    chosen: "",
    options: [
      { id: "", label: "Round 1", intent: { type: "set", change: { startFrom: null } } },
      {
        id: "late-game",
        label: "Late game",
        intent: { type: "set", change: { startFrom: "late-game" } },
      },
    ],
  });
  expect(start.help).toEqual([{ title: "Late game", text: "Round 4." }]);

  const late = table("ada", { start_from: "late-game", strategy_card_set: "pok" });
  expect(row(late, "start").value).toBe("Late game");
  expect(row(late, "players").control).toBeNull();
  expect(row(late, "cards").control).toBeNull();
  expect(row(late, "cards").value).toBe("Prophecy of Kings");
  // The wait is of the table, not of the game that starts.
  expect(row(late, "wait").control?.kind).toBe("counter");
  // Every player is told where the game starts.
  expect(row(table("bart", { start_from: "late-game" }), "start")).toMatchObject({
    value: "Late game",
    control: null,
  });
});

it("a change of the host is one sentence for the other players", () => {
  const before = lobby();
  expect(hostChange(before, lobby())).toBeNull();
  expect(hostChange(before, lobby({ lobby_version: 6 }))).toBeNull();
  expect(hostChange(before, lobby({ map_revision: 2 }))).toBe("The host changed the map");
  expect(hostChange(before, lobby({ strategy_card_set: "pok" }))).toBe(
    "The host changed the strategy cards",
  );
  expect(hostChange(before, lobby({ bot_delay_ms: 1000 }))).toBe(
    "The host set the wait of the bots to 1 s",
  );
  expect(hostChange(before, lobby({ bot_delay_ms: 0 }))).toBe(
    "The host set the wait of the bots to off",
  );
  // A new count has another map: the count is what the host changed.
  const five = lobby({ map_revision: 2 });
  five.slots.length = 5;
  expect(hostChange(before, five)).toBe("The host set the players to 5");
  expect(hostChange(before, lobby({ start_from: "late-game", map_revision: 2 }))).toBe(
    "The host changed where the game starts",
  );
  // The same players in another order of the rows; a join is no shuffle.
  const shuffled = lobby();
  shuffled.slots = [
    shuffled.slots[2],
    shuffled.slots[0],
    ...shuffled.slots.slice(3),
    shuffled.slots[1],
  ];
  expect(hostChange(before, shuffled)).toBe("The host shuffled the seats");
  const joined = lobby();
  joined.slots[3] = person(4, "cy", "Cy");
  expect(hostChange(before, joined)).toBeNull();
});

it("a lost connection has one signal", () => {
  expect(selectLobby(table("ada"), local(), null).status).toBe("");
  expect(selectLobby(table("ada"), local({ offline: true }), null).status).toBe(
    "Offline · reconnecting",
  );
});

it("the board is the tiles of the preview, and a home is the place of a seat", () => {
  const tile = (
    system_id: string,
    q: number,
    change: Partial<BoardTileView> = {},
  ): BoardTileView => ({
    system_id,
    label: `System ${system_id}`,
    q,
    r: 0,
    ...change,
  });
  const state: TableState = {
    ...table("ada"),
    preview: {
      choice: { kind: "random" },
      player_count: 2,
      tiles: [
        tile("18", 0, {
          planets: [{ id: "mecatol", label: "Mecatol Rex", resources: 1, influence: 6 }],
        }),
        tile("seat_1", 1),
        tile("26", 2, { wormholes: ["alpha"] }),
        tile("39", 3, { wormholes: ["alpha"] }),
        tile("nexus", 9, { special_area: "nexus" }),
      ],
      seats: [{ seat: 1, home_system_id: "seat_1" }],
    },
  };
  const view = selectLobby(state, local({ inspected: "18" }), null);
  expect(view.board.tiles.map((item) => [item.id, item.home, item.planets.length])).toEqual([
    ["18", null, 1],
    // Nobody sits there yet: the home is a place, with no planets of a faction.
    ["seat_1", "place", 0],
    ["26", null, 0],
    ["39", null, 0],
  ]);
  expect(view.seats.place).toBeDefined();
  expect(view.board.wormholes).toEqual([["26", "39"]]);
  expect(view.board.inspector?.title).toBe("System 18 · 18");
  expect(view.board.task).toBeNull();
  // Before the server said the board, the lobby has an empty one.
  expect(selectLobby(table("ada"), local(), null).board.tiles).toEqual([]);
});

it("another tab of a player is that name with the next free number", () => {
  expect(nextNickname(lobby(), "ada")).toBe("Ada 2");
  const more = lobby();
  more.slots[3] = person(4, "ada2", "Ada 2");
  expect(nextNickname(more, "ada")).toBe("Ada 3");
  // From the second tab the number goes on: it is not "Ada 2 2".
  expect(nextNickname(more, "ada2")).toBe("Ada 3");
  expect(nextNickname(lobby(), "bart").length).toBeLessThanOrEqual(64);
  // 21 characters of three bytes: the number takes the room of the last one.
  const wide = lobby();
  wide.slots[0] = person(1, "ada", "東".repeat(21));
  expect(nextNickname(wide, "ada")).toBe(`${"東".repeat(20)} 2`);
});

/** The factions as the server groups them, cut to four. */
const GROUPED: SetupOptions = {
  ...OPTIONS,
  faction_groups: [
    {
      name: "Base game",
      factions: [
        { id: "sol", name: "Sol" },
        { id: "hacan", name: "Hacan" },
        { id: "jolnar", name: "Jol-Nar" },
      ],
    },
    { name: "Prophecy of Kings", factions: [{ id: "argent", name: "Argent Flight" }] },
  ],
};
const editors = (viewer: string | null, change: Partial<LobbyState> = {}) =>
  selectLobby(of(lobby(change), viewer, GROUPED), local(), catalog).rows.map((row) => row.editor);

it("the row of the viewer opens into the choice of its faction: Random first, then the groups", () => {
  const [ada, bart] = editors("bart");
  // A row of another player is not a control.
  expect(ada).toBeNull();
  expect(bart!.label).toBe(`Faction of ${LONG}`);
  expect(bart!.random).toEqual({
    id: "",
    name: "Random",
    chosen: true,
    intent: null,
    taken: "",
    sheet: null,
  });
  expect(bart!.groups.map((group) => [group.name, group.options.map((item) => item.name)])).toEqual(
    [
      ["Base game", ["Sol", "Hacan", "Jol-Nar"]],
      ["Prophecy of Kings", ["Argent Flight"]],
    ],
  );
  // A faction that another seat has stays in its place and says who has it.
  const [sol, hacan, jolnar] = bart!.groups[0].options;
  expect([sol.intent, sol.taken]).toEqual([null, "Rule bot 1"]);
  expect([hacan.intent, hacan.taken]).toEqual([null, "Ada"]);
  expect(jolnar).toMatchObject({
    chosen: false,
    taken: "",
    intent: { type: "faction", player: "bart", faction: "jolnar" },
  });
});

it("the chosen faction has the mark, and Random gives it back", () => {
  const [ada] = editors("ada");
  expect(ada!.random).toMatchObject({
    chosen: false,
    intent: { type: "faction", player: "ada", faction: null },
  });
  const hacan = ada!.groups[0].options[1];
  // The own faction is not taken: it is the chosen one.
  expect(hacan).toMatchObject({ chosen: true, intent: null, taken: "" });
});

it("the host chooses for a seat that the server plays, and for no other person", () => {
  const rows = editors("ada");
  expect(rows.map((editor) => editor !== null)).toEqual([true, false, true, false, false, false]);
  const bot = rows[2]!;
  expect(bot.label).toBe("Faction of Rule bot 1");
  expect(bot.groups[0].options[2].intent).toEqual({
    type: "faction",
    player: "bot1",
    faction: "jolnar",
  });
  expect(bot.groups[0].options[1].taken).toBe("Ada");
  // A player who is not the host has the own row only, and a visitor none.
  expect(editors("bart").map((editor) => editor !== null)).toEqual([
    false,
    true,
    false,
    false,
    false,
    false,
  ]);
  expect(editors(null).every((editor) => editor === null)).toBe(true);
});

it("nothing is chosen for a table of a scenario, or after the start", () => {
  expect(editors("ada", { start_from: "late-game" }).every((editor) => editor === null)).toBe(true);
  expect(editors("ada", { phase: "running" }).every((editor) => editor === null)).toBe(true);
});

it("until the server said the groups, the factions of the table are one list", () => {
  const view = selectLobby(of(lobby(), "ada", null), local(), catalog);
  expect(view.rows[0].editor!.groups).toMatchObject([
    {
      name: "",
      options: [
        { id: "sol", name: "Sol", taken: "Rule bot 1" },
        { id: "hacan", name: "Hacan", chosen: true },
      ],
    },
  ]);
});

it("the sheet of a faction is behind its name, where the catalogue has it", () => {
  const view = selectLobby(of(lobby(), "bart", GROUPED), local(), catalog);
  expect(view.rows.map((row) => row.sheet?.title ?? null)).toEqual([
    "The Emirates of Hacan",
    null,
    // The catalogue of this test has no sheet of Sol.
    null,
    null,
    null,
    null,
  ]);
  expect(view.rows[1].editor!.groups[0].options[1].sheet).toEqual({
    title: "The Emirates of Hacan",
    facts: [
      { label: "Commodities", value: "6" },
      { label: "Starting technologies", value: "None" },
    ],
    abilities: [],
    technologies: [],
    leaders: [],
  });
});

it("a table in the page is single player: no tab to join from, no name to change, and the page plays the other seats", () => {
  const state: TableState = {
    ...of(
      lobby({
        slots: [
          person(1, "ada", "Ada"),
          slot(2, { occupant: "b", nickname: "Rule bot 1", bot: "smart", connected: true }),
          slot(3),
        ],
      }),
      "ada",
    ),
    local: true,
  };
  const view = selectLobby(state, local(), catalog);
  expect(view.title).toBe("Single player · 3 players");
  expect(view.rows[0].you).toBe(true);
  expect(view.rows[0].rename).toBe(false);
  expect(view.rows[0].editor).not.toBeNull();
  expect(view.rows[1].sign).toEqual({ kind: "rule", word: "Rule bot: played by the page" });
  expect(view.rows[1].editor).not.toBeNull();
  expect(view.rows[1].remove?.intent).toEqual({ type: "remove", player: "b" });
  expect(view.rows[2].buttons.map((button) => button.label)).toEqual(["Rule bot", "Random"]);
  expect(view.setup.find((row) => row.id === "wait")?.help[0].text).toMatch(
    /^A seat that the page plays waits/,
  );
  expect(view.footer.main).toEqual({
    label: "Start game",
    disabled: true,
    intent: { type: "start" },
  });
});

/** The table of a game that runs: Ada, Bart who is away, and a rule bot, in the order of their seats. */
const running = (change: Partial<LobbyState> = {}) =>
  lobby({
    phase: "running",
    slots: [
      person(1, "bart", LONG, { connected: false, can_take_over: true, faction: "sol" }),
      person(2, "ada", "Ada", { faction: "hacan" }),
      slot(3, { occupant: "bot1", nickname: "Rule bot 1", bot: "smart", connected: true }),
    ],
    ...change,
  });

it("the table of a running game is the same screen with nothing open, and the host sets the wait", () => {
  const view = selectLobby(of(running(), "ada"), local(), catalog);
  expect(view.title).toBe("Table · 3 players");
  // The rows are the seats of the game, by their players.
  expect(view.rows.map((row) => [row.player, row.name, row.sign?.kind ?? null])).toEqual([
    ["bart", LONG, "away"],
    ["ada", "Ada", "host"],
    ["bot1", "Rule bot 1", "rule"],
  ]);
  for (const row of view.rows) {
    expect([row.editor, row.remove, row.rename, row.buttons]).toEqual([null, null, false, []]);
  }
  expect(view.bots).toBeNull();
  expect(view.shuffle).toBeNull();
  expect(view.footer).toEqual({ note: "", main: null, nickname: false });
  // Of the setup only the wait has a control.
  expect(view.setup.filter((row) => row.control).map((row) => row.id)).toEqual(["wait"]);
  const wait = view.setup.find((row) => row.id === "wait")!;
  expect(wait.control).toMatchObject({
    kind: "counter",
    more: { intent: { type: "set", change: { waitMs: 1000 } } },
  });
});

it("a player who is not the host reads the table of a running game, with no control", () => {
  const view = selectLobby(of(running({ host_player_id: "bart" }), "ada"), local(), catalog);
  expect(view.setup.every((row) => row.control === null)).toBe(true);
  expect(view.rows.flatMap((row) => row.buttons)).toEqual([]);
  expect(view.footer.main).toBeNull();
});

it("a visitor of a running game watches, or sits down in the seat of a person who is away", () => {
  const view = selectLobby(of(running(), null), local(), catalog);
  expect(view.footer).toEqual({
    note: "",
    main: { label: "Watch", disabled: false, intent: { type: "watch" } },
    nickname: false,
  });
  expect(view.rows.map((row) => row.buttons)).toEqual([
    [
      {
        label: "Rejoin",
        title: `Rejoin as ${LONG}: play this seat from this tab`,
        intent: { type: "takeOver", player: "bart", name: LONG },
      },
    ],
    [],
    [],
  ]);
  expect(view.setup.every((row) => row.control === null)).toBe(true);
  // Before the grace time is over the seat is not offered.
  const early = running();
  early.slots[0].can_take_over = false;
  expect(selectLobby(of(early, null), local(), catalog).rows[0].buttons).toEqual([]);
});
