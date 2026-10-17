// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * How many ships of the seat can move to one system, if it is activated.
 */
export type ReachFact = { system: string, ships: number, 
/**
 * The number of systems that these ships are in.
 */
origins: number, 
/**
 * The production value of the units of the seat in the system: with more than 0 the action
 * has a production step there.
 */
production?: number, 
/**
 * How many guns of the seat roll SPACE CANNON at the activation: another player has ships in
 * the system, and the guns are there or reach it.
 */
cannon?: number, };
