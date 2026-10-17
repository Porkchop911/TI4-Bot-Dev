// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ActionCardId } from "./ActionCardId.ts";
import type { AgendaPeekCard } from "./AgendaPeekCard.ts";
import type { FactionId } from "./FactionId.ts";
import type { LeaderId } from "./LeaderId.ts";
import type { LeaderStatus } from "./LeaderStatus.ts";
import type { ObjectiveProgressView } from "./ObjectiveProgressView.ts";
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { RelicId } from "./RelicId.ts";
import type { SecretObjectiveId } from "./SecretObjectiveId.ts";
import type { StrategyCardId } from "./StrategyCardId.ts";
import type { TechnologyId } from "./TechnologyId.ts";

/**
 * Redacted view of a single player at the table.
 */
export type PlayerView = { id: PlayerId, faction: FactionId, victory_points: number, trade_goods: number, commodities: number, tactic_tokens: number, fleet_tokens: number, strategic_tokens: number, passed: boolean, strategy_cards: Array<StrategyCardId>, exhausted_strategy_cards: Array<StrategyCardId>, technologies: Array<TechnologyId>, exhausted_technologies: Array<TechnologyId>, relics: Array<RelicId>, exhausted_relics: Array<RelicId>, 
/**
 * The legendary planets whose ability card is exhausted. The card exhausts apart from
 * the planet.
 */
exhausted_legendary?: Array<PlanetId>, 
/**
 * Relic fragments by deck, faceup in the play area.
 */
relic_fragments?: { [key in string]: number }, 
/**
 * Exploration cards that stay faceup in the play area.
 */
exploration_cards?: Array<string>, 
/**
 * Public count of action cards in hand.
 */
action_cards_count: number, 
/**
 * Public count of unscored secret objectives held.
 */
secret_objectives_count: number, 
/**
 * Only present and populated if the viewer IS this player. Strictly empty for opponents/spectators.
 */
held_action_cards?: Array<ActionCardId>, 
/**
 * Only present and populated if the viewer IS this player or if revealed by law. Strictly empty otherwise.
 */
held_secret_objectives?: Array<SecretObjectiveId>, 
/**
 * Publicly scored secret objectives.
 */
scored_secret_objectives?: Array<SecretObjectiveId>, 
/**
 * Exact progress toward this seat's own held secrets with a countable requirement.
 * Display-only, and present only in the view of the seat that holds them: opponents and
 * spectators never receive it, not even for secrets a law has turned face up.
 */
secret_objective_progress?: { [key in SecretObjectiveId]: ObjectiveProgressView }, leaders: { [key in LeaderId]: LeaderStatus }, 
/**
 * Other players' command tokens held in this seat's fleet pool (Mahact's Edict), one entry
 * per token naming its owner, sorted. Public. Absent for every other seat and while the pool
 * holds none, so older clients and saves are unaffected.
 */
held_foreign_tokens?: Array<PlayerId>, 
/**
 * The top agendas this seat was shown by Insider Information, top first, while they are
 * still the top of the deck. Private: present only in the view of the seat itself, never for
 * opponents or spectators, and absent when there is nothing to show, so older clients and
 * saves are unaffected.
 */
agenda_peek?: Array<AgendaPeekCard>, };
