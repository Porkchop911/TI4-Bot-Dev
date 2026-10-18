// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { SpenderStatus } from "./SpenderStatus.ts";

/**
 * A strategy card that spends what a ready planet gives, and where it is in this round.
 */
export type ReadySpender = { 
/**
 * The printed name of the card.
 */
card: string, 
/**
 * What it spends: "resources" or "influence".
 */
spends: string, status: SpenderStatus, };
