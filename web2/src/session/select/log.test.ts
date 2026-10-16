import { describe, expect, it } from "vitest";
import type { LogItemView, LogTurnView } from "../../model";
import { eventsOf, withEvent } from "../events";
import type { Catalog, GameEvent, Phase, PublicFact, PublicRoll, SessionUpdate } from "../wire";
import { selectLog } from "./log";

// A game of three seats: Sol (a, the viewer), Hacan (b) and Jol-Nar (c), as the events of the
// host tell it. The names come from the board and the catalogue.
const update = (
  change: { viewer?: string | null; round?: number; phase?: Phase } = {},
): SessionUpdate =>
  ({
    viewer:
      change.viewer === null
        ? { role: "spectator" }
        : { role: "player", seat: change.viewer ?? "a" },
    seats: {},
    view: {
      round: change.round ?? 3,
      phase: change.phase ?? "action",
      players: [
        { id: "a", faction: "sol" },
        { id: "b", faction: "hacan" },
        { id: "c", faction: "jolnar" },
      ],
      board: {
        map_tiles: [
          {
            system_id: "18",
            label: "Mecatol Rex",
            q: 0,
            r: 0,
            planets: [{ id: "mr", label: "Mecatol Rex", resources: 1, influence: 6 }],
          },
          {
            system_id: "38",
            label: "Abyz, Fria",
            q: 1,
            r: 0,
            planets: [
              { id: "abyz", label: "Abyz", resources: 3, influence: 0 },
              { id: "fria", label: "Fria", resources: 2, influence: 0 },
            ],
          },
          {
            system_id: "01",
            label: "Jord",
            q: 2,
            r: 0,
            planets: [{ id: "jord", label: "Jord", resources: 4, influence: 2 }],
          },
        ],
      },
    },
  }) as unknown as SessionUpdate;

const printed = {
  components: {
    faction_sheets: {
      sol: { name: "Sol", commodities: 4 },
      hacan: { name: "Hacan", commodities: 6 },
      jolnar: { name: "Jol-Nar", commodities: 4 },
    },
    strategy_cards: [
      { id: "pok6warfare", name: "Warfare", initiative: 6 },
      { id: "pok7technology", name: "Technology", initiative: 7 },
    ],
    technologies: [{ id: "gd", name: "Gravity Drive", text: "", used: false }],
    leaders: [],
    relics: [],
    abilities: [],
    action_cards: [
      { id: "sabo1", name: "Sabotage", phase: "Any", window: "", group: "", text: "" },
    ],
    public_objectives: [],
    secret_objectives: [],
  },
  records: {
    units: [
      { id: "carrier", base_type: "carrier", name: "Carrier I" },
      { id: "cruiser", base_type: "cruiser", name: "Cruiser I" },
      { id: "dreadnought", base_type: "dreadnought", name: "Dreadnought I" },
      { id: "infantry", base_type: "infantry", name: "Infantry I" },
    ],
    planets: [],
    technologies: [],
    leaders: [],
    relics: [],
    abilities: [],
    action_cards: [],
    promissory_notes: [],
    explores: [],
    public_objectives: [
      { id: "corner", name: "Corner the Market", phase: "Status", points: 1, text: "" },
    ],
    secret_objectives: [],
  },
  exploration: { cards: [] },
} as unknown as Catalog;

