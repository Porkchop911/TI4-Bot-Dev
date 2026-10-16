import { expect, it } from "vitest";
import movementUpdate from "../../../../fixtures/protocol/update-movement.json";
import type { Catalog, CombatView, SessionUpdate } from "../wire";
import type { LocalState } from "./action";
import { roundTable } from "./combat";
import { selectShell } from "./shell";

// A space combat in the active system of a real update of the engine: Sol (a) attacks Letnev (c).
const base = movementUpdate as unknown as SessionUpdate;
const local: LocalState = {
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
};
const printed = {
  records: {
    units: [
      {
        id: "dreadnought",
        base_type: "dreadnought",
        name: "Dreadnought I",
        combat_hits_on: 5,
        combat_die_count: 1,
      },
      {
        id: "sol_carrier",
        base_type: "carrier",
        name: "Advanced Carrier I",
        combat_hits_on: 9,
        combat_die_count: 1,
      },
      { id: "fighter", base_type: "fighter", name: "Fighter I", combat_hits_on: 9 },
      { id: "cruiser", base_type: "cruiser", name: "Cruiser I", combat_hits_on: 7 },
    ],
  },
} as unknown as Catalog;
const ship = (unit_type: string, owner: string, damaged = false) => ({
  unit_type,
  owner,
  planet: null,
  damaged,
  galvanized: false,
});
const COMBAT: CombatView = {
  system_id: "23",
  round: 2,
  battle_seq: 1,
  phase: "pre_roll",
  round_start: [
    ship("dreadnought", "a", true),
    ship("dreadnought", "a"),
    ship("sol_carrier", "a"),
    ship("fighter", "a"),
    ship("fighter", "a"),
    ship("cruiser", "c"),
  ],
  barrage_start: [],
  barrage_hits: {},
  barrage_dice: [],
  remaining_hits: {},
  attacker: "a",
  defender: "c",
};
/** The update while the game asks another seat, or (with `choice`) the viewer. */
const during = (combat: CombatView, viewer = "a", asked = false): SessionUpdate => ({
  ...base,
  tactical: undefined,
  viewer: { role: "player", seat: viewer },
  pending_choice: asked
    ? {
        ...base.pending_choice!,
        nonce: "retreat",
        choice: {
          ...base.pending_choice!.choice,
          player: viewer,
          prompt: "Announce a retreat from 23",
          context: undefined,
          details: {},
          options: [
            { id: "stay", kind: "stay", label: "Stay and fight", payload: {} },
            { id: "retreat", kind: "retreat", label: "Announce a retreat", payload: {} },
          ],
        },
      }
    : undefined,
  turn_status: {
    ...base.turn_status,
    kind: "waiting_for_decision",
    seat: asked ? viewer : "c",
  } as SessionUpdate["turn_status"],
  view: { ...base.view, active_player: "a", board: { ...base.view.board, combat } },
});
const tactical = (update: SessionUpdate, state = local) => {
  // The shell without the catalogue: a faction is named by its id ("Letnev").
  const { action, toolbar } = selectShell(update, state);
  if (action.kind !== "tactical" || action.task.content.kind !== "combat") {
    throw new Error("no space combat");
  }
  return { action, content: action.task.content, status: toolbar.status };
};

it("is the step of the tactical action at a seat that watches, with the sides of the round", () => {
  const { action, content, status } = tactical(during(COMBAT, "b"));
  expect(action.tabs.map((tab) => tab.status)).toEqual([
    "done",
    "done",
    "battle",
    "future",
    "future",
  ]);
  expect(action.tabs[2].caption).toBe("Round 2");
  expect(action.selected).toBe(2);
  // Whom the game waits for is the status of the bar; the pill is the stage of the round.
  expect(status).toBe("Waiting for Letnev");
  expect(action.task.pill).toEqual({ tone: "quiet", label: "Before the roll" });
  expect(action.task.footer.note).toBe("");
  expect(content.decision).toBeNull();
  expect(content.records[0].table.label).toBe("Round 2");
  const table = roundTable(during(COMBAT, "b"), COMBAT, printed);
  expect(table.label).toBe("Round 2");
  expect(table.score).toBe("Before the roll");
  expect(table.sides.map((side) => [side.seat, side.role, side.isViewer])).toEqual([
    ["a", "Attacker", false],
    ["c", "Defender", false],
  ]);
  expect(
    table.sides[0].rows.map((row) => [row.label, row.count, row.target, row.dice, row.hits]),
  ).toEqual([
    ["Dreadnought I", 2, 5, 2, "≈1.2"],
    ["Advanced Carrier I", 1, 9, 1, "≈0.2"],
    ["Fighter I", 2, 9, 2, "≈0.4"],
  ]);
  expect(table.sides[0].rows[0].pills).toEqual([{ tone: "damage", label: "1 damaged" }]);
});

it("keeps the question of the viewer as its list, and the footer sends it", () => {
  const { action, content } = tactical(during(COMBAT, "a", true));
  expect(action.task.pill.label).toBe("Your decision");
  expect(content.decision?.prompt).toBe("Announce a retreat from 23");
  expect(content.decision?.blocks.at(-1)).toMatchObject({
    kind: "menu",
    rows: [{ title: "Stay and fight" }, { title: "Announce a retreat" }],
  });
  expect(action.task.footer.actions.at(-1)?.label).toBe("Send");
  expect(content.records[0].table.sides[0].isViewer).toBe(true);
});

it("after the roll has what the dice needed, with the printed value on hover, and the hits", () => {
  const rolled: CombatView = {
    ...COMBAT,
    phase: "resolving_hits",
    hits_to_assign: 1,
    active_player: "c",
    attacker_hits: 1,
    defender_hits: 0,
    dice_rolls: [
      { player: "a", unit: "dreadnought", roll: 4, target: 4, hit: true },
      { player: "a", unit: "dreadnought", roll: 2, target: 4, hit: false },
      { player: "a", unit: "sol_carrier+fighter", roll: 3, target: 9, hit: false },
    ],
  };
  const table = roundTable(during(rolled), rolled, printed);
  expect(table.score).toBe("Hits to assign");
  expect(table.owed).toEqual({ hits: 1, seat: "c" });
  expect(table.sides.map((side) => side.hits)).toEqual([1, 0]);
  const dreadnought = table.sides[0].rows[0];
  expect([dreadnought.target, dreadnought.modified, dreadnought.targetTitle]).toEqual([
    4,
    true,
    "Printed 5+",
  ]);
  expect(table.sides[0].rows.map((row) => row.dice)).toEqual([[], [], []]);
});

it("is not shown in a draft", () => {
  const { action } = selectShell(during(COMBAT, "b"), {
    ...local,
    draft: { recorded: false } as LocalState["draft"],
  });
  expect(action.kind === "tactical" && action.task.content.kind === "combat").toBe(false);
});
