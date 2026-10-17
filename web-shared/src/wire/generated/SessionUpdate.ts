// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ActionCardFacts } from "./ActionCardFacts.ts";
import type { AgendaFacts } from "./AgendaFacts.ts";
import type { ExplorationFacts } from "./ExplorationFacts.ts";
import type { GameView } from "./GameView.ts";
import type { PendingChoiceEnvelope } from "./PendingChoiceEnvelope.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { PublicTurnStatus } from "./PublicTurnStatus.ts";
import type { ReactionFacts } from "./ReactionFacts.ts";
import type { ResearchFacts } from "./ResearchFacts.ts";
import type { ScoreFacts } from "./ScoreFacts.ts";
import type { SeatFact } from "./SeatFact.ts";
import type { SecondaryFacts } from "./SecondaryFacts.ts";
import type { SecretObjectiveFacts } from "./SecretObjectiveFacts.ts";
import type { TacticalFacts } from "./TacticalFacts.ts";
import type { ViewerRole } from "./ViewerRole.ts";

/**
 * What a client is shown of a game: the part of a state update that depends on the game alone.
 *
 * The server's `StateUpdateMsg` has these four fields under the same names, and the wasm build
 * hands them to its host as they are, so one client reads both.
 */
export type SessionUpdate = { viewer: ViewerRole, view: GameView, pending_choice?: PendingChoiceEnvelope, turn_status: PublicTurnStatus, 
/**
 * What the viewer needs to stage the pending choice of a tactical action. It is there
 * for the seat that is asked, on a game that has its map ([`crate::view::tactical`]).
 */
tactical?: TacticalFacts, 
/**
 * The exploration decks and the planets that are still explored, on a game that has its
 * map ([`crate::view::exploration`]).
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
 * The strategic action that is resolved now, from the card that is played to the last
 * follower ([`crate::view::secondary`]). It is there when the host follows its game
 * ([`FactsContext::secondary`]).
 */
secondary?: SecondaryFacts, 
/**
 * The reaction window that the viewer is asked in, with what it offers as printed
 * ([`crate::view::reaction`]).
 */
reaction?: ReactionFacts, 
/**
 * The technology tree of the viewer, while it is asked to research ([`crate::view::research`]).
 */
research?: ResearchFacts, 
/**
 * Who sits in each seat: the name of its player and who plays it ([`crate::view::seats::SeatFact`]).
 * It is of the table and not of the game, so the host of the game sets it; a game with no
 * table has none.
 */
seats?: { [key in PlayerId]: SeatFact }, };
