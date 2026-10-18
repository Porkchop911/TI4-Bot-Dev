import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import type { BlockView, LogPhaseView, SeatRowView } from "../../model";
import { eventsOf } from "../events";
import type { TransportEvent } from "../transport";
import { type Engine, createWasmTransport } from "../wasmTransport";
import type { Catalog, GameEvent, PublicFact, SessionUpdate } from "../wire";
import type { EventLog } from "../events";
import { selectLog } from "./log";
import { selectPastPhase } from "./pastPhase";

// The real engine, built by `scripts/build-wasm.sh` (it is not in the repository).
const WASM = fileURLToPath(new URL("../ti4.wasm", import.meta.url));
const built = existsSync(WASM);

type Update = Extract<TransportEvent, { kind: "update" }>;

/** A game of six seats on seed 3, seat a played here with the first option of `answers` questions. */
async function game(answers: number): Promise<Update[]> {
  const engine: Engine = {
    module: await WebAssembly.compile(readFileSync(WASM)),
    hash: "test",
    scenarios: [],
    catalog: null,
  };
  const transport = createWasmTransport(engine, { seed: 3, players: 6, humans: 1 }, {});
  const seen: Update[] = [];
  let wake: (() => void) | null = null;
  transport.subscribe((event) => {
    if (event.kind === "error") {
      throw new Error(event.message);
    }
    if (event.kind === "update") {
      seen.push(event);
      wake?.();
    }
  });
  let read = 0;
  for (let answer = 0; answer < answers; answer += 1) {
    while (seen.length <= read) {
      await new Promise<void>((resume) => (wake = resume));
    }
    const pending = seen.at(-1)!.update.pending_choice;
    read = seen.length;
    if (pending) {
      transport.submitChoice(pending.nonce, pending.choice.options[0].id);
    }
  }
  return seen;
}

const seatsOf = (blocks: BlockView[], title: string): SeatRowView[] =>
  blocks.flatMap((block) => (block.kind === "seats" && block.title === title ? block.rows : []));

let played: Update[];
let last: Update;
let events: EventLog;
/** The phases of a round as a spectator sees the Log of the round. */
const phasesOf = (round: number): LogPhaseView[] =>
  selectLog(
    {
      ...last.update,
      viewer: { role: "spectator" },
      view: { ...last.update.view, round },
    } as SessionUpdate,
    events,
  ).entries.flatMap((entry) => (entry.kind === "phase" ? [entry] : []));

beforeAll(async () => {
  if (!built) {
    return;
  }
  played = await game(80);
  last = played.at(-1)!;
  events = last.events ?? [];
}, 60_000);

describe("a phase of the Log, opened", () => {
  it.skipIf(!built)(
    "the strategy phase is the picks in their order, each seat with its card and the trade goods it took",
    () => {
      const [strategy] = phasesOf(2).filter((phase) => phase.phase === "strategy");
      const past = selectPastPhase(last.update, events, strategy.id)!;
      expect(past.action.heading).toBe("Strategy phase");
      expect(past.action.context).toBe("Round 2");
      expect(past.action.pill).toEqual({ tone: "done", label: "Resolved" });
      expect(past.action.footer.actions).toEqual([]);
      const picks = seatsOf(past.action.blocks, "Picks");
      expect(picks.map((row) => row.status.label)).toEqual(
        strategy.groups.find((group) => group.title === "Picks")!.lines.map((line) => line.text),
      );
      expect(picks.map((row) => row.order)).toEqual(picks.map((_, at) => `${at + 1}`));
      // A card with trade goods on it says what its seat took.
      for (const row of picks) {
        if (row.text) {
          expect(row.text).toMatch(/^Trade goods \d+ → \d+$/);
        }
      }
    },
  );

  it.skipIf(!built)("the phase that is played now says who decides and who waits", async () => {
    // The strategy phase of a round, while the viewer picks.
    const picking = played.findLast(
      (update) => update.update.view.phase === "strategy" && update.update.pending_choice,
    )!;
    const log = selectLog(picking.update, picking.events ?? []);
    const live = log.entries.find((entry) => entry.live)!;
    expect(live).toMatchObject({ kind: "phase", phase: "strategy" });
    const past = selectPastPhase(picking.update, picking.events ?? [], live.id)!;
    expect(past.action.pill).toEqual({ tone: "live", label: "Played now" });
    const rows = seatsOf(past.action.blocks, "Picks");
    expect(rows).toHaveLength(6);
    expect(rows.filter((row) => row.status.label === "Deciding")).toHaveLength(1);
  });

  it.skipIf(!built)("a turn or an action phase is no phase that opens", () => {
    const [action] = phasesOf(2).filter((phase) => phase.phase === "action");
    expect(selectPastPhase(last.update, events, action.id)).toBeNull();
    expect(selectPastPhase(last.update, events, "status-99")).toBeNull();
  });
});

