import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, expect, it } from "vitest";
import type { LogTurnView } from "../../model";
import type { TransportEvent } from "../transport";
import { type Engine, createWasmTransport } from "../wasmTransport";
import type { LocalState } from "./action";
import { selectNow } from "./liveLine";
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

const local = (update: Update, inspect: LocalState["inspect"] = null): LocalState => ({
  inspected: null,
  staged: null,
  sent: null,
  error: null,
  replaying: null,
  canUndo: false,
  movement: {},
  landing: {},
  landingKind: null,
  production: { purchases: [], payment: {} } as unknown as LocalState["production"],
  productionStage: "build",
  handled: [],
  step: null,
  remaining: null,
  planNote: null,
  stepping: null,
  draftLocked: null,
  draft: null,
  events: update.events ?? [],
  inspect,
});

let updates: Update[];
beforeAll(async () => {
  if (built) {
    updates = await game();
  }
}, 60_000);

/** The turn of another seat that is played now in an update, if any. */
const liveOther = (update: Update): LogTurnView | null => {
  const live = selectShell(update.update, local(update)).log.entries.findLast(
    (entry) => entry.live,
  );
  return live?.kind === "turn" && !live.mine ? live : null;
};

it.skipIf(!built)(
  "while another seat plays, a line over the board says its turn, and its row says the step",
  () => {
    const at = updates.find((update) => liveOther(update));
    expect(at).toBeDefined();
    // While the game asks the viewer (a secondary, a window), the decision is the one place:
    // no line.
    expect(at!.update.pending_choice).toBeTruthy();
    expect(selectShell(at!.update, local(at!)).live).toBeNull();
    // Once the viewer answered, the game waits for no decision of the viewer: the line is there.
    const view = selectShell(at!.update, {
      ...local(at!),
      sent: at!.update.pending_choice!.nonce,
    });
    const row = liveOther(at!)!;
    expect(view.live?.turn).toBe(row.id);
    expect(view.live?.text).toContain(row.label);
    // The line names the seat, as the toolbar does.
    expect(view.live?.text.split(" · ")[0]).not.toBe("");
    // No line while the viewer plays its own turn.
    const own = updates.find((update) => {
      const live = selectShell(update.update, local(update)).log.entries.findLast(
        (entry) => entry.live,
      );
      return live?.kind === "turn" && live.mine;
    });
    if (own) {
      expect(selectShell(own.update, local(own)).live).toBeNull();
    }
  },
);

it.skipIf(!built)(
  "Watch opens the turn that is played now over the live board, in the present, and closes when the turn ends",
  () => {
    const index = updates.findIndex((update) => liveOther(update));
    const now = updates[index];
    const turn = liveOther(now)!.id;
    const watching = selectShell(now.update, local(now, { turn, step: null, watch: true }));
    expect(watching.inspect?.watch).toBe(true);
    expect(watching.inspect?.turn).toBe(turn);
    // The board is the live one: the same as with nothing open, but for the marked system.
    const plain = selectShell(now.update, local(now));
    expect(watching.board.tiles).toEqual(plain.board.tiles);
    expect(watching.toolbar.status).toBe(plain.toolbar.status);
    // One sheet at a time: Watch is what the line says, so the line is not there.
    expect(watching.live).toBeNull();
    // A later update where the turn is over: Watch is closed, the history of the same turn is not.
    const later = updates.slice(index).find((update) => liveOther(update)?.id !== turn)!;
    expect(
      selectShell(later.update, local(later, { turn, step: null, watch: true })).inspect,
    ).toBeNull();
    const opened = selectShell(later.update, local(later, { turn, step: null }));
    expect(opened.inspect?.watch).toBe(false);
    expect(opened.inspect?.turn).toBe(turn);
  },
);

it("the step of a space combat says its round and the ships of each side, and whom it waits for", () => {
  const unit = (owner: string, unit_type: string) => ({
    unit_type,
    owner,
    planet: null,
    damaged: false,
    galvanized: false,
  });
  const update = {
    viewer: { role: "player", seat: "a" },
    turn_status: { kind: "waiting_for_decision", seat: "c", phase: "action", round: 2, stage: "" },
    seats: { b: { nickname: "Hacan" }, c: { nickname: "Sol" } },
    view: {
      players: [],
      board: {
        systems: {
          "20": {
            system_id: "20",
            command_tokens: [],
            planets: {},
            units: [
              unit("b", "carrier"),
              unit("b", "cruiser"),
              unit("b", "infantry"),
              unit("b", "fighter"),
              unit("c", "dreadnought"),
              unit("c", "destroyer"),
            ],
          },
        },
        combat: {
          system_id: "20",
          round: 2,
          attacker: "b",
          defender: "c",
          remaining_hits: { c: 1 },
        },
      },
    },
  } as never;
  const entry = { kind: "turn", seat: "b", mine: false } as never;
  expect(selectNow(update, entry, "Movement", null)).toEqual({
    step: "Space combat · round 2",
    detail: "Waiting for Sol: casualties",
  });
  const free = {
    ...(update as object),
    turn_status: { kind: "active_turn", player: "b" },
  } as never;
  expect(selectNow(free, entry, "Movement", null)).toEqual({
    step: "Space combat · round 2",
    detail: "3 vs 2 ships",
  });
});
