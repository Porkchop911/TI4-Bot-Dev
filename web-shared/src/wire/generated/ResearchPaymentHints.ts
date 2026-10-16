// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PaymentPlanet } from "./PaymentPlanet.ts";
import type { PlanetId } from "./PlanetId.ts";
import type { TechnologyId } from "./TechnologyId.ts";

/**
 * The look-ahead of a paid research (`details.payment` of the research question).
 */
export type ResearchPaymentHints = { 
/**
 * For each offered technology whose prerequisites exhaust specialty planets, those
 * planets: the payment of that research does not offer them.
 */
kept: { [key in TechnologyId]: Array<PlanetId> }, 
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
