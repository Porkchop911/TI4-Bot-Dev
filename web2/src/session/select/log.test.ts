import { describe, expect, it } from "vitest";
import type { LogItemView, LogTurnView } from "../../model";
import { eventsOf, withEvent } from "../events";
import type { Catalog, GameEvent, Phase, PublicFact, PublicRoll, SessionUpdate } from "../wire";
import { selectLog } from "./log";

// A game of three seats: Sol (a, the viewer), Hacan (b) and Jol-Nar (c), as the events of the
// host tell it. The names come from the board and the catalogue.
const update = (
  change: { viewer?: string | null; round?: number; phase?: Phase; active?: string } = {},
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
      ...(change.active ? { active_player: change.active } : {}),
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
      { id: "pok1leadership", name: "Leadership", initiative: 1 },
      { id: "pok2diplomacy", name: "Diplomacy", initiative: 2 },
      { id: "pok3politics", name: "Politics", initiative: 3 },
      { id: "pok5trade", name: "Trade", initiative: 5 },
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
    agendas: [
      { id: "mutiny", name: "Mutiny", kind: "Directive", text: "", group: "" },
      { id: "shared_research", name: "Shared Research", kind: "Law", text: "", group: "" },
    ],
  },
  records: {
    units: [
      { id: "carrier", base_type: "carrier", name: "Carrier I" },
      { id: "cruiser", base_type: "cruiser", name: "Cruiser I" },
      { id: "dreadnought", base_type: "dreadnought", name: "Dreadnought I" },
      { id: "infantry", base_type: "infantry", name: "Infantry I" },
      { id: "fighter", base_type: "fighter", name: "Fighter I" },
      { id: "destroyer", base_type: "destroyer", name: "Destroyer I" },
      { id: "pds", base_type: "pds", name: "PDS I" },
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
  exploration: { cards: [{ id: "vfs2", name: "Volatile Fuel Source" }] },
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
/** A change of the state that no decision told, in the action of `actor`; `seat` is its seat. */
const changed = (
  action: string | null,
  actor: string | null,
  seat: string | null,
  fact: PublicFact,
  change: Partial<GameEvent> = {},
): GameEvent => ({
  id: `g-${++counter}`,
  timestamp: "",
  visibility: { visibility: "public" },
  event: { kind: "state_changed" },
  ...(action ? { action_id: action } : {}),
  ...(actor ? { action_actor: actor } : {}),
  ...(seat ? { actor: seat } : {}),
  round: 3,
  phase: "action",
  fact,
  ...change,
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
      "Space combat · Mecatol Rex · #18 · Hacan vs Jol-Nar",
      "Invasion",
    ]);
    expect(turn.groups[1].lines).toEqual([
      { seat: "b", text: "Abyz, Fria · #38 → Mecatol Rex · #18: Dreadnought I, 2 Carrier I" },
      { seat: "b", text: "Loaded on Abyz: Infantry I" },
    ]);
    // The rolls of one round are together; a seat that rolls again starts the next round.
    const combat = turn.groups[2].lines;
    expect(combat.map((line) => [line.seat, line.text, line.dice?.hits])).toEqual([
      ["b", "Round 1", 1],
      ["b", "Round 1", 0],
      ["c", "Round 1", 1],
      ["b", "Lost: Carrier I", undefined],
      ["b", "Round 2", 1],
      ["c", "Round 2", 0],
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

  it("counts a component action at the turn menu that does not end the turn as a part of it", () => {
    const log = selectLog(
      update({ viewer: null }),
      eventsOf([
        decided("action_1", "b", "b", { kind: "transaction_opened", with: "c" }),
        decided("action_2", "b", "b", {
          kind: "action_started",
          action: { type: "component", source: { type: "leader", leader: "carth" } },
        }),
        decided("action_3", "b", "b", started("tactical")),
        decided("action_3", "b", "b", { kind: "activated", system: "38" }),
        // The turn ends with no fact of its end; a component action that ends its turn is one.
        decided("action_4", "c", "c", {
          kind: "action_started",
          action: { type: "component", source: { type: "action_card" } },
        }),
        decided("action_4", "c", "c", ENDED),
        decided("action_5", "c", "c", started("tactical")),
      ]),
      printed,
    );
    const all = turns(log.entries);
    expect(all.map((turn) => [turn.id, turn.label, turn.turn])).toEqual([
      ["action_1", "Tactical · Abyz, Fria · #38", 1],
      ["action_4", "Component · Action card", 2],
      ["action_5", "Tactical", 3],
    ]);
    expect(all[0].groups.map((group) => group.title)).toEqual([
      "Activation",
      "Effects",
      "Transactions",
    ]);
    expect(all[0].groups[1].lines).toEqual([{ seat: "b", text: "Component action · Carth" }]);
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
    // A turn that ended with no fact of its end is not played now: another seat has the turn.
    const next = turns(
      selectLog(update({ viewer: null, active: "a" }), eventsOf(events), printed).entries,
    );
    expect(next.map((turn) => turn.live)).toEqual([false, false]);
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
      groups: [{ title: "Scored", lines: [{ seat: "c", text: "Corner the Market" }] }],
    });
    expect(log.entries[5]).toMatchObject({
      summary: "2 strategy cards picked",
      groups: [
        {
          title: "Picks",
          lines: [
            { seat: "a", text: "Warfare" },
            { seat: "b", text: "Technology" },
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
            title: "Agenda 1",
            lines: [
              { seat: "b", text: "Voted For: Abyz, Fria" },
              { seat: "c", text: "Abstained" },
            ],
          },
          {
            title: "Agenda 2",
            lines: [
              { seat: "b", text: "Voted Jol-Nar" },
              { seat: "c", text: "Voted Mecatol Rex" },
            ],
          },
        ],
      }),
    ]);
  });
});

