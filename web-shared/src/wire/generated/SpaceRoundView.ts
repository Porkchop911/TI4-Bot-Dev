// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { CombatDieRoll } from "./CombatDieRoll.ts";
import type { PlacedUnitView } from "./PlacedUnitView.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * One round of a space combat of the turn as it resolved ([`ti4_model::state::SpaceRoundRecord`]).
 */
export type SpaceRoundView = { system_id: SystemId, attacker: PlayerId, defender: PlayerId, battle_seq: number, round: number, round_start: Array<PlacedUnitView>, barrage_start?: Array<PlacedUnitView>, barrage_dice?: Array<CombatDieRoll>, barrage_hits?: { [key in PlayerId]: number }, dice_rolls: Array<CombatDieRoll>, hits: { [key in PlayerId]: number }, };
