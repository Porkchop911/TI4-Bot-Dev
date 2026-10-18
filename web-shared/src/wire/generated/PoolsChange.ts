// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";
import type { SeatPools } from "./SeatPools.ts";

/**
 * The pools of a seat, before and after.
 */
export type PoolsChange = { seat: PlayerId, before: SeatPools, after: SeatPools, };
