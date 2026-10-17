// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { CombatDieRoll } from "./CombatDieRoll.ts";
import type { PlacedUnitView } from "./PlacedUnitView.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * Publicly observable space combat state in progress.
 */
export type CombatView = { system_id: SystemId, round: number, battle_seq: number, phase: string, round_start: Array<PlacedUnitView>, barrage_start: Array<PlacedUnitView>, barrage_hits: { [key in PlayerId]: number }, barrage_dice: Array<CombatDieRoll>, 
/**
 * Hits still queued against each seat, including the other side's pending return fire.
 */
remaining_hits: { [key in PlayerId]: number }, attacker: PlayerId, defender: PlayerId, active_player?: PlayerId, stage?: string, hits_to_assign?: number, attacker_hits?: number, defender_hits?: number, dice_rolls?: Array<CombatDieRoll>, };
