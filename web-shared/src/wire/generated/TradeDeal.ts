// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { TradeItem } from "./TradeItem.ts";

/**
 * The contents of one deal (display only): what the proposer hands to the partner, and what the
 * proposer receives from the partner. The seats are those of [`TransactionHints`].
 */
export type TradeDeal = { given: Array<TradeItem>, received: Array<TradeItem>, };
