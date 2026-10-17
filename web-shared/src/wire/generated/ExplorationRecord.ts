// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";

/**
 * One exploration card drawn: who drew it, from which deck, for which planet, and the outcome.
 */
export type ExplorationRecord = { round: number, player: PlayerId, deck: string, card: string, 
/**
 * `None` for a frontier token.
 */
planet: PlanetId | null, 
/**
 * `fragment`, `attached:<attachment>`, `resolved`, `unresolved` (no handler) or `discarded`.
 */
outcome: string, 
/**
 * The system a frontier token was explored in (`None` for a planet).
 */
system?: string, 
/**
 * What the card did for the player, in plain words ("gained 1 trade good"; empty when it
 * did nothing). Display only: no rule reads it.
 */
result?: string, 
/**
 * What the card changed in the player's holdings, as signed amounts: `trade_goods`,
 * `commodities`, `command_tokens`, `action_cards`, `secret_objectives`, `relics`,
 * `technologies`. Absent keys did not change. Display only.
 */
gains?: { [key in string]: number }, 
/**
 * What the card put on the map or in the play area, one tag each: `wormhole:GAMMA:<system>`,
 * `ion_storm:<system>:<ALPHA|BETA>`, `planet:<planet>:<system>`, `attachment:<id>:<planet>`,
 * `fragment:<trait>`, `play_area:<card>`. Display only.
 */
placed?: Array<string>, };
