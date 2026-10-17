// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { PlanetValueBoost } from "./PlanetValueBoost.ts";
import type { PlayerId } from "./PlayerId.ts";

/**
 * Redacted view of a planet on the board.
 */
export type PlanetView = { planet_id: PlanetId, controlled_by: PlayerId | null, exhausted: boolean, attachments?: Array<string>, 
/**
 * What raises this planet's resources or influence above its printed values (attachments,
 * laws such as Core Mining, Helios docks), from the engine's own planet value function.
 * Display only: the printed values are in the map tile metadata. Empty when nothing does.
 */
value_boosts?: Array<PlanetValueBoost>, };
