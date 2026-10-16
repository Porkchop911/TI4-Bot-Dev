// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ActionCardId } from "./ActionCardId.ts";
import type { FactionId } from "./FactionId.ts";
import type { SecretObjectiveId } from "./SecretObjectiveId.ts";

/**
 * One thing that a side of a deal hands over, as a client names it (display only): the cards by
 * the ids of their records in the catalogue, so that a client shows their printed names and
 * texts without reading the ids of the engine. [`TradeDeal`] lists them.
 */
export type TradeItem = { "kind": "trade_goods", count: number, } | { "kind": "commodities", count: number, } | { "kind": "relic_fragments", trait: string, count: number, } | { "kind": "note", card: string, owner: FactionId, } | { "kind": "action_card", card: ActionCardId, } | { "kind": "secret_objective", card: SecretObjectiveId, };