describe("the parts of a strategic action", () => {
  const strategic = (action: string, seat: string, card: string): GameEvent =>
    decided(action, seat, seat, { kind: "action_started", action: { type: "strategic", card } });
  const follow = (action: string, actor: string, seat: string, card: string, followed = true) =>
    decided(action, actor, seat, { kind: "secondary_followed", card, followed });

  it("has a part for each seat in Secondaries: its answer, then what it did; who declined last", () => {
    const [turn] = turns(
      selectLog(
        update({ viewer: null }),
        eventsOf([
          strategic("action_1", "a", "pok1leadership"),
          decided("action_1", "a", "a", { kind: "tokens_gained", pool: "tactic", count: 1 }),
          decided("action_1", "a", "a", { kind: "tokens_gained", pool: "strategy", count: 1 }),
          decided("action_1", "a", "a", { kind: "tokens_gained", pool: "tactic", count: 1 }),
          follow("action_1", "a", "c", "pok1leadership", false),
          follow("action_1", "a", "b", "pok1leadership"),
          decided("action_1", "a", "b", {
            kind: "paid",
            with: { type: "planet", planet: "abyz" },
            spend: "influence",
          }),
          decided("action_1", "a", "b", { kind: "tokens_gained", pool: "fleet", count: 1 }),
          decided("action_1", "a", "b", { kind: "token_bought_with_influence" }),
          decided("action_1", "a", "b", { kind: "tokens_gained", pool: "fleet", count: 1 }),
          decided("action_1", "a", "a", ENDED),
          // What a seat does after the end of the turn is still a part of it.
          decided("action_1", "a", "c", { kind: "unit_removed", unit: "carrier", system: "38" }),
        ]),
        printed,
      ).entries,
    );
    expect(turn.label).toBe("Strategic · Leadership");
    expect(turn.summary).toBe("gained 3 command tokens · 1 followed");
    expect(turn.groups).toEqual([
      {
        title: "Primary",
        lines: [{ seat: "a", text: "Gained 2 tactic tokens, 1 strategy token" }],
      },
      {
        title: "Secondaries",
        lines: [
          { seat: "b", text: "Followed" },
          { seat: "b", text: "Spent influence: Abyz" },
          { seat: "b", text: "Gained 2 fleet tokens" },
          { seat: "c", text: "Declined" },
          { seat: "c", text: "Removed in Abyz, Fria · #38: Carrier I" },
        ],
      },
    ]);
  });

  it("says what the primary of Diplomacy, Politics and Trade chose", () => {
    const log = selectLog(
      update({ viewer: null }),
      eventsOf([
        strategic("action_1", "a", "pok2diplomacy"),
        decided("action_1", "a", "a", { kind: "diplomacy_chosen", system: "01" }),
        decided("action_1", "a", "a", { kind: "planet_readied", planet: "jord" }),
        follow("action_1", "a", "b", "pok2diplomacy"),
        decided("action_1", "a", "b", { kind: "planet_readied", planet: "abyz" }),
        decided("action_1", "a", "b", { kind: "planet_readied", planet: "fria" }),
        decided("action_1", "a", "a", ENDED),
        strategic("action_2", "b", "pok3politics"),
        decided("action_2", "b", "b", { kind: "speaker_chosen", seat: "c" }),
        follow("action_2", "b", "a", "pok3politics", false),
        decided("action_2", "b", "b", ENDED),
        strategic("action_3", "c", "pok5trade"),
        decided("action_3", "c", "c", { kind: "commodities_replenished", seat: "c" }),
        decided("action_3", "c", "c", { kind: "commodities_replenished", seat: "a" }),
        follow("action_3", "c", "a", "pok5trade"),
        decided("action_3", "c", "c", ENDED),
      ]),
      printed,
    );
    const [diplomacy, politics, trade] = turns(log.entries);
    expect(diplomacy.summary).toBe("chose Jord · #01 · 1 followed");
    expect(diplomacy.groups).toEqual([
      {
        title: "Primary",
        lines: [
          { seat: "a", text: "Chose Jord · #01" },
          { seat: "a", text: "Readied Jord" },
        ],
      },
      {
        title: "Secondaries",
        lines: [
          { seat: "b", text: "Followed" },
          { seat: "b", text: "Readied Abyz, Fria" },
        ],
      },
    ]);
    expect(politics.summary).toBe("chose Jol-Nar as speaker · none followed");
    expect(politics.groups[0].lines).toEqual([{ seat: "b", text: "Chose Jol-Nar as speaker" }]);
    expect(trade.summary).toBe("replenished 2 seats · 1 followed");
    expect(trade.groups[0].lines).toEqual([
      { seat: "c", text: "Replenished commodities: Jol-Nar, Sol" },
    ]);
  });

  it("says Warfare's recalled token, the command sheet and a free tactical action", () => {
    const [turn] = turns(
      selectLog(
        update({ viewer: null }),
        eventsOf([
          strategic("action_1", "b", "pok6warfare"),
          decided("action_1", "b", "b", { kind: "token_recalled", system: "38" }),
          decided("action_1", "b", "b", {
            kind: "tokens_redistributed",
            tactic: 3,
            fleet: 4,
            strategy: 1,
          }),
          decided("action_1", "b", "b", { kind: "free_tactical", system: "18" }),
          decided("action_1", "b", "b", ENDED),
        ]),
        printed,
      ).entries,
    );
    expect(turn.groups[0].lines.map((line) => line.text)).toEqual([
      "Took the command token back from Abyz, Fria · #38",
      "Redistributed command tokens: 3 tactic · 4 fleet · 1 strategy",
      "Activated Mecatol Rex · #18 without a command token",
    ]);
  });

  it("has the command sheet of each seat of the status phase in a group", () => {
    const [status] = selectLog(
      update({ viewer: null, phase: "status" }),
      eventsOf([
        transition("status", 3),
        inPhase("status", 3, "b", {
          kind: "tokens_redistributed",
          tactic: 3,
          fleet: 3,
          strategy: 2,
        }),
      ]),
      printed,
    ).entries;
    expect(status.groups).toEqual([
      {
        title: "Command tokens",
        lines: [{ seat: "b", text: "3 tactic · 3 fleet · 2 strategy" }],
      },
    ]);
  });
});

