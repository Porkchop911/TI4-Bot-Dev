// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { NeighbourProgressView } from "./NeighbourProgressView.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * Exact progress towards a public objective requirement.
 */
export type ObjectiveProgressView = { have: number, threshold: number, satisfied: boolean, 
/**
 * Push Boundaries: this player's planet count against each neighbour's. Display only.
 */
neighbours?: Array<NeighbourProgressView>, 
/**
 * Learn the Secrets of the Cosmos: the systems that count right now. Display only.
 */
systems?: Array<SystemId>, };
