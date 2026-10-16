// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { InvasionOddsContext } from "./InvasionOddsContext.ts";
import type { InvasionStepView } from "./InvasionStepView.ts";
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * Public invasion identity and current planet-local progress.
 */
export type InvasionView = { system_id: SystemId, invasion_seq: number, invader: PlayerId, phase: string, planets: Array<PlanetId>, current_planet: PlanetId | null, defender: PlayerId | null, ground_round: number, last_step?: InvasionStepView, 
/**
 * Authoritative public classification and still-pending modifiers for landing previews.
 */
odds_context: { [key in PlanetId]: InvasionOddsContext }, };
