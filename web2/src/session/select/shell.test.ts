import { expect, it } from "vitest";
import menuUpdate from "../fixtures/update-menu.json";
import systemsUpdate from "../fixtures/update-load.json";
import type { ComponentCatalog, SessionUpdate } from "../wire";
import type { LocalState } from "./action";
import { selectShell } from "./shell";

// The fixtures are updates of a real game, written by the engine:
//   cargo run -p ti4-wasm --example record -- 3 8 1 20 > web2/src/session/fixtures/update-menu.json
//   cargo run -p ti4-wasm --example record -- 3 8 1 6 pok8imperial no tactical 23 'move|01|0' > web2/src/session/fixtures/update-load.json
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
      },
    },
    local(),
  ).seats;
  expect(seats.a).toMatchObject({ name: "Ada", faction: "Sol" });
  expect(seats.a.played).toBeUndefined();
  expect(seats.b).toMatchObject({ name: "Rule bot 1", played: { kind: "rule" } });
  expect(seats.c).toMatchObject({ name: "Random bot 1", played: { kind: "random" } });
  // A seat that the table does not name keeps its letter.
  expect(seats.d.name).toBe("Seat D");
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

it("shows a choice as one list, and stages a row without sending", () => {
  const choice = menu.pending_choice!.choice;
  expect(rows(menu, local()).map((row) => row.intent)).toEqual(
    choice.options.map((option) => ({ type: "chooseOption", option: option.id })),
  );
  expect(flow(menu, local()).footer.actions.at(-1)).toMatchObject({
    label: "Send",
    disabled: true,
  });
  const staged = local({ staged: choice.options[1].id });
  expect(rows(menu, staged).map((row) => row.selected)).toEqual(
    choice.options.map((_, index) => index === 1),
  );
  expect(flow(menu, staged).badge).toEqual({ tone: "draft", label: "Not sent" });
  expect(flow(menu, staged).footer.actions.at(-1)).toMatchObject({
    label: "Send",
    disabled: false,
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
    exploration: catalog,
    components: NO_COMPONENTS,
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
  const view = selectShell(update, local(), { exploration: catalog, components: NO_COMPONENTS });
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
  expect(labels(local())).toEqual(["Send"]);
  expect(labels(local({ canUndo: true }))).toEqual(["Undo", "Send"]);
  expect(flow(menu, local({ canUndo: true })).footer.actions[0].intent).toEqual({ type: "undo" });
});

it("has no choice while a saved game is played again: the update is out of date", () => {
  const replaying = local({ canUndo: true, replaying: { done: 12, total: 40 } });
  expect(rows(menu, replaying)).toEqual([]);
  expect(flow(menu, replaying).footer.actions).toEqual([]);
  expect(selectShell(menu, replaying).toolbar.status).toBe("Replaying 12 of 40");
});
