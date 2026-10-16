// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { SideInput } from "./SideInput.ts";

/**
 * The two sides of a space combat, as `{"query":"space_odds"}` of [`ti4_host_query`] reads
 * them: each a side of `ti4_calc::odds::SideInput` with no player. A side of
 * `MovementFacts.battle` is such a side.
 */
export type SpaceOddsRequest = { attacker: SideInput, defender: SideInput, 
/**
 * The combat has begun: the rollouts start at a combat round, with no space cannon and no
 * anti-fighter barrage before it (they are behind the combat).
 */
in_progress?: boolean, };
