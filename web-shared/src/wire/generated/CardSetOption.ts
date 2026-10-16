// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { CardOption } from "./CardOption.ts";

/**
 * One set of strategy cards that a table can play.
 */
export type CardSetOption = { id: string, name: string, 
/**
 * The table plays this set when it names none.
 */
default: boolean, 
/**
 * The eight cards, by initiative.
 */
cards: Array<CardOption>, };
