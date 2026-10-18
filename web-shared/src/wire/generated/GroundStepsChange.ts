// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { GroundStepView } from "./GroundStepView.ts";

/**
 * The steps of the invasions of the turn, as [`SpaceRoundsChange`] has the rounds.
 */
export type GroundStepsChange = { kept: number, before: Array<GroundStepView>, after: Array<GroundStepView>, };
