// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { HitCause } from "./HitCause.ts";
import type { HitRestriction } from "./HitRestriction.ts";
import type { PlayerId } from "./PlayerId.ts";

/**
 * The kind of hit a casualty decision assigns (see [`DecisionContext::hit`]).
 */
export type HitDetail = { cause: HitCause, 
/**
 * `true` for a destroy effect: not a hit, so SUSTAIN DAMAGE does not apply to it.
 */
destroy: boolean, restriction: HitRestriction, 
/**
 * The seat whose ability or roll produced it, when public.
 */
producer?: PlayerId, };
