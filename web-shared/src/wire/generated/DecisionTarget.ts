// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";
import type { UnitTypeId } from "./UnitTypeId.ts";

/**
 * What the decision is about, when it points at something on the board.
 *
 * Distinct from the option payloads: this is the subject of the *question*, shared by every
 * option, where an option payload describes one answer. "Which unit takes this hit" has a system
 * as its target and units as its options.
 */
export type DecisionTarget = { "System": SystemId } | { "Planet": { system: SystemId, planet: PlanetId, } } | { "Unit": { system: SystemId, unit: UnitTypeId, } } | { "Player": PlayerId };
