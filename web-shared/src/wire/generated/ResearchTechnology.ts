// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { TechColour } from "./TechColour.ts";

/**
 * One technology of the tree, as printed, with whether the seat owns it.
 */
export type ResearchTechnology = { 
/**
 * The id of the technology: the id of its option when it can be researched now.
 */
id: string, 
/**
 * The colour of the card. A unit upgrade has none (90.7b).
 */
colour?: TechColour, 
/**
 * The prerequisites, one for each printed symbol, in the printed order.
 */
prerequisites: Array<TechColour>, unit_upgrade: boolean, 
/**
 * A technology of a faction (90.11).
 */
faction: boolean, owned: boolean, };
