// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * What raised this decision.
 *
 * Named rather than free text so a producer's identity survives rewording. `Rule` carries an LRR
 * reference where one exists, because "the engine asked" is not an explanation anyone can check.
 */
export type DecisionSource = { "Rule": string } | { "StrategyCard": { card: string, secondary: boolean, } } | { "ActionCard": string } | { "FactionAbility": string } | { "Content": string } | { "Agenda": string } | { "Reaction": string };
