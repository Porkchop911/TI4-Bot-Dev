// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { CommandPools } from "./CommandPools.ts";
import type { PoolOptions } from "./PoolOptions.ts";

/**
 * The look-ahead of a gain and the redistribution after it (`details.redistribution` of
 * Warfare's recall): what a client needs to plan both as one screen (display only).
 */
export type RedistributionHints = { 
/**
 * The pools as they stand now, before the gain.
 */
pools: CommandPools, gained: number, 
/**
 * The tokens that the redistribution arranges: the pools of now and the gained ones.
 */
total: number, 
/**
 * The option of the gain into each pool.
 */
gain: PoolOptions, 
/**
 * The id of every arrangement that the redistribution will offer (`tactic|fleet|strategic`).
 */
arrangements: Array<string>, };
