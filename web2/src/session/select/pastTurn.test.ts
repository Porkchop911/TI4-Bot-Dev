import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";
import { boardAt } from "../history";
import type { TransportEvent } from "../transport";
import { type Engine, createWasmTransport } from "../wasmTransport";
import type { LocalState } from "./action";
import { pastTactical, selectPastTurn } from "./pastTurn";
import { selectShell } from "./shell";

// The real engine, built by `scripts/build-wasm.sh` (it is not in the repository).
const WASM = fileURLToPath(new URL("../ti4.wasm", import.meta.url));
const built = existsSync(WASM);

type Update = Extract<TransportEvent, { kind: "update" }>;

/** A game of a scenario, seat a played here with the first option of `answers` questions. */
async function game(scenario: string, answers = 25): Promise<Update> {
  const engine: Engine = {
    module: await WebAssembly.compile(readFileSync(WASM)),
    hash: "test",
    scenarios: [],
    catalog: null,
  };
  const transport = createWasmTransport(engine, { seed: 0, players: 3, humans: 1, scenario }, {});
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
  return seen.at(-1)!;
}

const local = (update: Update, turn: string, step: number | null = null): LocalState => ({
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
  inspect: { turn, step },
});

it.skipIf(!built)(
  "a space combat turn opens at its combat, with each round and its dice on the rows, and the board of each step",
  async () => {
    const { update, events = [] } = await game("ongoing_combat");
    const open = (step: number | null) =>
      pastTactical(selectPastTurn(update, events, { turn: "action_1", step }));
    const past = open(null);
    expect(past.system).toBe("29");
    expect(past.action.tabs.map((tab) => tab.name)).toEqual([
      "Activation",
      "Movement",
      "Space combat",
    ]);
    expect(past.action.tabs.every((tab) => tab.status === "done")).toBe(true);
    expect(past.action.tabs[2].caption).toBe("2 rounds");
    // The last step that the action came to is open.
    expect(past.action.selected).toBe(2);
    expect(past.action.task.pill).toEqual({ tone: "done", label: "Resolved" });
    expect(past.action.context).toMatch(/^Tactical action · .*29/);
    const combat = past.action.task.content;
    if (combat.kind !== "combat") {
      throw new Error("not a combat");
    }
    expect(combat.decision).toBeNull();
    expect(combat.records.map((record) => record.label)).toEqual([
      "Anti-fighter barrage",
      "Round 1",
      "Round 2",
    ]);
    // Every round has dice on its rows, and its last table says how the combat ended.
    for (const record of combat.records.slice(1)) {
      const dice = record.table.sides.flatMap((side) =>
        side.rows.flatMap((row) => (typeof row.dice === "number" ? [] : row.dice)),
      );
      expect(dice.length).toBeGreaterThan(0);
      expect(record.table.sides.every((side) => side.hits !== null)).toBe(true);
    }
    expect(combat.records.at(-1)!.table.score).toMatch(/^Won by /);

    // The movement: the ships that left #01, and the fleet in the active system before and after.
    const movement = open(1).action.task.content;
    if (movement.kind !== "movement") {
      throw new Error("not the movement");
    }
    expect(movement.origins.map((origin) => origin.system)).toEqual(["01"]);
    expect(movement.origins[0].leaves).toBe("4 ships · 3 cargo");
    expect(movement.arrival.some((row) => row.after > row.now)).toBe(true);
    // Each step has the board as it left it: after the movement the ships of a are in #29, and
    // after the combat the ships of c are gone.
    const ships = (step: number, seat: string) =>
      (open(step).update.view.board.systems["29"]?.units ?? []).filter(
        (unit) => unit.owner === seat && unit.planet === null,
      ).length;
    expect(ships(0, "a")).toBe(0);
    expect(ships(1, "a")).toBeGreaterThan(0);
    expect(ships(1, "c")).toBeGreaterThan(0);
    expect(ships(2, "c")).toBe(0);
    // The activation spent a tactic token.
    const activation = open(0).action.task.content;
    expect(activation).toMatchObject({ kind: "activation", editing: false, costsToken: true });
  },
  60_000,
);

it.skipIf(!built)(
  "an invasion turn has what landed on each planet, and a table for the space cannon defense and each ground round",
  async () => {
    const { update, events = [] } = await game("ongoing_invasion_coexistence");
    const past = pastTactical(selectPastTurn(update, events, { turn: "action_1", step: null }));
    expect(past.action.tabs.map((tab) => tab.name)).toEqual(["Activation", "Invasion"]);
    const invasion = past.action.task.content;
    if (invasion.kind !== "invasion") {
      throw new Error("not the invasion");
    }
    const qucenn = invasion.landing.planets.find((planet) => planet.id === "qucenn")!;
    expect(qucenn.tokens).toHaveLength(7);
    expect(qucenn.lands).toMatch(/^1 Mech.*, 6 Infantry/);
    expect(invasion.records?.map((record) => record.label)).toEqual([
      "Space cannon defense",
      "Round 1",
      "Round 2",
    ]);
    const last = invasion.records!.at(-1)!.table;
    expect(last.score).toMatch(/^Won by /);
    expect(last.sides.map((side) => side.role)).toEqual(["Attacker", "Defender"]);
    expect(
      last.sides.some((side) =>
        side.rows.some((row) => typeof row.dice !== "number" && row.dice.length > 0),
      ),
    ).toBe(true);
    // After the invasion the planet is of a.
    expect(past.update.view.board.systems["29"].planets.qucenn?.controlled_by).toBe("a");
    expect(
      boardAt(update.view, events, 0).board.systems["29"].planets.qucenn?.controlled_by,
    ).not.toBe("a");
  },
  60_000,
);

it.skipIf(!built)(
  "the shell has the turn in place of the step sheet, and the board of its step with its system marked",
  async () => {
    const played = await game("ongoing_combat");
    const view = selectShell(played.update, local(played, "action_1", 1));
    expect(view.inspect?.turn).toBe("action_1");
    expect(view.inspect?.action.kind).toBe("tactical");
    expect(view.board.marked).toBe("29");
    // Nothing of the viewer's decision is on the board of the turn.
    expect(view.board.task ?? null).toBeNull();
    // A turn that is no tactical action, or that the events no longer have, opens nothing.
    expect(selectShell(played.update, local(played, "nothing")).inspect).toBeNull();
    expect(selectShell(played.update, local(played, "action_8")).inspect ?? null).toBeNull();
  },
  60_000,
);
