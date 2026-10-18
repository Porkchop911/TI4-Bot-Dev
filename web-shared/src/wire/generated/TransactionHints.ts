// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";
import type { TradeDeal } from "./TradeDeal.ts";

/**
 * The look-ahead of a negotiation's questions, `details.transaction`, written with
 * [`Choice::hinted`](crate::choice::Choice::hinted) (display only; labels, ids and payloads are
 * unchanged). The question of the proposer has `offers`, the deal of each offer option by its id
 * (the decline has none), and no `offer`; the question of the partner has `offer`, the deal on
 * the table that it accepts, refuses or counters, and `offers` is empty.
 */
export type TransactionHints = { 
/**
 * The seat that proposes: it gives `given` and receives `received`.
 */
proposer: PlayerId, 
/**
 * The seat that the proposal is made to.
 */
partner: PlayerId, offers: { [key in string]: TradeDeal }, offer?: TradeDeal, };