let counter = 0;
/** A public decision of the action phase: `seat` decides in the action of `actor`. */
const decided = (
  action: string,
  actor: string,
  seat: string,
  fact: PublicFact | null,
  change: Partial<GameEvent> = {},
): GameEvent => ({
  id: `g-${++counter}`,
  timestamp: "",
  visibility: { visibility: "public" },
  event: { kind: "decision_resolved" },
  action_id: action,
  action_actor: actor,
  actor: seat,
  round: 3,
  phase: "action",
  ...(fact ? { fact } : {}),
  ...change,
});
const rolled = (action: string, actor: string, roll: Partial<PublicRoll>): GameEvent => ({
  id: `g-${++counter}`,
  timestamp: "",
  visibility: { visibility: "public" },
  event: { kind: "dice_rolled" },
  action_id: action,
  action_actor: actor,
  round: 3,
  phase: "action",
  roll: { index: counter, reason: "space combat", label: "Space combat", faces: [5], ...roll },
});
const transition = (phase: Phase, round: number): GameEvent => ({
  id: `g-${++counter}`,
  timestamp: "",
  visibility: { visibility: "public" },
  event: { kind: "phase_transition", phase, round },
});
/** A public decision of a phase that has no turns. */
const inPhase = (phase: Phase, round: number, seat: string, fact: PublicFact): GameEvent => ({
  id: `g-${++counter}`,
  timestamp: "",
  visibility: { visibility: "public" },
  event: { kind: "decision_resolved" },
  actor: seat,
  round,
  phase,
  fact,
});

const started = (type: "tactical" | "pass"): PublicFact => ({
  kind: "action_started",
  action: { type },
});
const ENDED: PublicFact = { kind: "turn_ended" };

/** A tactical turn of `seat` into Mecatol Rex: it moves, fights and lands. */
const tactical = (action: string, seat: string, enemy: string): GameEvent[] => [
  decided(action, seat, seat, started("tactical"), { stage: "action selection" }),
  decided(action, seat, seat, { kind: "activated", system: "18" }, { stage: "activation" }),
  decided(action, seat, seat, { kind: "moved", unit: "carrier", from: "38", to: "18" }),
  decided(action, seat, seat, { kind: "moved", unit: "carrier", from: "38", to: "18" }),
  decided(action, seat, seat, { kind: "moved", unit: "dreadnought", from: "38", to: "18" }),
  decided(action, seat, seat, { kind: "loaded", unit: "infantry", system: "38", planet: "abyz" }),
  decided(action, seat, seat, null, { stage: "movement", detail: "Done moving" }),
  // Round 1: the dreadnought and the carriers roll apart, then the enemy.
  rolled(action, seat, { roller: seat, faces: [5], hits_on: 5, hits: 1 }),
  rolled(action, seat, { roller: seat, faces: [2, 3], hits_on: 9, hits: 0 }),
  rolled(action, seat, { roller: enemy, faces: [9], hits_on: 9, hits: 1 }),
  decided(action, seat, seat, {
    kind: "casualty",
    seat,
    unit: "carrier",
    system: "18",
    damaged: false,
  }),
  // Round 2.
  rolled(action, seat, { roller: seat, faces: [6], hits_on: 5, hits: 1 }),
  rolled(action, seat, { roller: enemy, faces: [1], hits_on: 9, hits: 0 }),
  decided(action, seat, enemy, {
    kind: "casualty",
    seat: enemy,
    unit: "cruiser",
    system: "18",
    damaged: false,
  }),
  decided(action, seat, seat, { kind: "landed", unit: "infantry", planet: "mr", damaged: false }),
  decided(action, seat, seat, ENDED, { stage: "effects" }),
];
const pass = (action: string, seat: string) => [
  decided(action, seat, seat, started("pass"), { stage: "action selection" }),
];
const technology = (action: string, seat: string, follower: string) => [
  decided(action, seat, seat, {
    kind: "action_started",
    action: { type: "strategic", card: "pok7technology" },
  }),
  decided(action, seat, seat, { kind: "researched", tech: "gd" }),
  decided(action, seat, follower, { kind: "researched", tech: "gd" }),
  decided(action, seat, follower, {
    kind: "paid",
    with: { type: "planet", planet: "jord" },
    spend: "resources",
  }),
  decided(action, seat, follower, {
    kind: "paid",
    with: { type: "trade_good" },
    spend: "resources",
  }),
  decided(action, seat, seat, ENDED),
];

const turns = (items: LogItemView[]) =>
  items.filter((item): item is LogTurnView => item.kind === "turn");
const labels = (items: LogItemView[]) =>
  items.map((item) => (item.kind === "turn" ? `${item.seat} ${item.label}` : item.label));
