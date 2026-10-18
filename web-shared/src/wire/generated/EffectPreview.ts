// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Phrase } from "./Phrase.ts";
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";
import type { TriggerUnits } from "./TriggerUnits.ts";

/**
 * What playing one reaction card would do in the window that offers it (display only).
 *
 * The engine words it in `phrase` (a [`crate::phrases::Phrase`]: the sentence, with the seats,
 * places, units and cards it names as subjects), so a client keeps no rule for each kind. The
 * facts stay beside it as data. Every fact except `option`, `card` and `effect` is optional and
 * omitted when empty.
 */
export type EffectPreview = { 
/**
 * The id of the option of this decision the preview belongs to; empty in
 * [`DecisionContext::pending_effect`], which belongs to no option.
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
fizzles?: string, 
/**
 * What playing it would do, as one or two sentences: "Cancels {0} {1}. The card stays spent
 * and its effect does not happen." When it would change nothing, the sentence says why and
 * ends with "Playing it now would change nothing." The pending effect of a played card speaks
 * of the seat that played it ("Bob would gain …"), and when it would change nothing it ends
 * with "It would change nothing when it resolves."
 */
phrase: Phrase, };
