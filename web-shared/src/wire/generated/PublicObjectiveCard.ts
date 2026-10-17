// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One public objective as printed.
 */
export type PublicObjectiveCard = { id: string, name: string, 
/**
 * What the objective is worth: 1 for stage I, 2 for stage II.
 */
points: number, 
/**
 * What the objective asks for, one of [`ComponentCatalog::public_objective_groups`].
 */
group: string, text: string, };
