// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { DecisionTrigger } from "./DecisionTrigger.ts";
import type { ReactionOffer } from "./ReactionOffer.ts";

/**
 * The reaction window that the viewer is asked in.
 */
export type ReactionFacts = { 
/**
 * What opened the window. Public: all of it has happened.
 */
trigger?: DecisionTrigger, 
/**
 * The window is in the action phase, so "only on my turns" says something about it.
 */
own_turn_applies: boolean, offers: Array<ReactionOffer>, };
