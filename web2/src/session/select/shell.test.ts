import { expect, it } from "vitest";
import menuUpdate from "../../../../fixtures/protocol/update-menu.json";
import systemsUpdate from "../../../../fixtures/protocol/update-load.json";
import type { Choice, ComponentCatalog, ContentRecords, SessionUpdate } from "../wire";
import type { LocalState } from "./action";
import { selectShell } from "./shell";

// The fixtures are updates of a real game, written by the engine: the menu of an action phase in
// round 2, and the loading of a carrier (`crates/ti4-wasm/tests/it/protocol_fixtures.rs` (`cargo wire-web`)).
const menu = menuUpdate as unknown as SessionUpdate;
const systems = systemsUpdate as unknown as SessionUpdate;
const local = (change: Partial<LocalState> = {}): LocalState => ({
  inspected: null,
  staged: null,
  sent: null,
  error: null,
  replaying: null,
  canUndo: false,
  movement: {},
  landing: {},
  landingKind: null,
  production: { builds: [], planets: [], goods: 0 },
  productionStage: "build",
  handled: [],
  step: null,
  remaining: null,
  planNote: null,
  stepping: null,
  draftLocked: null,
  draft: null,
  ...change,
});
const NO_COMPONENTS: ComponentCatalog = {
  abilities: [],
  factions: {},
  leaders: [],
  technologies: [],
  relics: [],
  legendary: [],
  action_cards: [],
  action_card_groups: [],
  secret_objectives: [],
  secret_objective_groups: [],
  faction_sheets: {},
  strategy_cards: [],
  public_objectives: [],
  public_objective_groups: [],
  agendas: [],
  agenda_groups: [],
};

const NO_RECORDS: ContentRecords = {
  strategy_cards: [],
  secret_objectives: [],
  public_objectives: [],
  action_cards: [],
  technologies: [],
  explores: [],
  planets: [],
  attachments: [],
  factions: [],
  units: [],
  leaders: [],
  abilities: [],
  promissory_notes: [],
  relics: [],
  breakthroughs: [],
};
const flow = (update: SessionUpdate, state: LocalState) => {
  const { action } = selectShell(update, state);
  if (action.kind !== "flow") {
    throw new Error("local play has only the flow panel");
  }
  return action;
};
const rows = (update: SessionUpdate, state: LocalState) =>
  flow(update, state).blocks.flatMap((block) => (block.kind === "menu" ? block.rows : []));

it("shows eight seats with different marks, in seat order", () => {
  const view = selectShell(menu, local());
  expect(view.players.rows.map((row) => row.seat)).toEqual(menu.view.seating_order);
  expect(view.players.rows).toHaveLength(8);
  const marks = Object.values(view.seats).map((seat) => seat.symbol + seat.color);
  expect(new Set(marks).size).toBe(8);
  expect(view.seats.a.faction).toBe("Sol");
  expect(view.players.rows.filter((row) => row.isMe).map((row) => row.seat)).toEqual(["a"]);
  expect(view.players.rows.filter((row) => row.turn === "now").map((row) => row.seat)).toEqual([
    "a",
  ]);
});

it("a seat has the name of its player, and the sign of a seat that no person plays", () => {
  // A game with no table has no names: the letter of the seat tells the seats apart.
  expect(selectShell(menu, local()).seats.a).toMatchObject({ name: "Seat A" });
  expect(selectShell(menu, local()).seats.a.played).toBeUndefined();
  const seats = selectShell(
    {
      ...menu,
      seats: {
        a: { nickname: "Ada", kind: "human" },
        b: { nickname: "Rule bot 1", kind: "smart" },
        c: { nickname: "Random bot 1", kind: "random" },
        d: { nickname: "Reviewer 473312 2", kind: "mlp:reviewer-473312" },
      },
    },
    local(),
  ).seats;
  expect(seats.a).toMatchObject({ name: "Ada", faction: "Sol" });
  expect(seats.a.played).toBeUndefined();
  expect(seats.b).toMatchObject({ name: "Rule bot 1", played: { kind: "rule" } });
  expect(seats.c).toMatchObject({ name: "Random bot 1", played: { kind: "random" } });
  // A model has its sign, with its label on hover.
  expect(seats.d).toMatchObject({
    name: "Reviewer 473312 2",
    played: { kind: "model", word: "Reviewer 473312: no person plays this seat" },
  });
  // A seat that the table does not name keeps its letter.
  expect(seats.e.name).toBe("Seat E");
});

