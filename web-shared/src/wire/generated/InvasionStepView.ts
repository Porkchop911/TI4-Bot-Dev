// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { InvasionDie } from "./InvasionDie.ts";
import type { PlacedUnitView } from "./PlacedUnitView.ts";
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";

/**
 * Latest resolved automatic step, scoped to one planet and one ground round.
 */
export type InvasionStepView = { planet: PlanetId, kind: string, round: number, before: Array<PlacedUnitView>, after: Array<PlacedUnitView>, dice: Array<InvasionDie>, hits: { [key in PlayerId]: number }, harrow_hits: number, };
