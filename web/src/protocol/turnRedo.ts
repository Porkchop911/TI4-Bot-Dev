/**
 * Turn redo (H7): a casual-play "replay my last turn". Mirrors `ti4-server/src/protocol/turn_redo.rs`.
 *
 * The game rewinds to the start of the seat's turn (same fixed seed), the seat plays a new turn
 * live, then the round auto-plays from the recorded decisions (the other seats', and the redoing
 * seat's own that are not a turn start) until the redoing seat's next turn begins or the first
 * decision that no longer fits. Cards keep their identity: a recorded draw gives the seat the
 * card it got originally. The timeline as it was before the redo is kept on the server and can
 * be restored.
 */

export type TurnRedoCommand =
  /** The seat defaults to the requester; naming another seat is host-only. */
  | { action: "request"; seat?: string; turns?: 1 | 2 }
  | { action: "autoplay" }
  | { action: "restore" }
  | { action: "keep" };

export type TurnRedoStage = "new_turn" | "auto_played";

export type TurnRedoConflictKind =
  | "actor"
  | "prompt"
  | "context"
  | "chosen_not_offered"
  | "options_changed"
  | "quantity_changed"
  | "engine_ended"
  | "engine_error"
  | "reserved_card";

const CONFLICT_KINDS: readonly string[] = [
  "actor",
  "prompt",
  "context",
  "chosen_not_offered",
  "options_changed",
  "quantity_changed",
  "engine_ended",
  "engine_error",
  "reserved_card",
];

export interface TurnRedoConflict {
  /** Index in the original timeline of the decision that did not fit. */
  original_cursor: number;
  kind: TurnRedoConflictKind;
  /** The seat that is asked live now. */
  seat: string;
  prompt: string;
  detail: string;
  /** `reserved_card` only: the deck (`action_card`, `secret`, `exploration:<kind>`, ...). */
  deck: string | null;
  /**
   * `reserved_card` only: the card that was reserved. Absent for a card of a hidden deck (action
   * card, secret objective) when the viewer is not the seat it was reserved for.
   */
  card: string | null;
  /** `reserved_card` only: the seat the card was reserved for. */
  recipient: string | null;
}

export type TurnRedoStop =
  | { kind: "handoff"; seat: string }
  | { kind: "conflict"; conflict: TurnRedoConflict }
  | { kind: "tail_exhausted" };

export interface TurnRedoOutcome {
  /** Recorded decisions that were replayed and kept (other seats', and the seat's own non-turn ones). */
  kept: number;
  /** How many recorded decisions lay between the redone turn and the seat's next turn. */
  tail_total: number;
  stop: TurnRedoStop;
  asking_seat: string | null;
}

export interface TurnRedoStatus {
  seat: string;
  requested_by: string;
  turns_back: number;
  redo_count: number;
  stage: TurnRedoStage;
  original_decisions: number;
  rewound_to: number;
  /** `new_turn` only: the new turn is complete, so auto-play can run now. */
  turn_complete: boolean;
  handoff_len: number | null;
  outcome: TurnRedoOutcome | null;
  /** The viewer is the host or the redoing seat: may auto-play, restore and keep. */
  can_control: boolean;
}

function fail(message: string): never {
  throw new Error(`Invalid turn redo status: ${message}`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

const isCount = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0;

const optionalString = (value: unknown, what: string): string | null => {
  if (value === undefined || value === null) return null;
  if (typeof value !== "string") fail(`${what} must be a string`);
  return value;
};

function decodeStop(value: unknown): TurnRedoStop {
  if (!isRecord(value)) fail("invalid stop");
  if (value.kind === "tail_exhausted") return { kind: "tail_exhausted" };
  if (value.kind === "handoff" && typeof value.seat === "string")
    return { kind: "handoff", seat: value.seat };
  if (value.kind === "conflict" && isRecord(value.conflict)) {
    const c = value.conflict;
    if (
      !isCount(c.original_cursor) ||
      typeof c.kind !== "string" ||
      !CONFLICT_KINDS.includes(c.kind) ||
      typeof c.seat !== "string" ||
      typeof c.prompt !== "string" ||
      typeof c.detail !== "string"
    )
      fail("invalid conflict");
    return {
      kind: "conflict",
      conflict: {
        original_cursor: c.original_cursor,
        kind: c.kind as TurnRedoConflictKind,
        seat: c.seat,
        prompt: c.prompt,
        detail: c.detail,
        deck: optionalString(c.deck, "deck"),
        card: optionalString(c.card, "card"),
        recipient: optionalString(c.recipient, "recipient"),
      },
    };
  }
  return fail("unknown stop kind");
}

function decodeOutcome(value: unknown): TurnRedoOutcome {
  if (!isRecord(value) || !isCount(value.kept) || !isCount(value.tail_total)) fail("invalid outcome");
  if (value.asking_seat !== null && value.asking_seat !== undefined && typeof value.asking_seat !== "string")
    fail("invalid asking seat");
  return {
    kept: value.kept,
    tail_total: value.tail_total,
    stop: decodeStop(value.stop),
    asking_seat: (value.asking_seat as string | null | undefined) ?? null,
  };
}

export function decodeTurnRedoStatus(value: unknown): TurnRedoStatus {
  if (!isRecord(value)) fail("not an object");
  if (
    typeof value.seat !== "string" ||
    typeof value.requested_by !== "string" ||
    !isCount(value.turns_back) ||
    !isCount(value.redo_count) ||
    (value.stage !== "new_turn" && value.stage !== "auto_played") ||
    !isCount(value.original_decisions) ||
    !isCount(value.rewound_to) ||
    typeof value.turn_complete !== "boolean" ||
    typeof value.can_control !== "boolean"
  )
    fail("invalid fields");
  const handoff = value.handoff_len;
  if (handoff !== null && handoff !== undefined && !isCount(handoff)) fail("invalid handoff_len");
  return {
    seat: value.seat,
    requested_by: value.requested_by,
    turns_back: value.turns_back,
    redo_count: value.redo_count,
    stage: value.stage,
    original_decisions: value.original_decisions,
    rewound_to: value.rewound_to,
    turn_complete: value.turn_complete,
    handoff_len: handoff ?? null,
    outcome:
      value.outcome === null || value.outcome === undefined ? null : decodeOutcome(value.outcome),
    can_control: value.can_control,
  };
}

/** `GET /api/games/{id}/turn-redo`: `{ status: null }` when no redo is in flight. */
export function decodeTurnRedoStatusResponse(value: unknown): TurnRedoStatus | null {
  if (!isRecord(value) || !("status" in value)) fail("missing status");
  return value.status === null || value.status === undefined ? null : decodeTurnRedoStatus(value.status);
}
