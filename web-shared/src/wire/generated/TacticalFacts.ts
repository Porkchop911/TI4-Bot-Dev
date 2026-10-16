// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ActivationFacts } from "./ActivationFacts.ts";
import type { InvasionFacts } from "./InvasionFacts.ts";
import type { MovementFacts } from "./MovementFacts.ts";
import type { ProductionFacts } from "./ProductionFacts.ts";

/**
 * The facts of the pending choice of a tactical action.
 */
export type TacticalFacts = { "kind": "activation" } & ActivationFacts | { "kind": "movement" } & MovementFacts | { "kind": "invasion" } & InvasionFacts | { "kind": "production" } & ProductionFacts;
