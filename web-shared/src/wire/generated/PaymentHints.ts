// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PaymentPlanet } from "./PaymentPlanet.ts";

/**
 * What can pay a bill that a question opens before the engine asks for the payment (display
 * only): a paid research ([`ResearchPaymentHints`]) and a purchase of command tokens
 * ([`PurchaseHints`]). The payment questions that follow ask which of them to spend, one planet
 * or trade good at a time; the engine has no automatic rule, so a client that stages the
 * payment names each of them. The same fields as the payment of a production's facts.
 */
export type PaymentHints = { 
/**
 * The price of one thing that is bought: the research, or one command token.
 */
cost: number, 
/**
 * The ready planets that pay, each with the worth of its best face. The Triad is not
 * listed.
 */
planets: Array<PaymentPlanet>, 
/**
 * Trade goods, spent one at a time at `trade_good_worth` each.
 */
trade_goods: number, trade_good_worth: number, 
/**
 * Commodities that the seat may spend as trade goods (the Keleres agent).
 */
commodities: number, };