it("counts what the planets of a seat give, ready and total", () => {
  const row = selectShell(menu, local()).players.rows[0];
  const values = new Map(
    (menu.view.board.map_tiles ?? []).flatMap((tile) =>
      (tile.planets ?? []).map((planet) => [planet.id, planet] as const),
    ),
  );
  const own = Object.values(menu.view.board.systems)
    .flatMap((system) => Object.values(system.planets))
    .filter((planet) => planet.controlled_by === "a");
  const sum = (ready: boolean) =>
    own
      .filter((planet) => !ready || !planet.exhausted)
      .reduce((total, planet) => total + values.get(planet.planet_id)!.resources, 0);
  expect(own.length).toBeGreaterThan(0);
  expect(row.planets).toBe(own.length);
  expect(row.resources).toEqual([sum(true), sum(false)]);
});

it("puts every system of the map on the board, and none from outside it", () => {
  const { board } = selectShell(menu, local());
  const onMap = (menu.view.board.map_tiles ?? []).filter((tile) => !tile.special_area);
  expect(board.tiles.map((tile) => tile.id)).toEqual(onMap.map((tile) => tile.system_id));
  const home = board.tiles.find((tile) => tile.id === "01")!;
  expect(home.planets.map((planet) => [planet.name, planet.owner])).toEqual([["Jord", "a"]]);
  expect(home.fleets.map((fleet) => fleet.seat)).toEqual(["a"]);
  const ships = menu.view.board.systems["01"].units.filter(
    (unit) => unit.planet === null && !/pds|spacedock/.test(unit.unit_type),
  );
  expect(home.fleets[0].ships).toBe(ships.length);
});

it("shows the turn menu as the picker: the fixed groups, with what the engine offers", () => {
  const picker = rows(menu, local());
  expect(picker.map((row) => [row.key, row.title, row.disabled])).toEqual([
    ["t", "Activate a system", false],
    ["6", "6 Warfare", false],
    ["a", "Play an action card", true],
    ["c", "Use another component", true],
    ["r", "Transaction", false],
    ["p", "Take no more actions in this round", true],
  ]);
  expect(picker.map((row) => row.state)).toEqual([
    "5 tactic tokens",
    undefined,
    "None is playable",
    "None is ready",
    "2 of 7 players",
    "Use 6 Warfare first",
  ]);
  // A choice in the picker sends nothing: there is no main button until something is staged.
  expect(flow(menu, local()).footer.actions).toEqual([]);
  expect(picker[0].intent).toEqual({ type: "pickAction", action: "tactical" });
  expect(picker[1].intent).toEqual({ type: "pickAction", action: "strategic", card: "strategic" });
});

it("stages an action of the turn menu, and its main button sends it", () => {
  const staged = flow(menu, local({ staged: "strategic" }));
  expect(staged.badge).toEqual({ tone: "draft", label: "Not sent" });
  expect(staged.heading).toBe("6 Warfare");
  expect(staged.footer.actions.map((item) => [item.label, item.key])).toEqual([
    ["Back to actions", "Escape"],
    ["Use Warfare", "Enter"],
  ]);
});

it("lists the partners of a transaction, and says why one cannot trade", () => {
  const trade = rows(menu, local({ menu: "trade" }));
  expect(trade).toHaveLength(7);
  expect(trade.filter((row) => !row.disabled).map((row) => row.intent)).toEqual([
    { type: "chooseOption", option: "component|trade|hacan" },
    { type: "chooseOption", option: "component|trade|l1z1x" },
  ]);
  expect(trade.find((row) => row.disabled)?.state).toBe("No contact: not neighbours");
  expect(flow(menu, local({ menu: "trade" })).footer.actions[0].intent).toEqual({
    type: "backToPicker",
  });
});

