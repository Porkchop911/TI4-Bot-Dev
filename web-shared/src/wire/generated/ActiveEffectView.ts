// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { EffectExpiry } from "./EffectExpiry.ts";
import type { Phrase } from "./Phrase.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";

/**
 * One lingering effect of a played action card.
 */
export type ActiveEffectView = { 
/**
 * The seat the effect belongs to: the player who played the card, or the player it binds
 * when the card marks another seat (`bars_retreat`, `duress`).
 */
seat: PlayerId, 
/**
 * A content alias of the card, when one card alone causes the effect.
 */
card?: string, 
/**
 * A stable snake_case kind; the same words as the previews of the reaction windows.
 */
effect: string, until: EffectExpiry, 
/**
 * How many copies of the card stack on it.
 */
copies?: number, 
/**
 * The size of the effect: a bonus, hits left to cancel, votes added.
 */
amount?: number, 
/**
 * The other seat involved: who holds the duress, the opponent of a rout.
 */
other?: PlayerId, system?: SystemId, 
/**
 * What causes it: the card (a subject), or the words of a pool of cards ("Shields Holding /
 * Maneuvering Jets").
 */
title: Phrase, 
/**
 * What it does, in one short line: "+1 to all combat rolls", "Marked by {0}: ...".
 */
summary: Phrase, 
/**
 * `until` in words: "until the end of this combat round".
 */
expiry: string, };