describe("the invasion of a tactical action", () => {
  it("numbers the rounds of a ground combat by the fact of each round, on its planet", () => {
    const groundRoll = (seat: string, faces: number[], hits: number) =>
      rolled("action_1", "b", {
        roller: seat,
        reason: "ground combat",
        label: "Ground combat",
        faces,
        hits_on: 8,
        hits,
      });
    const [turn] = turns(
      selectLog(
        update({ viewer: null }),
        eventsOf([
          decided("action_1", "b", "b", started("tactical")),
          decided("action_1", "b", "b", { kind: "activated", system: "18" }),
          decided("action_1", "b", "b", { kind: "custodians_removed", system: "18" }),
          decided("action_1", "b", "b", {
            kind: "bombardment_targeted",
            target: "c",
            planet: "mr",
          }),
          decided("action_1", "b", "b", {
            kind: "landed",
            unit: "infantry",
            planet: "mr",
            damaged: false,
          }),
          decided("action_1", "b", "b", { kind: "ground_combat_round", seat: "b", planet: "mr" }),
          // The same seat rolls twice in one round (two values): still round 1.
          groundRoll("b", [8, 2], 1),
          groundRoll("b", [3], 0),
          groundRoll("c", [9], 1),
          decided("action_1", "b", "c", { kind: "sustain_declined", seat: "c", planet: "mr" }),
          decided("action_1", "b", "b", { kind: "ground_combat_round", seat: "b", planet: "mr" }),
          groundRoll("b", [1], 0),
          groundRoll("c", [10], 1),
          decided("action_1", "b", "c", {
            kind: "ground_combat_started",
            seat: "c",
            planet: "mr",
          }),
          decided("action_1", "b", "b", ENDED),
        ]),
        printed,
      ).entries,
    );
    expect(turn.summary).toBe("landed on Mecatol Rex · removed the custodians");
    const lines = (group: {
      lines: { seat: string | null; text: string; dice?: { faces: number[] } }[];
    }) => group.lines.map((line) => [line.seat, line.text, line.dice?.faces]);
    expect(turn.groups.map((group) => group.title)).toEqual([
      "Activation",
      "Invasion",
      "Ground combat · Mecatol Rex · Hacan vs Jol-Nar",
      "Ground combat · Mecatol Rex",
    ]);
    expect(lines(turn.groups[1])).toEqual([
      ["b", "Removed the custodians from Mecatol Rex · #18", undefined],
      ["b", "Bombarded Jol-Nar on Mecatol Rex", undefined],
      ["b", "Landed on Mecatol Rex: Infantry I", undefined],
    ]);
    expect(lines(turn.groups[2])).toEqual([
      ["b", "Round 1", [8, 2]],
      ["b", "Round 1", [3]],
      ["c", "Round 1", [9]],
      ["c", "Did not sustain damage", undefined],
      ["b", "Round 2", [1]],
      ["c", "Round 2", [10]],
    ]);
    // A further ground combat on the planet is a combat of its own.
    expect(lines(turn.groups[3])).toEqual([["c", "Started a further ground combat", undefined]]);
  });
});