it("has no choice after the answer is sent, until the next update", () => {
  const sent = local({ sent: menu.pending_choice!.nonce });
  expect(rows(menu, sent)).toEqual([]);
  expect(flow(menu, sent).footer.actions).toEqual([]);
  expect(selectShell(menu, sent).toolbar.status).toBe("Sent · the game goes on");
});

it("shows a decision without a screen of its own as a list, also inside a tactical action", () => {
  const choice = systems.pending_choice!.choice;
  expect(choice.context?.subtype).toBe("load_cargo");
  expect(rows(systems, local()).map((row) => row.intent)).toEqual(
    choice.options.map((option) => ({ type: "chooseOption", option: option.id })),
  );
  expect(selectShell(systems, local()).board.task).toBeNull();
});

it("shows the forces of an inspected system", () => {
  const { inspector } = selectShell(menu, local({ inspected: "01" })).board;
  expect(inspector?.title).toBe("Jord - Sol · 01");
  expect(inspector?.rows[0].seat).toBe("a");
  expect(inspector?.rows.at(-1)?.planet?.name).toBe("Jord");
});

it("says what the exploration means for a planet: its type, and that a home is never explored", () => {
  const { board } = selectShell(menu, local({ inspected: "01" }));
  expect(board.inspector?.rows.at(-1)?.planet?.traits).toEqual({
    types: [],
    unexplored: false,
    attachments: [],
  });
  const saudor = board.tiles.find((tile) => tile.id === "23")!.planets[0];
  expect(saudor.traits).toMatchObject({ types: ["industrial"], unexplored: true });
});

it("names an attachment as printed, with what it changes", () => {
  const update: SessionUpdate = structuredClone(menu);
  update.view.board.systems["01"].planets.jord.attachments = ["dysonsphere", "cybernetic", "x_y"];
  const catalog = {
    cards: [],
    decks: {},
    attachments: [
      { id: "dysonsphere", name: "Dyson Sphere", resources: 2, influence: 1, text: "Printed." },
      {
        id: "cybernetic",
        name: "Cybernetic Research Facility",
        resources: 0,
        influence: 0,
        tech_specialties: ["CYBERNETIC"],
      },
    ],
  };
  const jord = selectShell(update, local(), {
    content: "",
    exploration: catalog,
    components: NO_COMPONENTS,
    records: NO_RECORDS,
  }).board.tiles.find((tile) => tile.id === "01")!.planets[0];
  expect(jord.traits.attachments).toEqual([
    { name: "Dyson Sphere", effect: "+2 / +1", text: "Printed." },
    { name: "Cybernetic Research Facility", effect: "Yellow skip", text: "" },
    // An attachment that the catalogue does not have keeps its id as the name.
    { name: "X Y", effect: "", text: "" },
  ]);
});

