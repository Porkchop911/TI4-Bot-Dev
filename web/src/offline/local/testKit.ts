import { PROTOCOL_VERSION } from "../../protocol/types.ts";
import type { Envelope, HostSave } from "@ti4/shared/engine/host.ts";
import type { FromWorker, HostPort, ToWorker } from "@ti4/shared/engine/protocol.ts";

/** Messages and a fake worker for the tests of the local transport (no wasm needed). */
export const GAME = "off_test";
export const SEAT = "a";

const base = { protocol_version: PROTOCOL_VERSION, game_id: GAME };

const view = {
  round: 1,
  phase: "action",
  speaker: SEAT,
  seating_order: [SEAT, "b", "c"],
  finished: false,
  players: [],
  board: { systems: {} },
  table: {
    revealed_objectives: [],
    scored_objectives: {},
    unclaimed_strategy_cards: [],
    strategy_card_goods: {},
    laws: {},
  },
};

export function question(version: number, nonce: string, subtype = "movement_step", options = ["o1", "o2"]) {
  return {
    type: "state_update",
    ...base,
    game_version: version,
    viewer: { role: "player", seat: SEAT },
    view,
    state: {},
    galaxy_layout: { version: 1, active_sources: [], placements: [] },
    turn_status: { kind: "waiting_for_decision", seat: SEAT },
    pending_choice: {
      nonce,
      choice: {
        player: SEAT,
        prompt: "Choose",
        context: { subtype },
        options: options.map((id) => ({ id, label: id })),
      },
    },
    history: { cursor: version, redo_count: 0, generation: 0 },
  };
}

export const snapshot = (version: number, nonce: string, generation = 0) => ({
  ...question(version, nonce),
  type: "initial_snapshot",
  events: [],
  history: { cursor: version, redo_count: 0, generation },
});

export const accepted = (version: number, option: string) => ({
  type: "action_accepted",
  ...base,
  game_version: version,
  option_id: option,
});

export const rejected = (version: number) => ({
  type: "action_rejected",
  ...base,
  game_version: version,
  reason: { reason: "stale_nonce" },
});

export const envelope = (kind: Envelope["kind"], fields: Partial<Envelope> = {}): Envelope => ({
  kind,
  messages: [],
  ...fields,
});

export const save = (answers: string[] = [], finished = false): HostSave => ({
  format: 2,
  seed: 7,
  players: 3,
  game: { setup: { game_id: GAME, seed: 7, players: 3, human_seat: 0 } },
  humans: 1,
  decisions: answers,
  answers,
  switches: [],
  redo: [],
  never: [],
  event_times: [],
  event_versions: [],
  version: 1,
  generation: 0,
  decisions_hash: "h",
  finished,
  round: 1,
  phase: "action",
  engine: { build: "b", content_digest: "d" },
});

/** A worker the test drives by hand. `onCommand` answers a command like the engine's reply would. */
export class FakePort implements HostPort {
  posted: ToWorker[] = [];
  terminated = false;
  private listener: (message: FromWorker) => void = () => undefined;
  onCommand: (request: Record<string, unknown>, id: number) => FromWorker | null = () => null;

  post(message: ToWorker): void {
    this.posted.push(message);
    if (message.t === "command") {
      const reply = this.onCommand(message.request as Record<string, unknown>, message.id);
      if (reply) queueMicrotask(() => this.listener(reply));
    }
  }
  listen(listener: (message: FromWorker) => void): void {
    this.listener = listener;
  }
  terminate(): void {
    this.terminated = true;
  }
  /** The engine sends something (a question, progress, an end). */
  emit(message: FromWorker): void {
    this.listener(message);
  }
  commands(type: string): Record<string, unknown>[] {
    return this.posted
      .filter((m): m is Extract<ToWorker, { t: "command" }> => m.t === "command")
      .map((m) => m.request as Record<string, unknown>)
      .filter((request) => request.type === type);
  }
}