/** Every text of the Log, for a check of the names. */
const texts = (items: LogItemView[]) =>
  items.flatMap((item) => [
    item.label,
    item.summary,
    ...item.groups.flatMap((group) => [group.title, ...group.lines.map((line) => line.text)]),
  ]);

describe("the turns of the Log", () => {
  it("is one turn for each action, with its kind, its number and its round", () => {
    const log = selectLog(
      update({ viewer: null }),
      eventsOf([
        transition("action", 3),
        ...tactical("action_1", "b", "c"),
        ...technology("action_20", "c", "a"),
        ...pass("action_30", "a"),
      ]),
      printed,
    );
    expect(log.scope).toBe("round");
    expect(labels(log.entries)).toEqual([
      "Action phase · Round 3",
      "b Tactical · Mecatol Rex · #18",
      "c Strategic · Technology",
      "a Pass",
    ]);
    expect(turns(log.entries).map((turn) => [turn.turn, turn.round, turn.action])).toEqual([
      [1, 3, "tactical"],
      [2, 3, "strategic"],
      [3, 3, "pass"],
    ]);
    // A phase with no details is a divider.
    expect(log.entries[0]).toMatchObject({ kind: "phase", groups: [], summary: "" });
  });

  it("groups the details of a tactical action by step, and says the outcome in one line", () => {
    const [turn] = turns(
      selectLog(update({ viewer: null }), eventsOf(tactical("action_1", "b", "c")), printed)
        .entries,
    );
    expect(turn.summary).toBe(
      "moved 3 ships · combat (lost 1, destroyed 1) · landed on Mecatol Rex",
    );
    expect(turn.groups.map((group) => group.title)).toEqual([
      "Activation",
      "Movement",
      "Space combat",
      "Invasion",
    ]);
    expect(turn.groups[1].lines).toEqual([
      { seat: "b", text: "Abyz, Fria · #38 → Mecatol Rex · #18: Dreadnought I, 2 Carrier I" },
      { seat: "b", text: "Loaded on Abyz: Infantry I" },
    ]);
    // The rolls of one round are together; a seat that rolls again starts the next round.
    const combat = turn.groups[2].lines;
    expect(combat.map((line) => [line.seat, line.text, line.dice?.hits])).toEqual([
      ["b", "Space combat · round 1", 1],
      ["b", "Space combat · round 1", 0],
      ["c", "Space combat · round 1", 1],
      ["b", "Lost: Carrier I", undefined],
      ["b", "Space combat · round 2", 1],
      ["c", "Space combat · round 2", 0],
      ["c", "Lost: Cruiser I", undefined],
    ]);
    expect(combat[1].dice).toEqual({ faces: [2, 3], hitsOn: 9, hits: 0 });
    expect(turn.groups[3].lines).toEqual([
      { seat: "b", text: "Landed on Mecatol Rex: Infantry I" },
    ]);
    expect(turn.live).toBe(false);
  });

  it("has the primary and each secondary of a strategic action by seat", () => {
    const [turn] = turns(
      selectLog(update({ viewer: null }), eventsOf(technology("action_1", "c", "a")), printed)
        .entries,
    );
    expect(turn.summary).toBe("researched Gravity Drive");
    expect(turn.groups).toEqual([
      { title: "Primary", lines: [{ seat: "c", text: "Researched Gravity Drive" }] },
      {
        title: "Secondaries",
        lines: [
          { seat: "a", text: "Researched Gravity Drive" },
          { seat: "a", text: "Spent resources: Jord, 1 trade good" },
        ],
      },
    ]);
  });

  it("is a pass with no details", () => {
    const [turn] = turns(
      selectLog(update({ viewer: null }), eventsOf(pass("action_1", "b"))).entries,
    );
    expect(turn).toMatchObject({ label: "Pass", summary: "", groups: [], live: false });
  });

  it("counts a transaction at the turn menu as a part of the action that follows it", () => {
    const log = selectLog(
      update({ viewer: null }),
      eventsOf([
        decided("action_1", "b", "b", { kind: "transaction_opened", with: "c" }),
        decided("action_1", "b", "c", {
          kind: "transaction_answered",
          proposer: "b",
          answer: "accept",
        }),
        ...pass("action_3", "b"),
      ]),
      printed,
    );
    const [turn] = turns(log.entries);
    expect(log.entries).toHaveLength(1);
    expect(turn.label).toBe("Pass");
    // A pass shows no details; the transaction was before it.
    expect(turn.turn).toBe(1);
  });

  it("marks the turn that is played now", () => {
    const events = [
      ...tactical("action_1", "b", "c"),
      decided("action_20", "c", "c", started("tactical")),
      decided("action_20", "c", "c", { kind: "activated", system: "38" }),
    ];
    const live = turns(selectLog(update({ viewer: null }), eventsOf(events), printed).entries);
    expect(live.map((turn) => turn.live)).toEqual([false, true]);
    expect(live[1].label).toBe("Tactical · Abyz, Fria · #38");
    // A phase that follows ends it.
    const over = turns(
      selectLog(
        update({ viewer: null, phase: "status" }),
        eventsOf([...events, transition("status", 3)]),
        printed,
      ).entries,
    );
    expect(over.map((turn) => turn.live)).toEqual([false, false]);
  });
});

