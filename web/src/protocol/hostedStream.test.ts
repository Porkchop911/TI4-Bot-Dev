import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { decodeServerMessage } from "./decode.ts";
import { reduceServerMessage, type GameSessionState } from "./client.ts";

/**
 * The messages of a game played by the wasm host (`crates/ti4-wasm`, "The hosted game") decode
 * with the page's own decoder and reduce to a game state, as the server's do.
 *
 *   node crates/ti4-wasm/js/host.mjs target/wasm32-unknown-unknown/wasm-release/ti4_wasm.wasm \
 *     --record /tmp/stream.json
 *   TI4_HOSTED_STREAM=/tmp/stream.json npx vitest run src/protocol/hostedStream.test.ts
 *
 * Skipped without TI4_HOSTED_STREAM (the file needs the built wasm, and is about 7 MB).
 */
const path = process.env.TI4_HOSTED_STREAM;

const blank: GameSessionState = {
  status: "connecting",
  gameVersion: 0,
  snapshot: null,
  pendingChoice: null,
  turnStatus: null,
  lastError: null,
  fatal: null,
  reconnecting: false,
  events: [],
  history: { cursor: 0, redo_count: 0 },
};

describe.skipIf(!path)("a hosted game", () => {
  it("decodes and reduces like a game from the server", () => {
    const stream = JSON.parse(readFileSync(path as string, "utf8")) as {
      game_id: string;
      messages: unknown[];
    };
    let state = blank;
    const kinds = new Set<string>();
    let snapshots = 0;
    for (const raw of stream.messages) {
      const message = decodeServerMessage(raw, stream.game_id);
      kinds.add(message.type);
      if (message.type === "initial_snapshot") snapshots += 1;
      state = reduceServerMessage(state, message);
    }
    for (const kind of ["initial_snapshot", "state_update", "event", "action_accepted"])
      expect(kinds.has(kind)).toBe(true);
    // The recording has an undo in it: a second snapshot, one history generation later.
    expect(snapshots).toBe(2);
    expect(state.history.generation).toBe(1);
    expect(state.snapshot).not.toBeNull();
    expect(state.events.length).toBeGreaterThan(50);
    expect(state.pendingChoice?.actor ?? "a").toBe("a");
    expect(state.lastError).toBeNull();
  });
});
