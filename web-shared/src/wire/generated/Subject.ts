// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ActionCardId } from "./ActionCardId.ts";
import type { FactionId } from "./FactionId.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SecretObjectiveId } from "./SecretObjectiveId.ts";

/**
 * One thing that a [`Phrase`] names. The cards are named by the ids of their records in the
 * catalogue, as [`crate::transactions::TradeItem`] names them.
 */
export type Subject = { "kind": "seat", player: PlayerId, } | { "kind": "note", card: string, owner: FactionId, } | { "kind": "action_card", card: ActionCardId, } | { "kind": "secret_objective", card: SecretObjectiveId, };
