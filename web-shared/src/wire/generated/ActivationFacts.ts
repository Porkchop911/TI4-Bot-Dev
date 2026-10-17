// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ReachFact } from "./ReachFact.ts";

export type ActivationFacts = { tactic_tokens: number, 
/**
 * The activation places a command token of the tactic pool. False for a tactical action
 * that places none, in any system, also one with the seat's token (Thunder's Edge Warfare).
 */
costs_token: boolean, systems: Array<ReachFact>, };
