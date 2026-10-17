// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { SpaceRoundView } from "./SpaceRoundView.ts";

/**
 * The rounds of the space combats of the turn: the first `kept` stay, the rest is `before`
 * before the step and `after` after it. A step adds a round, rolls the last one again, or ends
 * the turn and takes them all.
 */
export type SpaceRoundsChange = { kept: number, before: Array<SpaceRoundView>, after: Array<SpaceRoundView>, };