describe("the scope of the Log", () => {
  const game = () => [
    transition("action", 3),
    ...tactical("action_1", "b", "c"),
    ...pass("action_20", "a"),
    ...technology("action_21", "c", "b"),
    ...pass("action_30", "b"),
    transition("status", 3),
    inPhase("status", 3, "c", { kind: "objective_scored", objective: "corner", secret: false }),
    transition("agenda", 3),
    transition("strategy", 4),
    inPhase("strategy", 4, "a", { kind: "strategy_picked", card: "pok6warfare" }),
    inPhase("strategy", 4, "b", { kind: "strategy_picked", card: "pok7technology" }),
    transition("action", 4),
    decided("action_40", "b", "b", started("tactical"), { round: 4 }),
  ];

  it("starts at the last turn of the viewer, which is the first entry, with the phases between", () => {
    const log = selectLog(update({ round: 4 }), eventsOf(game()), printed);
    expect(log.scope).toBe("since-last-turn");
    expect(labels(log.entries)).toEqual([
      "a Pass",
      "c Strategic · Technology",
      "b Pass",
      "Status phase · Round 3",
      "Agenda phase · Round 3",
      "Strategy phase · Round 4",
      "Action phase · Round 4",
      "b Tactical",
    ]);
    expect(log.entries[0]).toMatchObject({ mine: true, turn: 2 });
    expect(log.entries.at(-1)).toMatchObject({ live: true, turn: 1, round: 4 });
    const status = log.entries[3];
    expect(status).toMatchObject({
      summary: "1 objective scored",
      groups: [{ title: "Scored", lines: [{ seat: "c", text: "Scored Corner the Market" }] }],
    });
    expect(log.entries[5]).toMatchObject({
      summary: "2 strategy cards picked",
      groups: [
        {
          title: "Picks",
          lines: [
            { seat: "a", text: "Picked Warfare" },
            { seat: "b", text: "Picked Technology" },
          ],
        },
      ],
    });
  });

  it("does not end at the turn of the viewer that is played now: that is last", () => {
    const events = [...game().slice(0, -1), decided("action_40", "a", "a", started("tactical"))];
    const log = selectLog(update({ round: 4 }), eventsOf(events), printed);
    expect(labels(log.entries)[0]).toBe("a Pass");
    expect(log.entries.at(-1)).toMatchObject({ seat: "a", live: true, mine: true });
  });

  it("is the round in play for a spectator, and before the first turn of the viewer", () => {
    const spectator = selectLog(update({ viewer: null, round: 4 }), eventsOf(game()), printed);
    expect(spectator.scope).toBe("round");
    expect(labels(spectator.entries)).toEqual([
      "Strategy phase · Round 4",
      "Action phase · Round 4",
      "b Tactical",
    ]);
    const before = selectLog(
      update({ viewer: "c", round: 3 }),
      eventsOf(game().slice(0, 17)),
      printed,
    );
    expect(before.scope).toBe("round");
    expect(labels(before.entries)).toEqual([
      "Action phase · Round 3",
      "b Tactical · Mecatol Rex · #18",
    ]);
  });

  it("is shorter after an undo: the snapshot of the game replaces the events", () => {
    const events = game();
    const log = events.reduce(withEvent, eventsOf());
    expect(turns(selectLog(update({ round: 4 }), log, printed).entries).at(-1)?.seat).toBe("b");
    // The undo took the last decision back: the snapshot has the events before it.
    const undone = eventsOf(events.slice(0, -1));
    const after = selectLog(update({ round: 4, phase: "strategy" }), undone, printed);
    expect(labels(after.entries).at(-1)).toBe("Action phase · Round 4");
  });
});

