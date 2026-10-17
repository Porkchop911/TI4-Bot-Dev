// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One secret objective as printed.
 */
export type SecretObjective = { id: string, name: string, 
/**
 * The phase in which the objective is scored, as printed: `Status`, `Action` or `Agenda`.
 */
phase: string, 
/**
 * What the objective asks for, one of [`ComponentCatalog::secret_objective_groups`].
 */
group: string, text: string, };
