// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlacedUnitView } from "./PlacedUnitView.ts";
import type { PlanetId } from "./PlanetId.ts";
import type { PlanetView } from "./PlanetView.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * Redacted view of a star system.
 */
export type SystemView = { system_id: SystemId, command_tokens: Array<PlayerId>, planets: { [key in PlanetId]: PlanetView }, units: Array<PlacedUnitView>, };
