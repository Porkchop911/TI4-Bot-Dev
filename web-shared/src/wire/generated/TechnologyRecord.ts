// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One technology as printed.
 */
export type TechnologyRecord = { id: string, name: string, 
/**
 * The colours of the technology (`PROPULSION`, …), or `unit_upgrade`.
 */
types: Array<string>, 
/**
 * The prerequisites, one letter for each: `B`, `G`, `Y`, `R`.
 */
requirements?: string, 
/**
 * The faction of a faction technology.
 */
faction?: string, source?: string, 
/**
 * The technology that a faction technology replaces.
 */
base_upgrade?: string, text: string, };
