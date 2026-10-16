// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Phase } from "./Phase.ts";
import type { PlayerId } from "./PlayerId.ts";

/**
 * Typed event payload. Visibility is never inferred from presentation text.
 */
export type GameEventKind = { "kind": "game_initialized", round: number, phase: Phase, speaker: PlayerId, } | { "kind": "decision_resolved" } | { "kind": "dice_rolled" } | { "kind": "phase_transition", phase: Phase, round: number, } | { "kind": "game_finished", winner: PlayerId | null, };
