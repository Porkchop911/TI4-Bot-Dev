// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ActionCardId } from "./ActionCardId.ts";
import type { FactionId } from "./FactionId.ts";
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SecretObjectiveId } from "./SecretObjectiveId.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * One thing that a [`Phrase`] names. The cards are named by the ids of their records in the
 * catalogue, as [`crate::transactions::TradeItem`] names them.
 */
export type Subject = { "kind": "seat", player: PlayerId, } | { "kind": "seat_possessive", player: PlayerId, } | { "kind": "system", system: SystemId, } | { "kind": "planet", planet: PlanetId, } | { "kind": "unit", unit_type: string, } | { "kind": "units", unit_type: string, count: number, } | { "kind": "agenda", card: string, } | { "kind": "strategy_card", card: string, } | { "kind": "note", card: string, owner: FactionId, } | { "kind": "action_card", card: ActionCardId, } | { "kind": "secret_objective", card: SecretObjectiveId, };