// A status phase where two seats score: a phase of three seats, as the host tells it.
describe("a status phase of the Log, opened", () => {
  let counter = 0;
  const event = (fact: PublicFact, seat?: string): GameEvent => ({
    id: `s-${++counter}`,
    timestamp: "",
    visibility: { visibility: "public" },
    event: { kind: seat ? "decision_resolved" : "state_changed" },
    ...(seat ? { actor: seat } : {}),
    round: 3,
    phase: "status",
    fact,
  });
  const update = {
    viewer: { role: "player", seat: "a" },
    seats: {},
    view: {
      round: 3,
      phase: "agenda",
      players: [
        { id: "a", faction: "sol" },
        { id: "b", faction: "hacan" },
        { id: "c", faction: "jolnar" },
      ],
      board: { systems: {}, map_tiles: [] },
    },
  } as unknown as SessionUpdate;
  const tokens = { kind: "tokens_redistributed", tactic: 3, fleet: 3, strategy: 2 } as const;
  const events = eventsOf([
    {
      id: "t",
      timestamp: "",
      visibility: { visibility: "public" },
      event: { kind: "phase_transition", phase: "status", round: 3 },
    },
    event({ kind: "objective_scored", objective: "corner", secret: false }, "b"),
    event({ kind: "objective_scored", objective: "monument", secret: true }, "a"),
    event({ kind: "objective_revealed", objective: "expand" }),
    event(tokens, "a"),
    event(tokens, "b"),
    event(tokens, "c"),
    {
      id: "u",
      timestamp: "",
      visibility: { visibility: "public" },
      event: { kind: "phase_transition", phase: "agenda", round: 3 },
    },
  ] as GameEvent[]);

  it("is the objectives scored by whom, the objectives revealed and the command tokens of each seat", () => {
    const [status] = selectLog(update, events).entries.flatMap((entry) =>
      entry.kind === "phase" && entry.phase === "status" ? [entry] : [],
    );
    const lines = status.groups.find((group) => group.title === "Scored")!.lines;
    expect(lines).toHaveLength(2);
    const past = selectPastPhase(update, events, status.id)!;
    expect(past.action.heading).toBe("Status phase");
    // The rows of "Scored" are the lines of its group of the Log, each one "Scored".
    const scored = seatsOf(past.action.blocks, "Scored");
    expect(scored.map((row) => row.seat)).toEqual(["b", "a"]);
    for (const [at, row] of scored.entries()) {
      expect(row.status).toEqual(expect.objectContaining({ label: "Scored" }));
      expect(row.text).toBe(lines[at].text);
    }
    expect(scored[1].text).toMatch(/secret objective$/);
    // With no catalogue the revealed objective is its name.
    expect(past.action.blocks).toContainEqual(
      expect.objectContaining({ kind: "note", strong: "Revealed" }),
    );
    expect(seatsOf(past.action.blocks, "Command tokens")).toHaveLength(3);
  });
});

// The agenda phase is not reached by the game above: a phase of three seats, as the host tells it.
describe("an agenda phase of the Log, opened", () => {
  let counter = 0;
  const vote = (seat: string, fact: PublicFact): GameEvent => ({
    id: `g-${++counter}`,
    timestamp: "",
    visibility: { visibility: "public" },
    event: { kind: "decision_resolved" },
    actor: seat,
    round: 3,
    phase: "agenda",
    fact,
  });
  const resolved = (fact: PublicFact): GameEvent => ({
    id: `g-${++counter}`,
    timestamp: "",
    visibility: { visibility: "public" },
    event: { kind: "state_changed" },
    round: 3,
    phase: "agenda",
    fact,
  });
  const update = {
    viewer: { role: "player", seat: "a" },
    seats: {},
    view: {
      round: 3,
      phase: "agenda",
      players: [
        { id: "a", faction: "sol" },
        { id: "b", faction: "hacan" },
        { id: "c", faction: "jolnar" },
      ],
      board: { systems: {}, map_tiles: [] },
    },
  } as unknown as SessionUpdate;
  const printed = {
    components: {
      faction_sheets: {},
      action_cards: [
        { id: "sabo1", name: "Sabotage", phase: "Any", window: "", group: "", text: "" },
      ],
      agendas: [
        {
          id: "mutiny",
          name: "Mutiny",
          kind: "Directive",
          elect: "For/Against",
          text: "For: each player who voted For gains 1 victory point.",
          against: "Against: each player who voted For loses 1 victory point.",
          group: "",
        },
      ],
    },
    records: { action_cards: [], promissory_notes: [], planets: [] },
  } as unknown as Catalog;

  it("is each agenda as printed, its outcome, and each seat's vote with its planets", () => {
    const events = eventsOf([
      {
        id: "t",
        timestamp: "",
        visibility: { visibility: "public" },
        event: { kind: "phase_transition", phase: "agenda", round: 3 },
      },
      vote("a", { kind: "card_played", card: "sabo1" }),
      vote("b", { kind: "voted", outcome: "for" }),
      vote("b", { kind: "planet_exhausted_for_vote", planet: "abyz", votes: 3 }),
      vote("c", { kind: "voted" }),
      vote("a", { kind: "voted", outcome: "against" }),
      vote("a", { kind: "planet_exhausted_for_vote", planet: "jord", votes: 2 }),
      resolved({
        kind: "agenda_resolved",
        agenda: "mutiny",
        outcome: "for",
        votes: { for: 3, against: 2 },
        speaker_decided: false,
        law: false,
        enacted: false,
        discarded: false,
      }),
    ] as GameEvent[]);
    const past = selectPastPhase(update, events, "agenda-3", printed)!;
    expect(past.action.heading).toBe("Agenda phase");
    expect(past.action.pill).toEqual({ tone: "live", label: "Played now" });
    const [source, outcome, votes] = past.action.blocks;
    expect(source).toMatchObject({
      kind: "source",
      name: "Mutiny",
      type: "Agenda · Directive · For/Against",
    });
    expect(outcome).toMatchObject({ kind: "note", strong: "For (For 3 · Against 2)" });
    expect(votes).toMatchObject({ kind: "seats", title: "Votes" });
    const rows = (votes as Extract<BlockView, { kind: "seats" }>).rows;
    expect(rows.map((row) => [row.seat, row.status.label, row.text])).toEqual([
      ["a", "Played", "Played Sabotage"],
      ["b", "For", "3 votes: Abyz"],
      ["c", "Abstained", undefined],
      ["a", "Against", "2 votes: Jord"],
    ]);
    expect(rows[0].name).toBe("You");
  });
});