it("lists the exploration decks: what is left of each card, who drew it, who has the fragments", () => {
  const update: SessionUpdate = structuredClone(menu);
  update.exploration = {
    decks: { CULTURAL: ["crf2", "crf3", "pw"], HAZARDOUS: [], INDUSTRIAL: [], FRONTIER: [] },
    drawn: [
      {
        round: 1,
        player: "a",
        deck: "CULTURAL",
        card: "crf1",
        planet: "saudor",
        outcome: "fragment",
      },
    ],
    explores: {},
  };
  update.view.players.find((player) => player.id === "a")!.relic_fragments = { CULTURAL: 1 };
  const catalog = {
    cards: [
      ...["crf1", "crf2", "crf3"].map((id) => ({
        id,
        name: "Cultural Relic Fragment",
        deck: "CULTURAL",
        resolution: "Fragment",
        text: "Purge 3.",
      })),
      { id: "pw", name: "Paradise World", deck: "CULTURAL", resolution: "Attach", text: "+2." },
    ],
    decks: { CULTURAL: ["crf1", "crf2", "crf3", "pw"] },
    attachments: [],
  };
  const view = selectShell(update, local(), {
    content: "",
    exploration: catalog,
    components: NO_COMPONENTS,
    records: NO_RECORDS,
  });
  const [cultural, hazardous] = view.reference.exploration;
  expect(view.reference.exploration.map((deck) => deck.deck)).toEqual([
    "cultural",
    "hazardous",
    "industrial",
    "frontier",
  ]);
  expect(cultural).toMatchObject({ title: "Cultural", left: 3, total: 4 });
  expect(cultural.fragments).toEqual([{ seat: "a", count: 1 }]);
  expect(cultural.cards).toEqual([
    {
      name: "Cultural Relic Fragment",
      kind: "Fragment",
      text: "Purge 3.",
      left: 2,
      total: 3,
      drawn: ["Round 1 · Sol · Saudor"],
    },
    { name: "Paradise World", kind: "Attach", text: "+2.", left: 1, total: 1, drawn: [] },
  ]);
  expect(hazardous).toMatchObject({ left: 0, total: 0, cards: [] });
  expect(view.players.rows[0].fragments).toEqual([{ deck: "cultural", count: 1 }]);
});

it("says why the game stopped", () => {
  const view = selectShell(menu, local({ error: "run: the engine failed" }));
  expect(view.toolbar.status).toBe("The game stopped");
});

it("offers undo once there is an answer to take back", () => {
  const labels = (state: LocalState) => flow(menu, state).footer.actions.map((item) => item.label);
  expect(labels(local())).toEqual([]);
  expect(labels(local({ canUndo: true }))).toEqual(["Undo"]);
  expect(flow(menu, local({ canUndo: true })).footer.actions[0].intent).toEqual({ type: "undo" });
});

it("has no choice while a saved game is played again: the update is out of date", () => {
  const replaying = local({ canUndo: true, replaying: { done: 12, total: 40 } });
  expect(rows(menu, replaying)).toEqual([]);
  expect(flow(menu, replaying).footer.actions).toEqual([]);
  expect(selectShell(menu, replaying).toolbar.status).toBe("Replaying 12 of 40");
});

/** The menu fixture with another question of seat a. */
const asking = (
  choice: Partial<Choice>,
  view: Partial<SessionUpdate["view"]> = {},
): SessionUpdate => ({
  ...menu,
  view: { ...menu.view, ...view },
  pending_choice: {
    ...menu.pending_choice!,
    choice: { player: "a", prompt: "choose", options: [], ...choice },
  },
});

it("says whose turn it is after the turn of the viewer, and that the viewer passed", () => {
  const theirs: SessionUpdate = {
    ...menu,
    pending_choice: undefined,
    view: { ...menu.view, active_player: "b" },
    turn_status: { kind: "waiting_for_decision", seat: "b", phase: "action", round: 2, stage: "" },
  };
  const waiting = flow(theirs, local());
  expect([waiting.title, waiting.subtitle, waiting.heading]).toEqual([
    "Turn ended",
    "Hacan is next",
    "Hacan’s turn",
  ]);
  expect(waiting.pill).toEqual({ tone: "quiet", label: "Waiting for Hacan" });
  expect(waiting.footer.note).toBe("Waiting for Hacan.");
  const passed: SessionUpdate = {
    ...theirs,
    view: {
      ...theirs.view,
      players: theirs.view.players.map((player) =>
        player.id === "a" ? { ...player, passed: true } : player,
      ),
    },
  };
  expect(flow(passed, local()).title).toBe("You passed");
});

it("closes the turn: what uses no action, and End turn", () => {
  const choice = menu.pending_choice!.choice;
  const closing = asking({
    ...choice,
    prompt: "end your turn",
    options: [
      { id: "end_turn", kind: "end_turn", label: "end your turn", payload: {} },
      ...choice.options.filter((option) => option.kind === "open_transaction"),
    ],
    details: { ...choice.details, closing: true },
  });
  const view = flow(closing, local());
  expect(view.heading).toBe("End your turn");
  expect(view.closing).toEqual([
    expect.objectContaining({ kind: "menu", title: "Before you end your turn" }),
  ]);
  expect(view.footer.actions.at(-1)).toMatchObject({
    label: "End turn",
    intent: { type: "endTurn" },
  });
});