describe("what the Log reads", () => {
  it("ignores an event of a seat, an event without a fact and a fact that it does not know", () => {
    const secret: GameEvent = {
      ...decided("action_1", "b", "b", { kind: "researched", tech: "gd" }),
      visibility: { visibility: "seat", seat: "b" },
    };
    const unknown = decided("action_1", "b", "b", {
      kind: "a_fact_of_a_newer_host",
    } as unknown as PublicFact);
    const log = selectLog(
      update({ viewer: null }),
      [
        decided("action_1", "b", "b", started("tactical")),
        secret,
        unknown,
        decided("action_1", "b", "b", null, { detail: "b resolved vfs2 choose reward" }),
      ],
      printed,
    );
    const [turn] = turns(log.entries);
    expect(turn.groups).toEqual([]);
    expect(texts(log.entries).join(" ")).not.toContain("gd");
  });

  it("names every id as the page does: no raw id on screen", () => {
    const log = selectLog(
      update({ viewer: null }),
      eventsOf([
        ...tactical("action_1", "b", "c"),
        ...technology("action_20", "c", "a"),
        decided("action_30", "a", "a", {
          kind: "action_started",
          action: { type: "component", source: { type: "action_card" } },
        }),
        decided("action_30", "a", "a", { kind: "card_played", card: "sabo1" }),
        decided("action_30", "a", "a", { kind: "transaction_opened", with: "b" }),
      ]),
      printed,
    );
    const all = texts(log.entries).join(" | ");
    expect(all).toContain("Component · Sabotage");
    expect(all).toContain("Transaction with Hacan");
    for (const id of [
      "carrier",
      "dreadnought",
      "gd",
      "mr",
      "abyz",
      "pok7technology",
      "sabo1",
      "jord",
    ]) {
      expect(all).not.toMatch(new RegExp(`\\b${id}\\b`));
    }
  });

  it("puts the votes of each agenda in a group, with the planets that each seat exhausted", () => {
    const log = selectLog(
      update({ viewer: null, phase: "agenda" }),
      eventsOf([
        transition("agenda", 3),
        inPhase("agenda", 3, "b", { kind: "voted", outcome: "for" }),
        inPhase("agenda", 3, "b", { kind: "planet_exhausted_for_vote", planet: "abyz" }),
        inPhase("agenda", 3, "b", { kind: "planet_exhausted_for_vote", planet: "fria" }),
        inPhase("agenda", 3, "c", { kind: "voted" }),
        inPhase("agenda", 3, "b", { kind: "voted", outcome: "c" }),
        inPhase("agenda", 3, "c", { kind: "voted", outcome: "mr" }),
      ]),
      printed,
    );
    expect(log.entries).toEqual([
      expect.objectContaining({
        label: "Agenda phase · Round 3",
        summary: "2 agendas voted",
        groups: [
          {
            title: "Agenda 1 · Votes",
            lines: [
              { seat: "b", text: "For · Abyz, Fria" },
              { seat: "c", text: "Abstained" },
            ],
          },
          {
            title: "Agenda 2 · Votes",
            lines: [
              { seat: "b", text: "Jol-Nar" },
              { seat: "c", text: "Mecatol Rex" },
            ],
          },
        ],
      }),
    ]);
  });
});
