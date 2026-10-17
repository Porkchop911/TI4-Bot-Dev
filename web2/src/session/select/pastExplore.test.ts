import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { loadCatalog } from "@ti4/shared/content/catalog.ts";
import { expect, it } from "vitest";
import type { TransportEvent } from "../transport";
import { type Engine, createWasmTransport, engineQuery } from "../wasmTransport";
import { selectLog } from "./log";
import { selectPastTurn } from "./pastTurn";

// The real engine, built by `scripts/build-wasm.sh` (it is not in the repository).
const WASM = fileURLToPath(new URL("../ti4.wasm", import.meta.url));
const built = existsSync(WASM);

type Update = Extract<TransportEvent, { kind: "update" }>;

/**
 * A game of three seats on seed 1, seat a played here with a spread of options, until the first
 * exploration of the game and then a few answers more.
 */
async function explored() {
  const module = await WebAssembly.compile(readFileSync(WASM));
  const catalog = await loadCatalog({ query: engineQuery(module) });
  const engine: Engine = { module, hash: "test", scenarios: [], catalog };
  const transport = createWasmTransport(engine, { seed: 1, players: 3, humans: 1 }, {});
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
  let after = -1;
  for (let answer = 0; answer < 500 && after !== 0; answer += 1) {
    while (seen.length <= read) {
      await new Promise<void>((resume) => (wake = resume));
    }
    const last = seen.at(-1)!;
    read = seen.length;
    if (after < 0 && (last.events ?? []).some((event) => event.fact?.kind === "explored")) {
      after = 3;
    }
    after -= after > 0 ? 1 : 0;
    const pending = last.update.pending_choice;
    if (pending && after !== 0) {
      const options = pending.choice.options;
      transport.submitChoice(pending.nonce, options[(answer * 7) % options.length].id);
    }
  }
  return { last: seen.at(-1)!, catalog };
}

it.skipIf(!built)(
  "an exploration that asks nothing is in the row of its turn and in the step of its turn that it came in, its card as printed and what came of it",
  async () => {
    const { last, catalog } = await explored();
    const { update, events = [] } = last;
    const fact = events.find((event) => event.fact?.kind === "explored");
    expect(fact?.fact).toMatchObject({ kind: "explored", card: "dmz", planet: "primor" });
    const turn = fact!.action_id!;
    // Demilitarized Zone attaches itself: the engine asks nothing.
    const record = update.exploration?.drawn.find((item) => item.card === "dmz");
    expect(record?.outcome).toBe("attached:dmz");

    // The row of the turn says it.
    const row = selectLog(update, events, catalog).entries.find(
      (entry) => entry.kind === "turn" && entry.id === turn,
    );
    expect(row?.kind === "turn" && row.summary).toContain("explored Primor: Demilitarized Zone");

    // The opened turn has the card in the step of the invasion, as printed, and what came of it.
    const past = selectPastTurn(
      update,
      events,
      { turn, step: null },
      catalog.exploration,
      catalog,
    )!;
    expect(past.action.task.title).toBe("Invasion");
    expect(past.action.closing).toEqual([
      {
        kind: "source",
        name: "Demilitarized Zone",
        type: "Cultural exploration · Primor",
        text: expect.stringContaining("Attach"),
      },
      { kind: "note", tone: "quiet", text: "Attached Demilitarized Zone" },
    ]);
    // The board of the step has the attachment on the planet.
    const primor = Object.values(past.update.view.board.systems)
      .flatMap((system) => Object.values(system.planets))
      .find((planet) => planet.planet_id === "primor");
    expect(primor?.attachments).toContain("dmz");
  },
  120_000,
);