it("chooses a planet of a component action on the board, with the ability behind the heading", () => {
  const drop = asking({
    prompt: "Orbital Drop: onto which planet",
    options: [
      { id: "jord", kind: "planet", label: "jord", payload: { planet: "jord", system: "01" } },
      {
        id: "saudor",
        kind: "planet",
        label: "saudor",
        payload: { planet: "saudor", system: "23" },
      },
    ],
    context: {
      ...menu.pending_choice!.choice.context!,
      source: { FactionAbility: "orbital_drop" },
    },
  });
  const shell = selectShell(drop, local());
  expect(shell.board.task).toMatchObject({ target: "planet", kind: "pick" });
  expect(Object.keys(shell.board.task!.values)).toEqual(["jord", "saudor"]);
  expect(shell.board.taskSystems).toEqual(expect.arrayContaining(["01", "23"]));
  const view = flow(drop, local());
  expect(view.title).toBe("Component action");
  expect(view.pill?.label).toBe("Your decision · Choose a planet");
  expect(view.blocks).toEqual([
    expect.objectContaining({
      kind: "card",
      title: "Planet",
      rows: [expect.objectContaining({ title: ["No planet chosen"] })],
    }),
  ]);
  const staged = flow(drop, local({ staged: "saudor" }));
  expect(staged.blocks[0]).toMatchObject({ rows: [{ title: ["Saudor · #23"] }] });
  expect(staged.footer.note).toBe("Selected: Saudor");
});

it("chooses a player of a decision in the player table", () => {
  const choose = asking({
    prompt: "choose a player",
    options: [
      { id: "hacan", kind: "player", label: "hacan", payload: {} },
      { id: "done", kind: "decline", label: "nobody", payload: {} },
    ],
    details: { seats: { hacan: "b" } },
  });
  const shell = selectShell(choose, local());
  expect(shell.players.pick).toEqual({
    verb: "Choose",
    seats: { b: { reason: null } },
    chosen: [],
  });
  // The answer that names no seat stays a row of the list.
  expect(rows(choose, local()).map((row) => row.title)).toEqual(["Nobody"]);
  expect(selectShell(choose, local({ staged: "hacan" })).players.pick?.chosen).toEqual(["b"]);
});

it("chooses several players that replenish commodities, with what it gives each seat", () => {
  const trade = asking({
    prompt: "let another player replenish commodities",
    options: [
      { id: "hacan", kind: "replenish", label: "hacan replenishes commodities", payload: {} },
      { id: "letnev", kind: "replenish", label: "letnev replenishes commodities", payload: {} },
      { id: "done", kind: "decline", label: "nobody else replenishes", payload: {} },
    ],
    details: {
      seats: { hacan: "b", letnev: "c" },
      commodities: { hacan: { have: 0, max: 6 }, letnev: { have: 1, max: 2 } },
    },
  });
  expect(selectShell(trade, local()).players.pick).toEqual({
    verb: "Choose",
    seats: {
      b: { reason: null, note: "Commodities 0 → 6" },
      c: { reason: null, note: "Commodities 1 → 2" },
    },
    chosen: [],
  });
  const both = local({ replenish: { seats: ["letnev", "hacan"], follow: null } });
  expect(selectShell(trade, both).players.pick?.chosen).toEqual(["b", "c"]);
  expect(flow(trade, both).footer.actions.at(-1)?.label).toBe("Resolve primary");
});