describe("what the state tells of a turn", () => {
  // Jol-Nar (c) attacks Hacan (b) in Mecatol Rex: an anti-fighter barrage, a retreat that Hacan
  // announces and does not make, a loss that the game assigned, the end of the space combat; then
  // two landings, two rounds of ground combat with a loss on each side, the end of the ground
  // combat and the planet that changes hands.
  const story = (action: string) => [
    decided(action, "c", "c", started("tactical"), { stage: "action selection" }),
    decided(action, "c", "c", { kind: "activated", system: "18" }),
    decided(action, "c", "c", { kind: "moved", unit: "destroyer", from: "38", to: "18" }),
    decided(action, "c", "c", { kind: "moved", unit: "carrier", from: "38", to: "18" }),
    decided(action, "c", "c", { kind: "loaded", unit: "infantry", system: "38", planet: "abyz" }),
    decided(action, "c", "c", { kind: "loaded", unit: "infantry", system: "38", planet: "abyz" }),
    rolled(action, "c", {
      roller: "c",
      reason: "anti-fighter barrage",
      label: "Anti-fighter barrage",
      faces: [9, 7],
      hits_on: 9,
      hits: 1,
      system: "18",
    }),
    changed(action, "c", "b", {
      kind: "destroyed",
      seat: "b",
      unit: "fighter",
      count: 1,
      system: "18",
    }),
    decided(action, "c", "b", {
      kind: "retreat_announced",
      seat: "b",
      system: "18",
      retreat: true,
    }),
    rolled(action, "c", { roller: "c", faces: [10, 4], hits_on: 9, hits: 1, system: "18" }),
    rolled(action, "c", { roller: "b", faces: [5], hits_on: 9, hits: 0, system: "18" }),
    changed(action, "c", "b", {
      kind: "destroyed",
      seat: "b",
      unit: "carrier",
      count: 1,
      system: "18",
    }),
    changed(action, "c", "c", {
      kind: "combat_ended",
      combat: "space",
      system: "18",
      attacker: "c",
      defender: "b",
      winner: "c",
    }),
    decided(action, "c", "c", { kind: "landed", unit: "infantry", planet: "mr", damaged: false }),
    decided(action, "c", "c", { kind: "landed", unit: "infantry", planet: "mr", damaged: false }),
    decided(action, "c", "c", { kind: "ground_combat_round", seat: "c", planet: "mr" }),
    rolled(action, "c", {
      roller: "c",
      reason: "ground combat",
      label: "Ground combat",
      faces: [2, 7],
      hits_on: 8,
      hits: 0,
    }),
    rolled(action, "c", {
      roller: "b",
      reason: "ground combat",
      label: "Ground combat",
      faces: [8],
      hits_on: 8,
      hits: 1,
    }),
    changed(action, "c", "c", {
      kind: "destroyed",
      seat: "c",
      unit: "infantry",
      count: 1,
      system: "18",
      planet: "mr",
    }),
    decided(action, "c", "c", { kind: "ground_combat_round", seat: "c", planet: "mr" }),
    rolled(action, "c", {
      roller: "c",
      reason: "ground combat",
      label: "Ground combat",
      faces: [8],
      hits_on: 8,
      hits: 1,
    }),
    rolled(action, "c", {
      roller: "b",
      reason: "ground combat",
      label: "Ground combat",
      faces: [1],
      hits_on: 8,
      hits: 0,
    }),
    changed(action, "c", "b", {
      kind: "destroyed",
      seat: "b",
      unit: "infantry",
      count: 1,
      system: "18",
      planet: "mr",
    }),
    // A structure of a taken planet is destroyed with it.
    changed(action, "c", "b", {
      kind: "destroyed",
      seat: "b",
      unit: "pds",
      count: 1,
      system: "18",
      planet: "mr",
    }),
    changed(action, "c", "c", {
      kind: "combat_ended",
      combat: "ground",
      system: "18",
      planet: "mr",
      attacker: "c",
      defender: "b",
      winner: "c",
    }),
    changed(action, "c", "c", {
      kind: "control_gained",
      seat: "c",
      planet: "mr",
      system: "18",
      from: "b",
    }),
    changed(action, "c", "c", {
      kind: "explored",
      seat: "c",
      card: "vfs2",
      deck: "HAZARDOUS",
      planet: "mr",
    }),
    decided(action, "c", "c", ENDED),
  ];
  const lines = (group: LogTurnView["groups"][number] | undefined) =>
    group?.lines.map((line) => [line.seat, line.text, line.dice?.faces]);

  it("has a block for each combat: its sides, its rounds and losses, and how it ended", () => {
    const [turn] = turns(
      selectLog(update({ viewer: null }), eventsOf(story("action_419")), printed).entries,
    );
    expect(turn.summary).toBe("moved 2 ships · won the space combat · took Mecatol Rex from Hacan");
    expect(turn.groups.map((group) => group.title)).toEqual([
      "Activation",
      "Movement",
      "Space combat · Mecatol Rex · #18 · Jol-Nar vs Hacan",
      "Invasion",
      "Ground combat · Mecatol Rex · Jol-Nar vs Hacan",
      "Exploration",
    ]);
    expect(lines(turn.groups[2])).toEqual([
      ["c", "Anti-fighter barrage", [9, 7]],
      ["b", "Lost: Fighter I", undefined],
      ["b", "Announced a retreat", undefined],
      ["c", "Round 1", [10, 4]],
      ["b", "Round 1", [5]],
      ["b", "Lost: Carrier I", undefined],
      ["c", "Won by Jol-Nar", undefined],
    ]);
    expect(lines(turn.groups[3])).toEqual([
      ["c", "Landed on Mecatol Rex: 2 Infantry I", undefined],
    ]);
    expect(lines(turn.groups[4])).toEqual([
      ["c", "Round 1", [2, 7]],
      ["b", "Round 1", [8]],
      ["c", "Lost: Infantry I", undefined],
      ["c", "Round 2", [8]],
      ["b", "Round 2", [1]],
      ["b", "Lost: Infantry I, PDS I", undefined],
      ["c", "Won by Jol-Nar", undefined],
      ["c", "Took Mecatol Rex from Hacan", undefined],
    ]);
    expect(lines(turn.groups[5])).toEqual([
      ["c", "Explored Mecatol Rex: Volatile Fuel Source", undefined],
    ]);
  });

  it("counts a loss that a seat chose and one that the game took in the same line", () => {
    const [turn] = turns(
      selectLog(
        update({ viewer: null }),
        eventsOf([
          decided("action_1", "c", "c", started("tactical")),
          decided("action_1", "c", "c", { kind: "activated", system: "18" }),
          rolled("action_1", "c", { roller: "c", faces: [10, 10], hits_on: 9, hits: 2 }),
          rolled("action_1", "c", { roller: "b", faces: [1], hits_on: 9, hits: 0 }),
          decided("action_1", "c", "b", {
            kind: "casualty",
            seat: "b",
            unit: "cruiser",
            system: "18",
            damaged: false,
          }),
          changed("action_1", "c", "b", {
            kind: "destroyed",
            seat: "b",
            unit: "fighter",
            count: 2,
            system: "18",
          }),
        ]),
        printed,
      ).entries,
    );
    // The combat is under way: its title has the sides of the rolls.
    expect(turn.groups[1].title).toBe("Space combat · Mecatol Rex · #18 · Jol-Nar vs Hacan");
    expect(lines(turn.groups[1])).toEqual([
      ["c", "Round 1", [10, 10]],
      ["b", "Round 1", [1]],
      ["b", "Lost: Cruiser I, 2 Fighter I", undefined],
    ]);
    expect(turn.summary).toBe("combat (destroyed 3)");
  });

  it("tells a retreat that no decision told, and a combat that left no side", () => {
    const end = (fact: Partial<Extract<PublicFact, { kind: "combat_ended" }>>) => [
      decided("action_1", "c", "c", started("tactical")),
      decided("action_1", "c", "c", { kind: "activated", system: "18" }),
      rolled("action_1", "c", { roller: "c", faces: [3], hits_on: 9, hits: 0 }),
      changed("action_1", "c", "c", {
        kind: "combat_ended",
        combat: "space",
        system: "18",
        attacker: "c",
        defender: "b",
        ...fact,
      }),
    ];
    const combatOf = (fact: Partial<Extract<PublicFact, { kind: "combat_ended" }>>) =>
      turns(selectLog(update({ viewer: null }), eventsOf(end(fact)), printed).entries)[0];
    const retreat = combatOf({ winner: "b", retreated: "c" });
    expect(lines(retreat.groups[1])?.slice(1)).toEqual([
      ["c", "Retreated", undefined],
      ["b", "Won by Hacan", undefined],
    ]);
    expect(retreat.summary).toBe("retreated from the space combat");
    const none = combatOf({});
    expect(lines(none.groups[1])?.slice(1)).toEqual([[null, "Both sides destroyed", undefined]]);
    expect(none.summary).toBe("space combat: both sides destroyed");
    expect(combatOf({ winner: "b" }).summary).toBe("lost the space combat");
  });

  it("keeps a loss to space cannon in the step of the shot", () => {
    const [turn] = turns(
      selectLog(
        update({ viewer: null }),
        eventsOf([
          decided("action_1", "c", "c", started("tactical")),
          decided("action_1", "c", "c", { kind: "activated", system: "18" }),
          rolled("action_1", "c", {
            roller: "b",
            reason: "space cannon",
            label: "Space cannon",
            faces: [9],
            hits_on: 6,
            hits: 1,
          }),
          changed("action_1", "c", "c", {
            kind: "destroyed",
            seat: "c",
            unit: "fighter",
            count: 1,
            system: "18",
          }),
        ]),
        printed,
      ).entries,
    );
    expect(turn.groups.map((group) => [group.title, lines(group)])).toEqual([
      ["Activation", [["c", "Activated Mecatol Rex · #18", undefined]]],
      [
        "Space cannon",
        [
          ["b", "Space cannon", [9]],
          ["c", "Lost: Fighter I", undefined],
        ],
      ],
    ]);
    // No combat: the summary says the loss.
    expect(turn.summary).toBe("lost 1 ship");
  });

  it("names the action card of a component action from the card that the state tells", () => {
    const [turn] = turns(
      selectLog(
        update({ viewer: null }),
        eventsOf([
          decided("action_1", "c", "c", {
            kind: "action_started",
            action: { type: "component", source: { type: "action_card" } },
          } as PublicFact),
          changed("action_1", "c", "c", { kind: "card_played", card: "sabo1" }),
          decided("action_1", "c", "c", ENDED),
        ]),
        printed,
      ).entries,
    );
    expect(turn.label).toBe("Component · Sabotage");
    expect(lines(turn.groups[0])).toEqual([["c", "Played Sabotage", undefined]]);
  });
});

