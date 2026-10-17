// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One faction sheet: its name and what belongs to it, by id.
 */
export type FactionRecord = { id: string, 
/**
 * The name as printed: "The Federation of Sol".
 */
name: string, commodities: number, 
/**
 * The technologies that the faction begins with. Empty for a faction that chooses them.
 */
starting_tech: Array<string>, faction_tech: Array<string>, abilities: Array<string>, leaders: Array<string>, promissory_notes: Array<string>, 
/**
 * The unit cards of the faction: its own, and the generic ones that it has.
 */
units: Array<string>, source?: string, };
