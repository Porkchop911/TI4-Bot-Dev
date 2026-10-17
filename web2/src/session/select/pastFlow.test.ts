import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, expect, it } from "vitest";
import type { BlockView, LogTurnView, SeatRowView } from "../../model";
import type { TransportEvent } from "../transport";
import { type Engine, createWasmTransport } from "../wasmTransport";
import type { EventLog } from "../events";
import type { LocalState } from "./action";
import { selectLog } from "./log";
import { selectPastFlow } from "./pastFlow";
import { selectShell } from "./shell";

// The real engine, built by `scripts/build-wasm.sh` (it is not in the repository).
const WASM = fileURLToPath(new URL("../ti4.wasm", import.meta.url));
const built = existsSync(WASM);

type Update = Extract<TransportEvent, { kind: "update" }>;

/** A game of six seats on seed 3, seat a played here with the first option of 80 questions. */
async function game(): Promise<Update[]> {
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
  for (let answer = 0; answer < 80; answer += 1) {
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

let played: Update;
/** Every update of the game, in order. */
let updates: Update[];
let events: EventLog;
/** Every turn of round 1, as a spectator sees the Log of the round. */
let turns: LogTurnView[];
beforeAll(async () => {
  if (!built) {
    return;
  }
  updates = await game();
  played = updates.at(-1)!;
  events = played.events ?? [];
  const round = {
    ...played.update,
    viewer: { role: "spectator" },
    view: { ...played.update.view, round: 1 },
  } as Update["update"];
  turns = selectLog(round, events).entries.flatMap((entry) =>
    entry.kind === "turn" ? [entry] : [],
  );
}, 60_000);

const turnOf = (label: string) => turns.find((turn) => turn.label === label)!;
const seatsOf = (blocks: BlockView[]): SeatRowView[] =>
  blocks.flatMap((block) => (block.kind === "seats" ? block.rows : []));

it.skipIf(!built)(
  "a strategic turn opens to its card and a row for each seat: what it did in short, and line by line one click further",
  () => {
    const past = selectPastFlow(played.update, events, turnOf("Strategic · Warfare").id)!;
    expect(past.action.heading).toBe("Warfare");
    expect(past.action.context).toMatch(/^Strategic action · /);
    expect(past.action.pill).toEqual({ tone: "done", label: "Resolved" });
    // Nothing of it is sent: its footer has no button.
    expect(past.action.footer.actions).toEqual([]);
    const rows = seatsOf(past.action.blocks);
    // The primary first, with what its seat did.
    expect(rows[0]).toMatchObject({ order: "P", status: { label: "Resolved" } });
    expect(rows[0].details).toContain("Gained 1 fleet token");
    // A seat that followed and produced says where, and its lines say what.
    const producer = rows.find((row) => /^produced \d+ units at /.test(row.text ?? ""))!;
    expect(producer.status).toMatchObject({ sign: "✓", label: "Followed" });
    expect(producer.details!.some((line) => line.startsWith("Produced on "))).toBe(true);
    // Every other seat is a row, in the order from the primary that the game asks them; a seat
    // that did not follow has "Passed".
    const ids = played.update.view.players.map((player) => player.id);
    const at = ids.indexOf(rows[0].seat);
    expect(rows.slice(1).map((row) => row.seat)).toEqual([
      ...ids.slice(at + 1),
      ...ids.slice(0, at),
    ]);
    expect(rows.slice(1).map((row) => row.order)).toEqual(
      rows.slice(1).map((_, index) => `${index + 1}`),
    );
    expect(rows.some((row) => row.status.label === "Passed")).toBe(true);
    // A row opens only to what its summary does not say already.
    for (const row of rows) {
      if (row.details) {
        expect(row.details.join(" · ").toLowerCase()).not.toBe(row.text?.toLowerCase());
      }
    }

    const technology = seatsOf(
      selectPastFlow(played.update, events, turnOf("Strategic · Technology").id)!.action.blocks,
    );
    expect(technology[0].text).toMatch(/^researched /);
    expect(technology.find((row) => row.status.label === "Followed")?.text).toMatch(/^researched /);
  },
);

it.skipIf(!built)(
  "a component turn opens to its card and what it did as the chosen rows, with what the board says of it",
  () => {
    const component = turns.find((turn) => turn.label === "Component · Xxchaagent")!;
    const past = selectPastFlow(played.update, events, component.id)!;
    expect(past.action.heading).toBe("Xxchaagent");
    expect(past.action.context).toMatch(/^Component action · /);
    const menus = past.action.blocks.flatMap((block) => (block.kind === "menu" ? [block] : []));
    expect(menus.length).toBeGreaterThan(0);
    // Every row is the choice that was made: selected, and no control.
    for (const row of menus.flatMap((menu) => menu.rows)) {
      expect(row).toMatchObject({ selected: true, disabled: true });
    }
    // The seat of the turn first, with no title; the others by name.
    expect(menus[0].title).toBeUndefined();
    const lines = menus.flatMap((menu) => menu.rows.map((row) => row.title));
    // What no fact tells is what the board says: a pool "now → after", a planet readied.
    expect(lines.some((line) => /^(Trade goods|Commodities) \d+ → \d+$/.test(line))).toBe(true);
    expect(lines.some((line) => line.startsWith("Readied "))).toBe(true);
  },
);

it.skipIf(!built)("a pass and a tactical turn are no strategic or component turn", () => {
  const pass = turns.find((turn) => turn.action === "pass")!;
  expect(selectPastFlow(played.update, events, pass.id)).toBeNull();
  const tactical = turns.find((turn) => turn.action === "tactical")!;
  expect(selectPastFlow(played.update, events, tactical.id)).toBeNull();
  expect(selectPastFlow(played.update, events, "nothing")).toBeNull();
});

it.skipIf(!built)(
  "the shell has a strategic turn in place of the step sheet, and the board frames the system of Diplomacy",
  () => {
    const turn = turnOf("Strategic · Diplomacy");
    const local = {
      inspected: null,
      staged: null,
      sent: null,
      error: null,
      replaying: null,
      canUndo: false,
      movement: {},
      landing: {},
      landingKind: null,
      production: { purchases: [], payment: {} },
      productionStage: "build",
      handled: [],
      step: null,
      remaining: null,
      planNote: null,
      stepping: null,
      draftLocked: null,
      draft: null,
      events,
      inspect: { turn: turn.id, step: null },
    } as unknown as LocalState;
    const view = selectShell(played.update, local);
    expect(view.inspect?.action).toMatchObject({ kind: "flow", heading: "Diplomacy" });
    const system = view.board.marked!;
    expect(system).toBeTruthy();
    // The system and the systems next to it are framed.
    expect(view.board.taskSystems).toContain(system);
    expect(view.board.taskSystems.length).toBeGreaterThan(1);
    expect(view.board.fitKey).toBe(`turn:${turn.id}:${system}`);
  },
);

it.skipIf(!built)(
  "the strategic turn of another seat that is played now opens with who decides and who waits, and stays open as resolved",
  () => {
    // The viewer is asked a secondary while another seat plays its strategic card.
    const at = updates.findIndex((update) => {
      const live = selectLog(update.update, update.events ?? []).entries.find(
        (entry) => entry.live,
      );
      return live?.kind === "turn" && live.action === "strategic" && live.seat !== "a";
    });
    const now = updates[at];
    const live = selectLog(now.update, now.events ?? []).entries.find((entry) => entry.live)!;
    const past = selectPastFlow(now.update, now.events ?? [], live.id)!;
    expect(past.action.pill).toEqual({ tone: "live", label: "Played now" });
    const rows = seatsOf(past.action.blocks);
    expect(rows.find((row) => row.seat === "a")?.status.label).toBe("Deciding");
    // Later the same turn is resolved, and it opens the same way.
    const later = updates.at(-1)!;
    const resolved = selectPastFlow(later.update, later.events ?? [], live.id)!;
    expect(resolved.action.pill).toEqual({ tone: "done", label: "Resolved" });
    expect(seatsOf(resolved.action.blocks).find((row) => row.seat === "a")?.status.label).toMatch(
      /^(Followed|Passed)$/,
    );
  },
);
