// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { FollowerFact } from "./FollowerFact.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { StrategyCardId } from "./StrategyCardId.ts";

/**
 * The strategic action that is resolved now: who played which strategy card, and what each
 * follower did.
 */
export type SecondaryFacts = { primary: PlayerId, card: StrategyCardId, 
/**
 * The printed name of the card.
 */
name: string, 
/**
 * The primary is resolved: the followers are asked, in their order.
 */
open: boolean, 
/**
 * The seats that may follow, in the order in which they are asked.
 */
followers: Array<FollowerFact>, 
/**
 * The viewer may draft its secondary now ([`PlanScope::Secondary`]).
 */
draft: boolean, };
