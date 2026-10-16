/**
 * Read-only dry run of "remove (or change) one past decision, then replay the rest from the same
 * seed" (H7 phase 1). Mirrors `ti4-server/src/protocol/splice.rs`. No UI uses this yet.
 */

export type SpliceEdit =
  | { kind: "remove"; cursor: number }
  | { kind: "replace"; cursor: number; option_id: string };

export type ConflictKind =
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

export interface SpliceConflict {
  /** Index of the recorded decision in the ORIGINAL log. */
  cursor: number;
  kind: ConflictKind;
  /** Options changed or quantities changed: the earlier intent was made against another menu. */
  soft: boolean;
  seat: string;
  prompt: string;
  expected_chosen: string;
  expected_offered: string[];
  found_seat: string | null;
  found_prompt: string | null;
  found_offered: string[] | null;
  added: string[];
  removed: string[];
  detail: string;
  /** Removal only: the engine asks exactly the removed question again. */
  asks_removed_decision: boolean;
}

export interface AlignmentNote {
  change: "appeared" | "vanished";
  cursor: number;
  seat: string;
  prompt: string;
  option_id: string;
}

export type RngStatus = "neutral" | "not_neutral" | "unknown";

export interface RngCounters {
  dice_rolls: number;
  dice_faces: number;
  decks: Record<string, number>;
}

export interface RngIndicator {
  status: RngStatus;
  compared: number;
  removed_consumed: RngCounters | null;
  first_divergence_cursor: number | null;
  first_divergence: RngCounters | null;
}

export interface SplicePreview {
  edit: SpliceEdit;
  original_decisions: number;
  later_decisions: number;
  kept_later: number;
  dropped_later: number;
  survives_to_end: boolean;
  kept_reordered: number[];
  first_conflict: SpliceConflict | null;
  alignment: AlignmentNote[];
  rng: RngIndicator;
}

function fail(message: string): never {
  throw new Error(`Invalid splice preview: ${message}`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

const isCount = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0;

function strings(value: unknown, what: string): string[] {
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string"))
    fail(`${what} must be a list of strings`);
  return value as string[];
}

function nullableStrings(value: unknown, what: string): string[] | null {
  return value === null || value === undefined ? null : strings(value, what);
}

function nullableString(value: unknown, what: string): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value !== "string") fail(`${what} must be a string`);
  return value;
}

function decodeEdit(value: unknown): SpliceEdit {
  if (!isRecord(value) || !isCount(value.cursor)) fail("invalid edit");
  if (value.kind === "remove") return { kind: "remove", cursor: value.cursor };
  if (value.kind === "replace" && typeof value.option_id === "string")
    return { kind: "replace", cursor: value.cursor, option_id: value.option_id };
  return fail("unknown edit kind");
}

function decodeCounters(value: unknown, what: string): RngCounters {
  if (!isRecord(value) || typeof value.dice_rolls !== "number" || typeof value.dice_faces !== "number")
    fail(`invalid ${what}`);
  const decks: Record<string, number> = {};
  if (!isRecord(value.decks)) fail(`invalid ${what} decks`);
  for (const [deck, delta] of Object.entries(value.decks)) {
    if (typeof delta !== "number" || !Number.isFinite(delta)) fail(`invalid ${what} deck ${deck}`);
    decks[deck] = delta;
  }
  return { dice_rolls: value.dice_rolls, dice_faces: value.dice_faces, decks };
}

function decodeConflict(value: unknown): SpliceConflict {
  if (
    !isRecord(value) ||
    !isCount(value.cursor) ||
    typeof value.kind !== "string" ||
    !CONFLICT_KINDS.includes(value.kind) ||
    typeof value.soft !== "boolean" ||
    typeof value.seat !== "string" ||
    typeof value.prompt !== "string" ||
    typeof value.expected_chosen !== "string" ||
    typeof value.detail !== "string" ||
    typeof value.asks_removed_decision !== "boolean"
  )
    fail("invalid conflict");
  return {
    cursor: value.cursor,
    kind: value.kind as ConflictKind,
    soft: value.soft,
    seat: value.seat,
    prompt: value.prompt,
    expected_chosen: value.expected_chosen,
    expected_offered: strings(value.expected_offered, "expected_offered"),
    found_seat: nullableString(value.found_seat, "found_seat"),
    found_prompt: nullableString(value.found_prompt, "found_prompt"),
    found_offered: nullableStrings(value.found_offered, "found_offered"),
    added: strings(value.added, "added"),
    removed: strings(value.removed, "removed"),
    detail: value.detail,
    asks_removed_decision: value.asks_removed_decision,
  };
}

function decodeAlignment(value: unknown): AlignmentNote {
  if (
    !isRecord(value) ||
    (value.change !== "appeared" && value.change !== "vanished") ||
    !isCount(value.cursor) ||
    typeof value.seat !== "string" ||
    typeof value.prompt !== "string" ||
    typeof value.option_id !== "string"
  )
    fail("invalid alignment note");
  return {
    change: value.change,
    cursor: value.cursor,
    seat: value.seat,
    prompt: value.prompt,
    option_id: value.option_id,
  };
}

export function decodeSplicePreview(value: unknown): SplicePreview {
  if (!isRecord(value)) fail("not an object");
  if (
    !isCount(value.original_decisions) ||
    !isCount(value.later_decisions) ||
    !isCount(value.kept_later) ||
    !isCount(value.dropped_later) ||
    typeof value.survives_to_end !== "boolean"
  )
    fail("invalid counts");
  if (!Array.isArray(value.kept_reordered) || !value.kept_reordered.every(isCount))
    fail("invalid kept_reordered");
  if (!Array.isArray(value.alignment)) fail("invalid alignment");
  const rng = value.rng;
  if (
    !isRecord(rng) ||
    (rng.status !== "neutral" && rng.status !== "not_neutral" && rng.status !== "unknown") ||
    !isCount(rng.compared)
  )
    fail("invalid rng indicator");
  const divergenceCursor = rng.first_divergence_cursor;
  if (divergenceCursor !== null && divergenceCursor !== undefined && !isCount(divergenceCursor))
    fail("invalid rng divergence cursor");
  return {
    edit: decodeEdit(value.edit),
    original_decisions: value.original_decisions,
    later_decisions: value.later_decisions,
    kept_later: value.kept_later,
    dropped_later: value.dropped_later,
    survives_to_end: value.survives_to_end,
    kept_reordered: value.kept_reordered as number[],
    first_conflict:
      value.first_conflict === null || value.first_conflict === undefined
        ? null
        : decodeConflict(value.first_conflict),
    alignment: value.alignment.map(decodeAlignment),
    rng: {
      status: rng.status,
      compared: rng.compared,
      removed_consumed:
        rng.removed_consumed === null || rng.removed_consumed === undefined
          ? null
          : decodeCounters(rng.removed_consumed, "removed_consumed"),
      first_divergence_cursor: divergenceCursor ?? null,
      first_divergence:
        rng.first_divergence === null || rng.first_divergence === undefined
          ? null
          : decodeCounters(rng.first_divergence, "first_divergence"),
    },
  };
}
