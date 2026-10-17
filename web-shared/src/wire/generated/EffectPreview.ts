// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";
import type { TriggerUnits } from "./TriggerUnits.ts";

/**
 * What playing one reaction card would do in the window that offers it (display only).
 *
 * Structured facts, not prose: the client words them. Every field except `option`, `card` and
 * `effect` is optional and omitted when empty, so a new effect kind never breaks a client (it
 * ignores a kind it does not know).
 */
export type EffectPreview = { 
/**
 * The id of the option of this decision the preview belongs to.
 */
option: string, 
/**
 * The content alias of the card.
 */
card: string, 
/**
 * A stable snake_case effect kind (`cancel_card`, `destroy_unit`, `cancel_hits`, ...).
 */
effect: string, 
/**
 * Seats the effect lands on or concerns (the victim, the opponent, the elected player).
 */
seats?: Array<PlayerId>, system?: SystemId, planets?: Array<PlanetId>, 
/**
 * The units the effect concerns, public on the board.
 */
units?: Array<TriggerUnits>, 
/**
 * The main quantity: hits cancelled, tokens gained, votes added, planets exhausted.
 */
amount?: number, 
/**
 * The quantity it applies to: the hits that are about to be assigned.
 */
of?: number, 
/**
 * Other cards, agendas or strategy cards the effect names (the card cancelled, the agenda
 * discarded). Ids, not names.
 */
refs?: Array<string>, 
/**
 * Why playing it would change nothing right now (`immune`, `nothing_there`, ...).
 */
fizzles?: string, };
