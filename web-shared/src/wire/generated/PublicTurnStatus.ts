// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Phase } from "./Phase.ts";
import type { PlayerId } from "./PlayerId.ts";

/**
 * Public turn status visible to all participants and spectators.
 *
 * Crucially, this communicates which seat is active or making a decision without disclosing
 * private card identities, private reaction windows, or legal options.
 */
export type PublicTurnStatus = { "kind": "active_turn", player: PlayerId, phase: Phase, round: number, } | { "kind": "waiting_for_decision", seat: PlayerId, phase: Phase, round: number, stage: string, } | { "kind": "waiting_for_reactions", phase: Phase, round: number, } | { "kind": "phase_transition", phase: Phase, round: number, } | { "kind": "game_over", winner: PlayerId | null, };