describe("what the state tells of a phase", () => {
  it("has a group for each agenda: its outcome and the votes, each seat's vote and its count", () => {
    const vote = (seat: string, fact: PublicFact) => inPhase("agenda", 3, seat, fact);
    const log = selectLog(
      update({ viewer: null, phase: "agenda" }),
      eventsOf([
        transition("agenda", 3),
        vote("a", { kind: "card_played", card: "sabo1" }),
        vote("b", { kind: "voted", outcome: "for" }),
        vote("b", { kind: "planet_exhausted_for_vote", planet: "abyz", votes: 3 }),
        vote("b", { kind: "planet_exhausted_for_vote", planet: "fria", votes: 2 }),
        vote("c", { kind: "voted", outcome: "against" }),
        vote("c", { kind: "planet_exhausted_for_vote", planet: "jord", votes: 2 }),
        vote("a", { kind: "voted" }),
        changed(
          null,
          null,
          null,
          {
            kind: "agenda_resolved",
            agenda: "mutiny",
            outcome: "for",
            votes: { for: 5, against: 2 },
            speaker_decided: false,
            law: false,
            enacted: false,
            discarded: false,
          },
          { phase: "agenda" },
        ),
        // The second agenda: the same seats vote again, and the speaker breaks a tie.
        vote("b", { kind: "voted", outcome: "against" }),
        vote("b", { kind: "planet_exhausted_for_vote", planet: "abyz", votes: 3 }),
        vote("c", { kind: "voted", outcome: "for" }),
        vote("c", { kind: "planet_exhausted_for_vote", planet: "mr", votes: 3 }),
        vote("a", { kind: "tie_broken", outcome: "for" }),
        changed(
          null,
          null,
          null,
          {
            kind: "agenda_resolved",
            agenda: "shared_research",
            outcome: "for",
            votes: { for: 3, against: 3 },
            speaker_decided: true,
            law: true,
            enacted: true,
            discarded: false,
          },
          { phase: "agenda" },
        ),
      ]),
      printed,
    );
    const [phase] = log.entries;
    expect(phase.summary).toBe("Mutiny: For · Shared Research: For");
    expect(phase.groups).toEqual([
      {
        title: "Mutiny · For (For 5 · Against 2)",
        lines: [
          { seat: "a", text: "Played Sabotage" },
          { seat: "b", text: "Voted For · 5 votes: Abyz, Fria" },
          { seat: "c", text: "Voted Against · 2 votes: Jord" },
          { seat: "a", text: "Abstained" },
        ],
      },
      {
        title: "Shared Research · For (For 3 · Against 3)",
        lines: [
          { seat: "b", text: "Voted Against · 3 votes: Abyz" },
          { seat: "c", text: "Voted For · 3 votes: Mecatol Rex" },
          { seat: "a", text: "Broke the tie: For" },
          { seat: null, text: "Enacted as a law" },
        ],
      },
    ]);
  });

  it("has the objectives that the status phase revealed", () => {
    const log = selectLog(
      update({ viewer: null, phase: "status" }),
      eventsOf([
        transition("status", 3),
        inPhase("status", 3, "b", { kind: "objective_scored", objective: "corner", secret: false }),
        changed(
          null,
          null,
          null,
          { kind: "objective_revealed", objective: "corner" },
          {
            phase: "status",
          },
        ),
      ]),
      printed,
    );
    const [phase] = log.entries;
    expect(phase.summary).toBe("1 objective scored · revealed Corner the Market");
    expect(phase.groups).toEqual([
      { title: "Scored", lines: [{ seat: "b", text: "Corner the Market" }] },
      { title: "Revealed", lines: [{ seat: null, text: "Corner the Market" }] },
    ]);
  });
});
