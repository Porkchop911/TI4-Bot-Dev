// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { CommodityLimit } from "./CommodityLimit.ts";
import type { TradeOwnGain } from "./TradeOwnGain.ts";

/**
 * The look-ahead of Trade's question of the other seats: its fields are top-level keys of the
 * details, `details.commodities` and `details.own`, written with [`Choice::hinted_fields`], so
 * that a client can stage the whole primary at once (display only). Every later question says
 * it again, for the seats that are left.
 */
export type TradeReplenishHints = { 
/**
 * The commodities of each seat that the question offers, by the id of its option (the
 * faction name of `details.seats`), now and at the limit that its replenish fills to.
 */
commodities: { [key in string]: CommodityLimit }, 
/**
 * What the primary already gave its own seat, before it chooses the others.
 */
own: TradeOwnGain, };
