// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";
import type { TriggerKind } from "./TriggerKind.ts";
import type { TriggerUnits } from "./TriggerUnits.ts";

/**
 * The public event that opened a reaction window (see [`DecisionContext::trigger`]).
 *
 * Everything named here has already happened in public: a played action card, an activated
 * system, a moved fleet. It never carries a vote outcome, a hand, or any unplayed alternative.
 */
export type DecisionTrigger = { kind: TriggerKind, 
/**
 * The raw engine event type, for fallback and debugging.
 */
event_type: string, 
/**
 * The engine event id, valid within one trace only: a session-local key, not a durable id.
 */
event_id: number, 
/**
 * `"when"` or `"after"`.
 */
relation: "when" | "after", 
/**
 * The seat that caused the event; absent for phase and agenda events.
 */
actor?: PlayerId, 
/**
 * A second seat: a transaction partner, the victim of hits, an elected player, a defender.
 */
subject?: PlayerId, 
/**
 * An action card alias (public once played) or a strategy card id.
 */
card?: string, agenda?: string, system?: SystemId, planet?: PlanetId, units?: Array<TriggerUnits>, hits?: number, 
/**
 * Ids of the events being resolved around this one, outermost first: non-empty when this is a
 * reaction to a reaction.
 */
chain?: Array<number>, };
