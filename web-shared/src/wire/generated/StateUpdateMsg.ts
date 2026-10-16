// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ActionCardFacts } from "./ActionCardFacts.ts";
import type { AgendaFacts } from "./AgendaFacts.ts";
import type { AutoResolvedNote } from "./AutoResolvedNote.ts";
import type { CurrentLogPath } from "./CurrentLogPath.ts";
import type { ExplorationFacts } from "./ExplorationFacts.ts";
import type { GalaxyLayout } from "./GalaxyLayout.ts";
import type { GameView } from "./GameView.ts";
import type { HistoryStatus } from "./HistoryStatus.ts";
import type { PendingChoiceEnvelope } from "./PendingChoiceEnvelope.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { PublicTurnStatus } from "./PublicTurnStatus.ts";
import type { ReactionFacts } from "./ReactionFacts.ts";
import type { ReactionMode } from "./ReactionMode.ts";
import type { ResearchFacts } from "./ResearchFacts.ts";
import type { ScoreFacts } from "./ScoreFacts.ts";
import type { SeatFact } from "./SeatFact.ts";
import type { SecondaryFacts } from "./SecondaryFacts.ts";
import type { SecretObjectiveFacts } from "./SecretObjectiveFacts.ts";
import type { TacticalFacts } from "./TacticalFacts.ts";
import type { ViewerRole } from "./ViewerRole.ts";

/**
 * Versioned state update or replacement snapshot after state transition.
 */
export type StateUpdateMsg = { protocol_version: number, game_id: string, game_version: number, viewer: ViewerRole, view: GameView, 
/**
 * The engine's `GameState`, passed through without a projection.
 */
state: Record<string, unknown>, galaxy_layout: GalaxyLayout, pending_choice?: PendingChoiceEnvelope, turn_status: PublicTurnStatus, history?: HistoryStatus, current_path?: CurrentLogPath, 
/**
 * Decisions the engine settled for the receiving seat since the previous update because
 * exactly one option was legal. Feedback only: never journaled, never sent to other seats.
 */
auto_resolved?: Array<AutoResolvedNote>, 
/**
 * The receiving seat's own "never offer" choices, by printed card name; absent when it has
 * none. Always empty for spectators and for every other seat's view.
 */
reaction_modes?: { [key in string]: ReactionMode }, 
/**
 * What the viewer needs to stage the pending choice of a tactical action
 * ([`crate::view::tactical`]). Only the seat that is asked has it.
 */
tactical?: TacticalFacts, 
/**
 * The exploration decks and the planets that are still explored ([`crate::view::exploration`]).
 */
exploration?: ExplorationFacts, 
/**
 * The action card deck, its discard pile and the hand limit of the viewer
 * ([`crate::view::action_cards`]).
 */
action_cards?: ActionCardFacts, 
/**
 * The secret objective deck and what each seat has scored of it
 * ([`crate::view::secret_objectives`]).
 */
secret_objectives?: SecretObjectiveFacts, 
/**
 * Where the victory points of every seat come from ([`crate::view::score`]).
 */
score?: ScoreFacts, 
/**
 * The agenda deck ([`crate::view::agendas`]).
 */
agendas?: AgendaFacts, 
/**
 * The strategic action that is resolved now ([`crate::view::secondary`]).
 */
secondary?: SecondaryFacts, 
/**
 * The reaction window that the viewer is asked in ([`crate::view::reaction`]).
 */
reaction?: ReactionFacts, 
/**
 * The technology tree of the viewer, while it is asked to research
 * ([`crate::view::research`]).
 */
research?: ResearchFacts, 
/**
 * Who sits in each seat: the name of its player and who plays it
 * ([`crate::setup::SeatFact`]). Public; absent for a game with no table.
 */
seats?: { [key in PlayerId]: SeatFact }, };
