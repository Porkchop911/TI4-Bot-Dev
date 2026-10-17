// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { DecisionSource } from "./DecisionSource.ts";
import type { DecisionTarget } from "./DecisionTarget.ts";
import type { DecisionTrigger } from "./DecisionTrigger.ts";
import type { EffectPreview } from "./EffectPreview.ts";
import type { HitDetail } from "./HitDetail.ts";
import type { OutstandingConstraint } from "./OutstandingConstraint.ts";
import type { Phase } from "./Phase.ts";
import type { PlayerId } from "./PlayerId.ts";

/**
 * Why the engine is asking, in a form that does not depend on prompt wording.
 */
export type DecisionContext = { version: number, 
/**
 * The seat being asked.
 */
actor: PlayerId, source: DecisionSource, 
/**
 * A stable machine identifier for the question, e.g. `"score_public_objective"`. Never a
 * sentence, and never derived from the prompt.
 */
subtype: string, phase: Phase, round: number, 
/**
 * Whether the seat may decline. Declining an obligation is not the same as choosing between
 * obligations, and a policy cannot tell those apart from an option list alone.
 */
optional: boolean, target: DecisionTarget | null, 
/**
 * This decision belongs to the currently running space battle, not a separate cannon or invasion.
 */
space_battle?: boolean, 
/**
 * Activation sequence of the invasion that owns this choice, including nested reactions.
 */
invasion_seq?: number, 
/**
 * Quantities still owed or available within a decision already under way.
 */
outstanding: Array<OutstandingConstraint>, 
/**
 * What happened to open this reaction window, for display only (public facts).
 *
 * Deliberately not part of [`DecisionContext::canonical`] or the replay fingerprint: it is
 * derived deterministically from the event, so it adds no information a replay needs, and
 * it is stripped from recorded decisions (`DecisionLog::record`). Additive and optional, so
 * old saves and old clients are unaffected.
 */
trigger?: DecisionTrigger, 
/**
 * What kind of hit a casualty decision assigns, for display only (public facts).
 *
 * Like [`DecisionContext::trigger`], deliberately not part of [`DecisionContext::canonical`]
 * or the replay fingerprint, stripped from recorded decisions, additive and optional: it
 * restates rules the engine already enforces, so a client need not guess them from labels.
 */
hit?: HitDetail, 
/**
 * What playing each card of a reaction window would do right now, for display only.
 *
 * Like [`DecisionContext::trigger`], deliberately not part of [`DecisionContext::canonical`]
 * or the replay fingerprint, stripped from recorded decisions, additive and optional: it is
 * recomputed from the state and the event, adds no information a replay needs, and is only
 * ever sent to the seat that is asked (the pending choice reaches no one else).
 */
previews?: Array<EffectPreview>, };
