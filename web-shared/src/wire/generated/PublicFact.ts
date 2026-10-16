// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PaidWith } from "./PaidWith.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { Spend } from "./Spend.ts";
import type { TokenPool } from "./TokenPool.ts";
import type { TransactionAnswer } from "./TransactionAnswer.ts";
import type { TurnAction } from "./TurnAction.ts";

/**
 * What one resolved decision did, in public ids.
 */
export type PublicFact = { "kind": "action_started", action: TurnAction, } | { "kind": "turn_ended" } | { "kind": "transaction_opened", with: PlayerId, } | { "kind": "transaction_proposed", to: PlayerId, } | { "kind": "negotiation_ended", with: PlayerId, } | { "kind": "transaction_answered", proposer: PlayerId, answer: TransactionAnswer, } | { "kind": "activated", system: string, } | { "kind": "moved", unit: string, from: string, to: string, } | { "kind": "loaded", unit: string, system?: string, planet?: string, } | { "kind": "produced", unit: string, count: number, system?: string, planet?: string, } | { "kind": "unit_placed", unit: string, system: string, planet?: string, } | { "kind": "paid", with: PaidWith, spend: Spend, } | { "kind": "structure_placed", unit: string, system: string, planet: string, } | { "kind": "casualty", seat: PlayerId, unit: string, system?: string, planet?: string, damaged: boolean, } | { "kind": "sustained", seat: PlayerId, unit: string, system?: string, planet?: string, } | { "kind": "retreat_announced", seat: PlayerId, system: string, retreat: boolean, } | { "kind": "retreated", seat: PlayerId, from: string, to: string, } | { "kind": "landed", unit: string, planet: string, damaged: boolean, } | { "kind": "researched", tech: string, } | { "kind": "strategy_picked", card: string, } | { "kind": "voted", outcome?: string, } | { "kind": "tie_broken", outcome: string, } | { "kind": "planet_exhausted_for_vote", planet: string, } | { "kind": "card_played", card: string, } | { "kind": "objective_scored", objective: string, secret: boolean, } | { "kind": "tokens_gained", pool: TokenPool, count: number, };
