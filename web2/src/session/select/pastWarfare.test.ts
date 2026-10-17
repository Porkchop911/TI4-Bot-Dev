import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { loadCatalog } from "@ti4/shared/content/catalog.ts";
import { expect, it } from "vitest";
import type { MovementFacts } from "../wire";
import { setLoad, setShip, shipId } from "../movementDraft";
import { stepsOf } from "../movementPlan";
import type { TransportEvent } from "../transport";
import { type Engine, createWasmTransport, engineQuery } from "../wasmTransport";
import { selectPastTurn } from "./pastTurn";

// The real engine, built by `scripts/build-wasm.sh` (it is not in the repository).
const WASM = fileURLToPath(new URL("../ti4.wasm", import.meta.url));
const built = existsSync(WASM);

type Update = Extract<TransportEvent, { kind: "update" }>;

/**
 * The scenario of Warfare of Thunder's Edge, Sol played here: Sol plays Warfare, redistributes its
 * tokens, takes the tactical action of the card in #71, moves its carrier with two infantry and a
 * destroyer, lands both infantry on Bakal (which it explores), and ends its turn; the other seats
 * answer the secondary.
 */
async function warfare() {
  const module = await WebAssembly.compile(readFileSync(WASM));
  const catalog = await loadCatalog({ query: engineQuery(module) });
  const engine: Engine = { module, hash: "test", scenarios: [], catalog };
  const transport = createWasmTransport(
    engine,
    { seed: 0, players: 3, humans: 1, scenario: "strategy_warfare_te" },
    {},
  );
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
  let landed = 0;
  for (let answer = 0; answer < 10; answer += 1) {
    while (seen.length <= read) {
      await new Promise<void>((resume) => (wake = resume));
    }
    read = seen.length;
    const { update } = seen.at(-1)!;
    const pending = update.pending_choice;
    if (!pending) {
      continue;
    }
    const options = pending.choice.options.map((option) => option.id);
    const facts = update.tactical as MovementFacts | undefined;
    if (facts && "ships" in facts && facts.ships.some((ship) => ship.move)) {
      let draft = {};
      for (const ship of facts.ships.filter((item) => item.move)) {
        draft = setShip(facts, draft, shipId(ship), true);
      }
      const carrier = facts.ships.find((ship) => ship.move && ship.capacity > 0)!;
      const infantry = facts.cargo.findIndex((pool) => pool.unit === "sol_infantry");
      draft = setLoad(facts, draft, shipId(carrier), infantry, 2);
      transport.submitPlan(pending.nonce, { destination: "71", steps: stepsOf(facts, draft) });
      continue;
    }
    const pick =
      options.find((id) => id === "71" && pending.choice.prompt.includes("without")) ??
      (landed < 2 && options.includes("commit|0|bakal") ? (landed++, "commit|0|bakal") : null) ??
      options.find((id) => id === "strategic" || id === "end_turn") ??
      options[0];
    transport.submitChoice(pending.nonce, pick);
  }
  return { last: seen.at(-1)!, catalog };
}

it.skipIf(!built)(
  "a strategic turn with a tactical action has the card first, then the steps of its tactical action, and the card does not say the steps again",
  async () => {
    const { last, catalog } = await warfare();
    const { update, events = [] } = last;
    const started = events.find(
      (event) => event.fact?.kind === "action_started" && event.fact.action.type === "strategic",
    )!;
    const turn = started.action_id!;
    const open = (step: number | null) =>
      selectPastTurn(update, events, { turn, step }, catalog.exploration, catalog)!;

    // The card: its seats, with the tabs of the tactical action after it.
    const card = open(null);
    expect(card.system).toBe("71");
    expect(card.action.tabs.map((tab) => tab.name)).toEqual([
      "Warfare",
      "Activation",
      "Movement",
      "Invasion",
    ]);
    expect(card.action.selected).toBe(0);
    expect(card.action.context).toMatch(/^Strategic action · .*#71/);
    if (card.action.kind !== "flow") {
      throw new Error("the card is no list of seats");
    }
    const seats = card.action.blocks.find((block) => block.kind === "seats");
    if (seats?.kind !== "seats") {
      throw new Error("no seats");
    }
    const primary = seats.rows[0];
    expect(primary.order).toBe("P");
    // The primary's own parts once; the steps of its tactical action are in their tabs.
    const lines = primary.details ?? [primary.text ?? ""];
    expect(lines.filter((line) => line.startsWith("Redistributed"))).toHaveLength(1);
    expect(lines.join(" ")).not.toMatch(/Moved|Landed|Loaded|Activated|Round|Explored|Took/);
    // The secondaries keep what they did.
    expect(seats.rows.slice(1).some((row) => /produced/i.test(row.text ?? ""))).toBe(true);

    // The steps, as an opened tactical turn has them, with the card's text behind "?".
    const movement = open(2);
    expect(movement.action.selected).toBe(2);
    if (movement.action.kind !== "tactical") {
      throw new Error("the step is no tactical step");
    }
    expect(movement.action.task.title).toBe("Movement");
    expect(movement.action.task.help.length).toBeGreaterThan(0);
    expect(movement.action.context).toBe(card.action.context);
    const moved = movement.action.task.content;
    expect(moved.kind === "movement" && moved.origins.map((origin) => origin.leaves)).toEqual([
      "3 ships · 2 cargo",
    ]);
    expect(movement.update.view.board.systems["71"]?.units.length).toBeGreaterThan(0);

    const invasion = open(3);
    if (invasion.action.kind !== "tactical") {
      throw new Error("the step is no tactical step");
    }
    const landing = invasion.action.task.content;
    expect(
      landing.kind === "invasion" &&
        landing.landing.planets.find((planet) => planet.name === "Bakal")?.lands,
    ).toBe("2 Spec Ops I");
    // The exploration of Bakal is with the step that it came in.
    expect(invasion.action.closing[0]).toMatchObject({ kind: "source" });
  },
  120_000,
);