it("shows the fleet supply that a unit is removed for, as the engine counts it", () => {
  const over = asking({
    prompt: "remove a unit: over fleet supply in 01",
    options: [{ id: "remove|0", kind: "remove", label: "remove destroyer", payload: {} }],
    details: { reason: "fleet supply", system: "01", fleet_charged: 3, fleet_limit: 2 },
  });
  expect(flow(over, local()).blocks[0]).toEqual({
    kind: "gauges",
    gauges: [{ label: "Fleet supply in Jord - Sol · #01", used: 3, total: 2 }],
  });
});

it("puts a reaction window on top of the action of another seat: Pass, or play the staged card", () => {
  const printed = {
    content: "",
    exploration: { cards: [], decks: {}, attachments: [] },
    components: {
      ...NO_COMPONENTS,
      action_cards: [
        {
          id: "mining_initiative",
          name: "Mining Initiative",
          phase: "action",
          window: "",
          group: "",
          text: "Gain trade goods equal to the resource value of 1 planet you control.",
        },
      ],
    },
    records: NO_RECORDS,
  };
  const window: SessionUpdate = {
    ...asking({
      prompt: "reaction",
      options: [
        { id: "reaction:sabotage", kind: "action_card", label: "play sabotage", payload: {} },
        { id: "decline", kind: "decline", label: "decline", payload: {} },
      ],
    }),
    reaction: {
      trigger: {
        kind: "action_card_played",
        event_type: "ACTION_CARD_PLAYED",
        event_id: 7,
        relation: "when",
        actor: "b",
        card: "mining_initiative",
      },
      own_turn_applies: true,
      offers: [
        {
          option: "reaction:sabotage",
          key: "Sabotage",
          kind: "action_card",
          name: "Sabotage",
          text: "Cancel that action card.",
        },
      ],
    },
  };
  const view = selectShell(window, local(), printed).action;
  if (view.kind !== "flow") {
    throw new Error("a window is a flow");
  }
  // The action of the other seat stays in view: its card as printed, not resolved yet.
  expect(view.title).toBe("Action card");
  expect(view.subtitle).toBe("Mining Initiative · Hacan");
  expect(view.heading).toBe("Mining Initiative");
  expect(view.pill?.label).toBe("Your decision · Reaction");
  expect(view.blocks).toEqual([
    {
      kind: "source",
      name: "Mining Initiative",
      type: "Action card",
      text: "Gain trade goods equal to the resource value of 1 planet you control.",
    },
    { kind: "note", tone: "quiet", text: "Not resolved yet." },
  ]);
  // The window is on top: the trigger, and a row for each card, with its setting.
  expect(view.interrupt?.eyebrow).toBe("Reaction · When Hacan plays Mining Initiative");
  const [menuBlock] = view.interrupt!.blocks;
  expect(menuBlock.kind === "menu" && menuBlock.rows.map((row) => [row.key, row.title])).toEqual([
    ["1", "Sabotage"],
  ]);
  expect(view.footer.actions.map((item) => [item.label, item.key, item.intent])).toEqual([
    ["Pass", "Enter", { type: "reaction", action: "pass" }],
  ]);
  const staged = selectShell(window, local({ staged: "reaction:sabotage" }), printed).action;
  if (staged.kind !== "flow") {
    throw new Error("a window is a flow");
  }
  expect(staged.badge).toEqual({ tone: "draft", label: "Not sent" });
  expect(staged.footer.note).toBe("Selected: Sabotage");
  expect(staged.footer.actions.map((item) => [item.label, item.key, item.intent])).toEqual([
    ["Clear", "Escape", { type: "clearChoice" }],
    ["Play Sabotage", "Enter", { type: "reaction", action: "play" }],
  ]);
  // A window in a tactical action is over the active system of the seat.
  const tactical = selectShell(
    {
      ...window,
      reaction: {
        ...window.reaction!,
        trigger: {
          kind: "action_completed",
          event_type: "TACTICAL_ACTION_ENDED",
          event_id: 9,
          relation: "after",
          actor: "b",
          system: "01",
        },
      },
    },
    local(),
    printed,
  ).action;
  expect(tactical).toMatchObject({
    title: "Tactical action",
    heading: "Hacan’s tactical action",
    interrupt: { link: ["sys:01"] },
  });
});
