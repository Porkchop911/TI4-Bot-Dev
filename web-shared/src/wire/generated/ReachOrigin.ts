// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ReachShip } from "./ReachShip.ts";

/**
 * One seat's ships in one system that could reach the queried system.
 */
export type ReachOrigin = { owner: string, origin: string, ships: Array<ReachShip>, };
