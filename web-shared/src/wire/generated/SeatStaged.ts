// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { StagedSlot } from "./StagedSlot.ts";
import type { StagedValue } from "./StagedValue.ts";

/**
 * Every slot of one seat.
 */
export type SeatStaged = { 
/**
 * The revision of the newest write of any slot (0: never written).
 */
revision: number, slots: { [key in StagedSlot]?: StagedValue }, };
