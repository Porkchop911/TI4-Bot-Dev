// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PaymentPlanet } from "./PaymentPlanet.ts";

/**
 * The look-ahead of Leadership's purchase of command tokens for influence (`details.purchase`).
 */
export type PurchaseHints = { 
/**
 * All the influence that the seat can spend, its trade goods included.
 */
influence_available: number, 
/**
 * How many tokens that influence pays for in total (`floor(available / cost)`; the loop
 * asks again exactly while `available >= cost * (bought + 1)`).
 */
max: number, 
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
