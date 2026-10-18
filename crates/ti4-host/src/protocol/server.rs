//! Server to client messages.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ti4_model::id::PlayerId;

use super::error::ErrorKind;
use super::status::{PublicTurnStatus, RejectionReason, ViewerRole};
use super::view::GameView;
use crate::map::GalaxyLayout;
pub use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_model::state::GameState;
use ti4_model::state::Phase;
use ti4_model::state::ReactionMode;

pub use crate::projection::PendingChoiceEnvelope;

/// Initial per-viewer snapshot sent upon subscription or reconnection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct InitialSnapshotMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub viewer: ViewerRole,
    pub view: GameView,
    /// The engine's `GameState`, passed through without a projection.
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub state: GameState,
    pub galaxy_layout: GalaxyLayout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_choice: Option<PendingChoiceEnvelope>,
    pub turn_status: PublicTurnStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<GameEvent>,
    #[serde(default, skip_serializing_if = "is_default_history")]
    pub history: HistoryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_path: Option<CurrentLogPath>,
    /// The receiving seat's own "never offer" choices, by printed card name; absent when it has
    /// none. Always empty for spectators and for every other seat's view.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reaction_modes: BTreeMap<String, ReactionMode>,
    /// What the viewer needs to stage the pending choice of a tactical action
    /// ([`crate::view::tactical`]). Only the seat that is asked has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tactical: Option<crate::view::tactical::TacticalFacts>,
    /// The exploration decks and the planets that are still explored ([`crate::view::exploration`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exploration: Option<crate::view::exploration::ExplorationFacts>,
    /// The action card deck, its discard pile and the hand limit of the viewer
    /// ([`crate::view::action_cards`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_cards: Option<crate::view::action_cards::ActionCardFacts>,
    /// The secret objective deck and what each seat has scored of it
    /// ([`crate::view::secret_objectives`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_objectives: Option<crate::view::secret_objectives::SecretObjectiveFacts>,
    /// Where the victory points of every seat come from ([`crate::view::score`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<crate::view::score::ScoreFacts>,
    /// The agenda deck ([`crate::view::agendas`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agendas: Option<crate::view::agendas::AgendaFacts>,
    /// The strategic action that is resolved now ([`crate::view::secondary`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<crate::view::secondary::SecondaryFacts>,
    /// The reaction window that the viewer is asked in ([`crate::view::reaction`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<crate::view::reaction::ReactionFacts>,
    /// The technology tree of the viewer, while it is asked to research
    /// ([`crate::view::research`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research: Option<crate::view::research::ResearchFacts>,
    /// Who sits in each seat: the name of its player and who plays it
    /// ([`crate::setup::SeatFact`]). Public; absent for a game with no table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<crate::setup::TableSeats>,
    /// The digest of the corpus that this server plays with
    /// ([`ti4_content::provenance::embedded_digest`]). A client that has the printed texts of
    /// its own (`crate::view::components::Catalog::content`) compares the two before it plays.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    /// The steps of a movement, of the landings or of a production that the undo of the
    /// viewer took back, in order, for the page to stage them again. Told once, with the
    /// first snapshot of the seat after its undo.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub undone: Vec<crate::planning::steps::PlanStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CurrentLogPath {
    pub round: u32,
    pub phase: Phase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
}

/// A pending action-phase prompt is a boundary, not part of the previous action.
#[must_use]
pub fn current_log_path(
    state: &GameState,
    pending: Option<&Choice>,
    records: &[ti4_engine::choice::DecisionRecord],
) -> Option<CurrentLogPath> {
    if state.finished {
        return None;
    }
    let action_id = pending
        .filter(|choice| state.phase == Phase::Action && choice.prompt != "action phase")
        .and_then(|_| action_id_for(records, records.len()));
    let stage = pending
        .and_then(|choice| choice.context.as_ref())
        .and_then(|context| stage_for(&context.subtype))
        .filter(|_| action_id.is_some())
        .map(str::to_owned);
    Some(CurrentLogPath {
        round: state.round,
        phase: state.phase,
        action_id,
        stage,
    })
}

/// Public cursor counts decisions, not engine steps or wall-clock events.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HistoryStatus {
    pub cursor: usize,
    pub redo_count: usize,
    #[serde(default)]
    pub generation: u64,
    /// The viewer may take back its last decision (`ClientMessage::Undo`): it is its own, and
    /// since it nothing hidden was shown, nothing was rolled and no other seat decided. It is
    /// not the host's history: `cursor` and `redo_count` say nothing of it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub can_undo: bool,
}

fn is_default_history(value: &HistoryStatus) -> bool {
    *value == HistoryStatus::default()
}

impl InitialSnapshotMsg {
    #[must_use]
    pub fn with_reaction_modes(mut self, modes: BTreeMap<String, ReactionMode>) -> Self {
        self.reaction_modes = modes;
        self
    }

    #[must_use]
    pub fn with_history(mut self, cursor: usize, redo_count: usize, generation: u64) -> Self {
        self.history = HistoryStatus {
            cursor,
            redo_count,
            generation,
            can_undo: false,
        };
        self
    }
}

/// Explicit audience for an authoritative event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "visibility", content = "seat", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EventVisibility {
    Public,
    Seat(PlayerId),
    Referee,
}

impl EventVisibility {
    #[must_use]
    pub fn permits(&self, viewer: &ViewerRole) -> bool {
        match self {
            Self::Public => true,
            Self::Seat(seat) => viewer.is_actor(seat),
            Self::Referee => false,
        }
    }
}

/// Typed event payload. Visibility is never inferred from presentation text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum GameEventKind {
    GameInitialized {
        round: u32,
        phase: Phase,
        speaker: PlayerId,
    },
    DecisionResolved,
    /// A die roll the table would see: gravity rifts, combat, abilities. Carries [`GameEvent::roll`].
    DiceRolled,
    PhaseTransition {
        phase: Phase,
        round: u32,
    },
    GameFinished {
        winner: Option<PlayerId>,
    },
}

/// Authoritative, auditable event log entry recorded during game execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GameEvent {
    pub id: String,
    pub timestamp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    pub visibility: EventVisibility,
    pub event: GameEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_start_cursor: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_end_cursor: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_start_cursor: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<PlayerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<Phase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_actor: Option<PlayerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement: Option<MovementFact>,
    /// Stored only in authoritative history; stripped from every projected event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat_detail: Option<SeatDecisionDetail>,
    /// Only populated on the acting seat's projected copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_detail: Option<String>,
    /// The roll behind a [`GameEventKind::DiceRolled`] event. Public: dice are rolled in the open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roll: Option<PublicRoll>,
}

/// One public die roll: who rolled, what for, what showed, and what it did.
///
/// Built from the engine's roll history after the fact, so publishing it cannot change any
/// decision or draw. Rolls that decide hidden information (which card Spy takes) are not made
/// public; see [`public_roll`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PublicRoll {
    /// Position in the game's roll history. A restart replays the same rolls; this is how the
    /// replay is told apart from a new roll.
    pub index: usize,
    /// Whose dice these are, when the engine said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roller: Option<PlayerId>,
    /// The engine's tag for the roll ("gravity rift", "space combat").
    pub reason: String,
    /// The tag in words.
    pub label: String,
    pub faces: Vec<u32>,
    /// The face a die needs to hit, for hit rolls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hits_on: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hits: Option<u32>,
    /// Positions of the dice that are rerolls.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rerolled: Vec<usize>,
    /// The system it happened in: a rift exited on the way, else the active system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// What it did ("ship destroyed", "2 hits").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// Rolls whose face decides a hidden draw. Showing the die would show which card was taken.
const HIDDEN_ROLLS: [&str; 1] = ["spy"];

fn roll_label(reason: &str) -> String {
    let words = reason.replace('_', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The public face of roll number `index`, or `None` when it is internal bookkeeping or settles
/// hidden information. `active` is the active system when the engine named none.
#[must_use]
pub fn public_roll(
    index: usize,
    roll: &ti4_engine::dice::Roll,
    active: Option<&str>,
) -> Option<PublicRoll> {
    let reason = roll.reason.split(':').next().unwrap_or(&roll.reason);
    if roll.faces.is_empty()
        || HIDDEN_ROLLS.contains(&reason)
        || reason.contains("rollback")
        || reason.contains("probe")
    {
        return None;
    }
    let hits = roll
        .hits_on
        .map(|_| u32::try_from(roll.hits()).unwrap_or(u32::MAX));
    let outcome = roll.outcome.clone().or_else(|| {
        hits.map(|hits| match hits {
            0 => "no hits".to_owned(),
            1 => "1 hit".to_owned(),
            n => format!("{n} hits"),
        })
    });
    Some(PublicRoll {
        index,
        roller: roll.by.as_deref().map(PlayerId::new),
        reason: roll.reason.clone(),
        label: roll_label(reason),
        faces: roll.faces.clone(),
        hits_on: roll.hits_on,
        hits,
        rerolled: roll.rerolled.iter().copied().collect(),
        system: roll.system.clone().or_else(|| active.map(str::to_owned)),
        outcome,
    })
}

impl PublicRoll {
    /// The event-log line: `p1 rolled Gravity rift in system 25: 2 (ship destroyed)`.
    #[must_use]
    pub fn describe(&self) -> String {
        let faces = self
            .faces
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let who = self
            .roller
            .as_ref()
            .map_or_else(|| "The game".to_owned(), ToString::to_string);
        let place = self
            .system
            .as_ref()
            .map_or_else(String::new, |system| format!(" in system {system}"));
        let outcome = self
            .outcome
            .as_ref()
            .map_or_else(String::new, |outcome| format!(" ({outcome})"));
        format!("{who} rolled {}{place}: {faces}{outcome}", self.label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SeatDecisionDetail {
    pub seat: PlayerId,
    pub detail: String,
}

impl GameEvent {
    #[must_use]
    pub fn for_viewer(&self, viewer: &ViewerRole) -> Option<Self> {
        if !self.visibility.permits(viewer) {
            return None;
        }
        let mut projected = self.clone();
        projected.private_detail = self
            .seat_detail
            .as_ref()
            .and_then(|fact| viewer.is_actor(&fact.seat).then(|| fact.detail.clone()));
        projected.seat_detail = None;
        Some(projected)
    }
}

/// Stable decision-cursor identity of the most recent engine action selection.
/// Histories predating action IDs still use the prompt boundary as a fallback.
#[must_use]
pub fn action_id_for(
    records: &[ti4_engine::choice::DecisionRecord],
    cursor: usize,
) -> Option<String> {
    records
        .get(..cursor)?
        .iter()
        .rposition(|record| record.prompt == "action phase")
        .map(|start| format!("action_{}", start + 1))
}

/// Public identifiers only. Unknown subtypes never turn into player-facing prose.
#[must_use]
pub fn stage_for(subtype: &str) -> Option<&'static str> {
    Some(match subtype {
        "draft_strategy_card" => "draft",
        "starting_technology" => "setup",
        "score_objective" | "score_secret_objective" => "scoring",
        "cast_vote"
        | "vote_exhaust_planet"
        | "vote_predictive_intelligence"
        | "vote_tiebreak" => "voting",
        "propose_transaction" | "answer_transaction" => "transactions",
        "activate_system" => "activation",
        "movement_step" | "load_cargo" => "movement",
        "assign_casualty"
        | "sustain_damage"
        | "letnev_commander_trade_good"
        | "announce_retreat"
        | "retreat_to" => "combat",
        "assign_ground_casualty"
        | "fight_ground_combat_round"
        | "start_next_ground_combat"
        | "commit_ground_forces"
        | "bombardment_target"
        | "remove_custodians" => "invasion",
        "produce_unit" | "pay_resources" | "pay_influence" => "production",
        "gain_command_token"
        | "buy_token_with_influence"
        | "research_technology"
        | "ready_planet"
        | "diplomacy_choose_system"
        | "politics_choose_speaker"
        | "politics_place_agenda"
        | "place_structure"
        | "trade_choose_replenish"
        | "warfare_recall_token"
        | "warfare_redistribute_tokens"
        | "imperial_score_objective"
        | "warfare_free_tactical"
        | "construction_choose_ability" => "strategy",
        "discard_over_hand_limit"
        | "expedition_discard_action_card"
        | "return_over_secret_hand_limit"
        | "expedition_discard_secret"
        | "legendary_galactic_council" => "hand management",
        s if s.starts_with("reaction_when_")
            || s.starts_with("reaction_after_")
            || s.starts_with("play_reaction_") =>
        {
            "reactions"
        }
        s if s.starts_with("leader_") => "leaders",
        s if s.starts_with("legendary_") => "legendary abilities",
        s if s.starts_with("relic_") => "relics",
        s if s.starts_with("agenda_") => "agenda effects",
        s if s.starts_with("explor") => "exploration",
        s if s.starts_with("status_") => "status",
        s if s.starts_with("technology_") || s.starts_with("research_") => "technology",
        s if s.starts_with("trade_") => "trade",
        s if s.starts_with("warfare_") => "warfare",
        s if s.starts_with("politics_") => "politics",
        s if s.starts_with("diplomacy_") => "diplomacy",
        s if s.starts_with("construction_") => "construction",
        _ => "effects",
    })
}

fn strategy_card_name(id: &str) -> Option<String> {
    ti4_engine::strategy_cards::card_name(ti4_content::ContentStore::embedded(), id)
}

/// Name only verified public choices. Option labels can contain hidden cards or agendas.
fn public_choice_detail(
    context: &ti4_engine::decision_context::DecisionContext,
    option: &ChoiceOption,
    actor: &PlayerId,
) -> Option<String> {
    let id = option.id.as_str();
    let detail = match context.subtype.as_str() {
        "draft_strategy_card" if option.kind == "strategy_card" => {
            format!("{actor} picked {}", strategy_card_name(id)?)
        }
        // Technology cards lie face up in the play area from the start: a public fact.
        "starting_technology" if option.kind == "starting_technology" => format!(
            "{actor} starts with {}",
            ti4_engine::technology::name(
                ti4_content::ContentStore::embedded(),
                &ti4_model::id::TechnologyId::new(id)
            )
        ),
        "activate_system" if option.kind == "activate" => format!("{actor} activated #{id}"),
        "gain_command_token" if option.kind == "pool" => {
            let pool = match id {
                "tactic_tokens" => "tactic",
                "fleet_tokens" => "fleet",
                "strategic_tokens" => "strategy",
                _ => return None,
            };
            format!("{actor} gained a {pool} command token")
        }
        "buy_token_with_influence" => match id {
            "yes" => format!("{actor} bought a command token with influence"),
            "no" => format!("{actor} stopped buying command tokens"),
            _ => return None,
        },
        "research_technology" if option.kind == "research" => {
            format!(
                "{actor} researched {}",
                ti4_engine::technology::name(
                    ti4_content::ContentStore::embedded(),
                    &ti4_model::id::TechnologyId::new(id)
                )
            )
        }
        "research_technology" if option.is_decline() => {
            format!("{actor} declined to research a technology")
        }
        "ready_planet" if option.kind == "ready" => format!("{actor} readied {id}"),
        "diplomacy_choose_system" if option.kind == "system" => {
            format!("{actor} chose #{id} for Diplomacy")
        }
        "politics_choose_speaker" if option.kind == "speaker" => {
            format!("{actor} chose {id} as speaker")
        }
        "trade_choose_replenish" if option.kind == "replenish" => {
            format!("{actor} replenished {id}'s commodities")
        }
        "trade_choose_replenish" if option.is_decline() => {
            format!("{actor} finished replenishing commodities")
        }
        "warfare_recall_token" if option.kind == "recall" => {
            format!("{actor} recalled a command token from #{id}")
        }
        "warfare_free_tactical" if option.kind == "activate" => {
            format!("{actor} chose a free tactical action in #{id}")
        }
        "place_structure" if option.kind == "build" => {
            let mut parts = id.split('|');
            let (Some(unit), Some(_system), Some(planet), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return None;
            };
            format!("{actor} placed {unit} on {planet}")
        }
        "place_structure" if option.is_decline() => {
            format!("{actor} declined to place a structure")
        }
        "cast_vote" if option.kind == "vote" => {
            if matches!(id, "for" | "against") {
                format!("{actor} voted {id}")
            } else {
                format!("{actor} voted for {id}")
            }
        }
        "cast_vote" if option.is_decline() => format!("{actor} abstained from voting"),
        "vote_tiebreak" if option.kind == "tiebreak" => {
            format!("{actor} broke the tie in favor of {id}")
        }
        "propose_transaction" if option.is_decline() => format!("{actor} ended negotiations"),
        "answer_transaction" => match id {
            "accept" => format!("{actor} accepted the transaction"),
            "refuse" => format!("{actor} refused the transaction"),
            "counter" => format!("{actor} made a counteroffer"),
            _ => return None,
        },
        "imperial_score_objective" if option.kind == "objective" => {
            format!("{actor} scored public objective {id} with Imperial")
        }
        "imperial_score_objective" if option.is_decline() => {
            format!("{actor} declined to score with Imperial")
        }
        "politics_place_agenda" if matches!(id, "top" | "bottom") => {
            format!("{actor} arranged a looked-at agenda")
        }
        "discard_over_hand_limit" | "expedition_discard_action_card" => {
            format!("{actor} discarded an action card")
        }
        "return_over_secret_hand_limit"
        | "expedition_discard_secret"
        | "legendary_galactic_council" => {
            format!("{actor} returned an unscored secret objective")
        }
        "score_objective"
        | "score_secret_objective"
        | "movement_step"
        | "load_cargo"
        | "produce_unit"
        | "pay_resources"
        | "pay_influence"
        | "vote_exhaust_planet"
        | "assign_casualty"
        | "assign_ground_casualty"
        | "sustain_damage"
        | "announce_retreat"
        | "retreat_to"
        | "fight_ground_combat_round"
        | "start_next_ground_combat"
        | "commit_ground_forces"
        | "bombardment_target"
        | "remove_custodians"
        | "play_card" => return None,
        s if s.starts_with("reaction_when_")
            || s.starts_with("reaction_after_")
            || s.starts_with("play_reaction_") =>
        {
            return None;
        }
        subtype => {
            // The subtype identifies the public question; its selected option may be private.
            format!("{actor} resolved {}", subtype.replace('_', " "))
        }
    };
    Some(detail)
}

pub type DecisionGrouping = (
    Option<PlayerId>,
    Option<u32>,
    Option<Phase>,
    Option<String>,
    Option<PlayerId>,
    Option<String>,
);

#[must_use]
pub fn decision_grouping(
    record: &ti4_engine::choice::DecisionRecord,
    offered: Option<&ChoiceOption>,
    records: &[ti4_engine::choice::DecisionRecord],
    cursor: usize,
) -> DecisionGrouping {
    let context = record.context.as_ref();
    let selection = records
        .iter()
        .take(cursor)
        .rfind(|r| r.prompt == "action phase");
    let selected_type = |id: &str| {
        match id {
            "tactical" => "tactical",
            "pass" => "pass",
            "strategic" => "strategic",
            id if id.starts_with("strategic|") => "strategic",
            _ => "component",
        }
        .to_owned()
    };
    let action_type = selection.map(|r| selected_type(&r.chosen));
    let selected_type = if record.prompt == "action phase" {
        // The recorded chosen ID came from the verified engine offer.
        offered
            .filter(|o| o.id == record.chosen)
            .map(|o| selected_type(&o.id))
            .or_else(|| action_type.clone())
    } else {
        action_type
    };
    (
        Some(record.player.clone()),
        context.map(|c| c.round),
        context.map(|c| c.phase),
        selected_type,
        selection.map(|r| r.player.clone()),
        if record.prompt == "action phase" {
            Some("action selection".to_owned())
        } else {
            context
                .and_then(|c| stage_for(&c.subtype))
                .map(str::to_owned)
        },
    )
}

/// Public facts derived from the engine's offered option, never from a submitted plan.
/// Unrecognized choices stay generic so private option payloads cannot enter the public log.
#[must_use]
pub fn public_decision_facts(
    record: &ti4_engine::choice::DecisionRecord,
    offered: Option<&ChoiceOption>,
    destination: Option<&str>,
) -> (Option<String>, Option<MovementFact>) {
    let Some(context) = record.context.as_ref() else {
        return (None, None);
    };
    let Some(option) = offered.filter(|option| option.id == record.chosen) else {
        return (None, None);
    };
    let actor = &record.player;
    let detail = played_card_detail(context, option, actor)
        .or_else(|| combat_decision_detail(context, option, actor))
        .or_else(|| {
            match (
                context.subtype.as_str(),
                option.kind.as_str(),
                option.id.as_str(),
            ) {
                ("pay_resources" | "pay_influence", "pay", "trade_good") => {
                    Some(format!("{actor} spent a trade good"))
                }
                ("pay_resources" | "pay_influence", "pay", id) => id
                    .strip_prefix("exhaust|")
                    .filter(|planet| !planet.is_empty())
                    .map(|planet| format!("{actor} exhausted {planet}")),
                ("vote_exhaust_planet", _, "decline") => Some("Done voting".into()),
                ("vote_exhaust_planet", "vote_planet", _) => {
                    Some(format!("{actor} exhausted {} to vote", option.id))
                }
                ("produce_unit", _, "done_producing") => Some("Done producing".into()),
                ("produce_unit", "produce", _) => {
                    let unit = option
                        .payload
                        .get("unit")
                        .and_then(serde_json::Value::as_str);
                    let count = option
                        .payload
                        .get("count")
                        .and_then(serde_json::Value::as_u64);
                    unit.zip(count)
                        .map(|(unit, count)| format!("{actor} produced {count} {unit}"))
                }
                ("load_cargo", _, "done_loading") => Some("Done loading".into()),
                ("load_cargo", "load", _) => option
                    .payload
                    .get("unit")
                    .and_then(serde_json::Value::as_str)
                    .map(|unit| format!("{actor} loaded {unit}")),
                ("movement_step", _, "done_moving") => Some("Done moving".into()),
                ("movement_step", "move", _) => {
                    let origin = option
                        .payload
                        .get("origin")
                        .and_then(serde_json::Value::as_str);
                    let unit = option
                        .payload
                        .get("unit")
                        .and_then(serde_json::Value::as_str);
                    origin
                        .zip(unit)
                        .zip(destination)
                        .map(|((origin, unit), destination)| {
                            format!("{actor} moved {unit} from #{origin} to #{destination}")
                        })
                }
                _ => None,
            }
        })
        .or_else(|| public_choice_detail(context, option, actor));
    let movement = if context.subtype == "movement_step" && option.kind == "move" {
        option
            .payload
            .get("origin")
            .and_then(serde_json::Value::as_str)
            .zip(
                option
                    .payload
                    .get("unit")
                    .and_then(serde_json::Value::as_str),
            )
            .zip(destination)
            .map(|((origin, unit), destination)| MovementFact {
                actor: actor.clone(),
                origin: origin.into(),
                destination: destination.into(),
                unit: unit.into(),
            })
    } else {
        None
    };
    (detail, movement)
}

/// Only the card explicitly named by the engine's selected offer can become a public fact.
/// A multi-card outer offer contains no card; its inner selected offer carries the alias.
pub fn played_card_detail(
    context: &ti4_engine::decision_context::DecisionContext,
    option: &ChoiceOption,
    actor: &ti4_model::id::PlayerId,
) -> Option<String> {
    if !(context.subtype.starts_with("reaction_when_")
        || context.subtype.starts_with("reaction_after_")
        || context.subtype.starts_with("play_reaction_"))
        || !matches!(option.kind.as_str(), "ability" | "action_card")
    {
        return None;
    }
    let alias = option.payload.get("card")?.as_str()?;
    let name = option.payload.get("card_name")?.as_str()?;
    let actual = ti4_engine::action_cards::name_of(
        ti4_content::ContentStore::embedded(),
        &ti4_model::id::ActionCardId::new(alias),
    );
    (actual == name && actual != alias).then(|| format!("{actor} played {actual}"))
}

/// Combat choices are public board decisions. Use only known context/option pairs and
/// allowlisted payload fields; option labels and opaque IDs may contain private data.
fn combat_decision_detail(
    context: &ti4_engine::decision_context::DecisionContext,
    option: &ChoiceOption,
    actor: &ti4_model::id::PlayerId,
) -> Option<String> {
    use ti4_engine::decision_context::DecisionTarget;

    let location = match context.target.as_ref()? {
        DecisionTarget::System(system) => format!("in #{system}"),
        DecisionTarget::Planet { planet, .. } => format!("on {planet}"),
        _ => return None,
    };
    let unit = || {
        option
            .payload
            .get("unit")
            .and_then(serde_json::Value::as_str)
    };
    match (
        context.subtype.as_str(),
        option.kind.as_str(),
        option.id.as_str(),
    ) {
        ("assign_casualty" | "assign_ground_casualty", "casualty" | "ground_casualty", _)
            if option.id.starts_with("destroy|") =>
        {
            let damage = if option
                .payload
                .get("damaged")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                "damaged "
            } else {
                ""
            };
            unit().map(|unit| format!("{actor} lost a {damage}{unit} {location}"))
        }
        ("sustain_damage", "sustain", _) if option.id.starts_with("sustain|") => {
            unit().map(|unit| format!("{actor} sustained damage on a {unit} {location}"))
        }
        ("sustain_damage", "decline", "decline") => {
            Some(format!("{actor} took the hit {location}"))
        }
        ("announce_retreat", "retreat", "retreat") => {
            Some(format!("{actor} announced a retreat {location}"))
        }
        ("announce_retreat", "retreat", "stay") => {
            Some(format!("{actor} stayed to fight {location}"))
        }
        ("retreat_to", "retreat_to", _) => option
            .payload
            .get("system")
            .and_then(serde_json::Value::as_str)
            .filter(|system| *system == option.id)
            .map(|system| {
                format!(
                    "{actor} retreated from {} to #{system}",
                    location.strip_prefix("in ").unwrap_or(&location)
                )
            }),
        ("fight_ground_combat_round", "ground_casualty", "fight") => {
            Some(format!("{actor} fought a ground combat round {location}"))
        }
        ("start_next_ground_combat", "ground_casualty", _) if option.id.starts_with("fight|") => {
            Some(format!("{actor} started another ground combat {location}"))
        }
        ("start_next_ground_combat", "decline", "decline") => {
            Some(format!("{actor} declined another ground combat {location}"))
        }
        ("commit_ground_forces", "commit", _) if option.id.starts_with("commit|") => {
            let planet = option
                .payload
                .get("planet")
                .and_then(serde_json::Value::as_str)?;
            let unit = unit()?;
            let damage = if option
                .payload
                .get("damaged")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                "damaged "
            } else {
                ""
            };
            Some(format!("{actor} landed {damage}{unit} on {planet}"))
        }
        ("commit_ground_forces", "decline", "done_committing") => {
            Some(format!("{actor} finished landing {location}"))
        }
        ("bombardment_target", "bombardment_target", _) => Some(format!(
            "{actor} targeted {} with bombardment {location}",
            option.id
        )),
        ("remove_custodians", "custodians", "yes") => {
            Some(format!("{actor} removed the custodians token {location}"))
        }
        _ => None,
    }
}

/// Derive public facts and an explicitly seat-only detail from the same offered option.
/// Only known private contexts are allowed to expose their selected ID to their actor.
#[must_use]
pub fn decision_facts(
    record: &ti4_engine::choice::DecisionRecord,
    offered: Option<&ChoiceOption>,
    destination: Option<&str>,
) -> (
    Option<String>,
    Option<MovementFact>,
    Option<SeatDecisionDetail>,
) {
    let (detail, movement) = public_decision_facts(record, offered, destination);
    let private = record
        .context
        .as_ref()
        .zip(offered)
        .and_then(|(context, option)| {
            if option.id != record.chosen || option.id.is_empty() || option.id.len() > 128 {
                return None;
            }
            let description = match context.subtype.as_str() {
                "score_secret_objective" if option.kind == "score" => {
                    Some(format!("Scored secret objective {}", option.id))
                }
                "score_objective" if option.kind == "score" => {
                    Some(format!("Selected objective {}", option.id))
                }
                "legendary_galactic_council" if option.kind == "legendary" => {
                    Some(format!("Discarded secret objective {}", option.id))
                }
                _ => None,
            }?;
            Some(SeatDecisionDetail {
                seat: record.player.clone(),
                detail: description,
            })
        });
    (detail, movement, private)
}

/// The shared live/batch commit path verifies a card was announced by the engine.
/// Selecting an outer reaction is not sufficient evidence if resolution stops before `play`.
#[must_use]
pub fn verified_decision_facts(
    record: &ti4_engine::choice::DecisionRecord,
    offered: Option<&ChoiceOption>,
    destination: Option<&str>,
    state: &ti4_model::state::GameState,
) -> (
    Option<String>,
    Option<MovementFact>,
    Option<SeatDecisionDetail>,
) {
    let (mut detail, movement, mut private) = decision_facts(record, offered, destination);
    if let Some(option) = offered.filter(|option| option.id == record.chosen)
        && let Some(context) = record.context.as_ref()
        && matches!(
            context.subtype.as_str(),
            "score_objective" | "score_secret_objective"
        )
        && option.kind == "score"
        && state
            .scored_objectives
            .get(&record.player)
            .is_some_and(|scored| scored.contains(&ti4_model::id::ObjectiveId::new(&option.id)))
    {
        // A scored secret is revealed; an offered but unscored secret is not.
        detail = Some(format!("{} scored objective {}", record.player, option.id));
        private = None;
    }
    // The action selection says what was chosen. A recorded selection carries its `action_menu`
    // context, for which the generic fallback ("resolved action menu") would hide the card.
    let generic = record
        .context
        .as_ref()
        .is_some_and(|context| context.subtype == "action_menu");
    if (detail.is_none() || generic)
        && record.prompt == "action phase"
        && let Some(option) = offered.filter(|option| option.id == record.chosen)
    {
        detail = match option.id.as_str() {
            "tactical" => Some(format!("{} began a tactical action", record.player)),
            "pass" => Some(format!("{} passed their turn", record.player)),
            // One strategy card: the web names it from the seat's single unexhausted card.
            "strategic" => Some(format!("{} began a strategic action", record.player)),
            id if id.starts_with("strategic|") => id
                .strip_prefix("strategic|")
                .and_then(strategy_card_name)
                .map(|name| format!("{} played {name}", record.player))
                .or(detail),
            _ => Some(format!("{} began a component action", record.player)),
        };
    }
    if let Some(option) = offered
        && let Some(context) = record.context.as_ref()
        && played_card_detail(context, option, &record.player).is_some()
    {
        let announced = option
            .payload
            .get("card")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|alias| {
                state
                    .action_card_plays
                    .iter()
                    .any(|(seat, card)| seat == &record.player && card.as_str() == alias)
            });
        if !announced {
            detail = None;
        }
    }
    (detail, movement, private)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MovementFact {
    pub actor: PlayerId,
    pub origin: String,
    pub destination: String,
    pub unit: String,
}

#[cfg(test)]
mod roll_tests {
    use super::*;
    use std::collections::BTreeSet;
    use ti4_engine::dice::Roll;

    fn roll(reason: &str, faces: &[u32], hits_on: Option<u32>) -> Roll {
        Roll {
            reason: reason.to_owned(),
            faces: faces.to_vec(),
            hits_on,
            rerolled: BTreeSet::new(),
            by: Some("p1".to_owned()),
            system: None,
            outcome: None,
        }
    }

    #[test]
    fn a_gravity_rift_roll_says_who_what_for_the_die_the_system_and_the_outcome() {
        let mut rift = roll("gravity rift", &[2], Some(4));
        rift.system = Some("67".to_owned());
        rift.outcome = Some("ship destroyed".to_owned());
        let public = public_roll(5, &rift, Some("01")).unwrap();
        assert_eq!(public.index, 5);
        assert_eq!(public.roller, Some(PlayerId::new("p1")));
        assert_eq!(public.label, "Gravity rift");
        assert_eq!(public.faces, vec![2]);
        assert_eq!(
            public.system.as_deref(),
            Some("67"),
            "the rift, not the active system"
        );
        assert_eq!(
            public.describe(),
            "p1 rolled Gravity rift in system 67: 2 (ship destroyed)"
        );
    }

    #[test]
    fn hit_rolls_count_their_hits_and_default_to_the_active_system() {
        let public =
            public_roll(0, &roll("space combat", &[8, 3, 10], Some(7)), Some("18")).unwrap();
        assert_eq!(public.hits, Some(2));
        assert_eq!(public.outcome.as_deref(), Some("2 hits"));
        assert_eq!(public.system.as_deref(), Some("18"));
        let miss = public_roll(1, &roll("space combat", &[1], Some(7)), None).unwrap();
        assert_eq!(miss.outcome.as_deref(), Some("no hits"));
        assert_eq!(miss.system, None);
        let anti_fighter =
            public_roll(2, &roll("anti-fighter barrage", &[9], Some(9)), None).unwrap();
        assert_eq!(anti_fighter.label, "Anti-fighter barrage");
        assert_eq!(anti_fighter.outcome.as_deref(), Some("1 hit"));
    }

    #[test]
    fn rerolls_report_which_dice_were_replaced() {
        let mut again = roll("space combat:reroll", &[9, 2], Some(7));
        again.rerolled = BTreeSet::from([1]);
        let public = public_roll(3, &again, None).unwrap();
        assert_eq!(public.rerolled, vec![1]);
        assert_eq!(public.label, "Space combat");
    }

    #[test]
    fn hidden_draws_and_bookkeeping_rolls_are_not_public() {
        // Spy's die picks a card from a hand: showing it would show which card.
        assert!(public_roll(0, &roll("spy", &[3], None), None).is_none());
        assert!(public_roll(0, &roll("strict rollback", &[3], None), None).is_none());
        assert!(public_roll(0, &roll("space combat", &[], Some(7)), None).is_none());
    }

    #[test]
    fn a_roll_event_round_trips_and_is_visible_to_every_viewer() {
        let public = public_roll(0, &roll("ambush", &[10, 4], Some(8)), Some("75")).unwrap();
        let event = GameEvent {
            id: "g-1".into(),
            timestamp: String::new(),
            version: Some(1),
            visibility: EventVisibility::Public,
            event: GameEventKind::DiceRolled,
            decision_count: Some(0),
            batch_id: None,
            batch_start_cursor: None,
            batch_end_cursor: None,
            action_id: None,
            action_start_cursor: None,
            actor: public.roller.clone(),
            round: None,
            phase: None,
            action_type: None,
            action_actor: None,
            stage: None,
            detail: Some(public.describe()),
            movement: None,
            seat_detail: None,
            private_detail: None,
            roll: Some(public),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"]["kind"], "dice_rolled");
        assert_eq!(json["roll"]["faces"], serde_json::json!([10, 4]));
        assert_eq!(serde_json::from_value::<GameEvent>(json).unwrap(), event);
        for viewer in [
            ViewerRole::Spectator,
            ViewerRole::Player(PlayerId::new("p2")),
        ] {
            assert_eq!(event.for_viewer(&viewer).unwrap().roll, event.roll);
        }
    }
}

#[cfg(test)]
mod fact_tests {
    use super::*;
    use ti4_engine::choice::DecisionRecord;
    use ti4_engine::decision_context::{DecisionContext, DecisionSource, DecisionTarget};
    use ti4_model::id::{PlanetId, SystemId};

    fn record(subtype: &str, option: &ChoiceOption) -> DecisionRecord {
        let player = PlayerId::new("p1");
        DecisionRecord {
            player: player.clone(),
            prompt: "test".into(),
            chosen: option.id.clone(),
            offered: vec![option.id.clone()],
            context: Some(DecisionContext::new(
                player,
                DecisionSource::Rule("test".into()),
                subtype,
                Phase::Action,
                1,
            )),
        }
    }

    #[test]
    fn common_public_decisions_name_the_selected_outcome() {
        let cases = [
            (
                "draft_strategy_card",
                "strategy_card",
                "pok1leadership",
                "p1 picked Leadership",
            ),
            ("activate_system", "activate", "22", "p1 activated #22"),
            (
                "gain_command_token",
                "pool",
                "fleet_tokens",
                "p1 gained a fleet command token",
            ),
            (
                "buy_token_with_influence",
                "strategy",
                "yes",
                "p1 bought a command token with influence",
            ),
            ("ready_planet", "ready", "jord", "p1 readied jord"),
            (
                "diplomacy_choose_system",
                "system",
                "22",
                "p1 chose #22 for Diplomacy",
            ),
            ("cast_vote", "vote", "for", "p1 voted for"),
            (
                "cast_vote",
                "decline",
                "decline",
                "p1 abstained from voting",
            ),
            (
                "vote_tiebreak",
                "tiebreak",
                "against",
                "p1 broke the tie in favor of against",
            ),
            (
                "warfare_recall_token",
                "recall",
                "22",
                "p1 recalled a command token from #22",
            ),
            (
                "place_structure",
                "build",
                "pds|22|jord",
                "p1 placed pds on jord",
            ),
        ];
        for (subtype, kind, id, expected) in cases {
            let option = ChoiceOption::new(id, kind);
            let record = record(subtype, &option);
            assert_eq!(
                public_decision_facts(&record, Some(&option), None)
                    .0
                    .as_deref(),
                Some(expected),
                "{subtype}"
            );
            assert_eq!(
                public_decision_facts(&record, None, None).0,
                None,
                "{subtype}"
            );
        }
    }

    #[test]
    fn scoring_is_public_only_after_an_award_and_private_hands_stay_hidden() {
        let score = ChoiceOption::new("hidden_objective", "score");
        let selected = record("score_objective", &score);
        let mut state = GameState::new(&[PlayerId::new("p1")], &[], BTreeMap::new(), None, 42);
        let before = verified_decision_facts(&selected, Some(&score), None, &state);
        assert_eq!(before.0, None);
        assert_eq!(
            before.2.unwrap().detail,
            "Selected objective hidden_objective"
        );
        state.record_score(
            &PlayerId::new("p1"),
            ti4_model::id::ObjectiveId::new("hidden_objective"),
        );
        let after = verified_decision_facts(&selected, Some(&score), None, &state);
        assert_eq!(
            after.0.as_deref(),
            Some("p1 scored objective hidden_objective")
        );
        assert_eq!(after.2, None);
        let declined = ChoiceOption::decline();
        for subtype in ["score_objective", "score_secret_objective"] {
            assert_eq!(
                verified_decision_facts(&record(subtype, &declined), Some(&declined), None, &state)
                    .0,
                None
            );
        }
        for (subtype, id) in [
            ("politics_place_agenda", "top"),
            ("expedition_discard_action_card", "0"),
            ("return_over_secret_hand_limit", "hidden_objective"),
        ] {
            let option = ChoiceOption::new(id, "private").with("card", "hidden_card");
            let description = public_decision_facts(&record(subtype, &option), Some(&option), None)
                .0
                .unwrap();
            assert!(!description.contains("hidden_objective"));
            assert!(!description.contains("hidden_card"));
            if subtype == "politics_place_agenda" {
                assert!(!description.contains("top"));
            }
        }
    }

    #[test]
    fn action_selections_and_uncategorized_effects_have_a_useful_path() {
        let state = GameState::new(&[PlayerId::new("p1")], &[], BTreeMap::new(), None, 42);
        let action = ChoiceOption::new("strategic|pok2diplomacy", "action");
        let selection = DecisionRecord {
            prompt: "action phase".into(),
            context: None,
            ..record("select_action", &action)
        };
        assert_eq!(
            verified_decision_facts(&selection, Some(&action), None, &state)
                .0
                .as_deref(),
            Some("p1 played Diplomacy")
        );
        assert_eq!(
            decision_grouping(
                &selection,
                Some(&action),
                std::slice::from_ref(&selection),
                1
            )
            .5
            .as_deref(),
            Some("action selection")
        );
        // Nightly 2026-10-14 #11 (s2-36-0118 and five more server runs): a recorded selection
        // carries its `action_menu` context, whose generic "resolved action menu" hid the card.
        // The web then took the primary's other, still unexhausted card for the action, a
        // follower prepared a secondary for that card, and the plan was dropped before its window.
        for (id, expected) in [
            ("strategic|pok2diplomacy", "p1 played Diplomacy"),
            ("tactical", "p1 began a tactical action"),
            ("pass", "p1 passed their turn"),
            ("strategic", "p1 began a strategic action"),
        ] {
            let option = ChoiceOption::new(id, "action");
            let selection = DecisionRecord {
                prompt: "action phase".into(),
                ..record("action_menu", &option)
            };
            assert_eq!(
                verified_decision_facts(&selection, Some(&option), None, &state)
                    .0
                    .as_deref(),
                Some(expected)
            );
        }
        let effect = ChoiceOption::new("unknown", "effect").with("card", "hidden_card");
        let detail = public_decision_facts(&record("activate_relic", &effect), Some(&effect), None)
            .0
            .unwrap();
        assert_eq!(detail, "p1 resolved activate relic");
        assert_eq!(stage_for("activate_relic"), Some("effects"));
    }

    #[test]
    fn card_play_facts_require_a_selected_engine_card_and_validate_its_identity() {
        let sole = ChoiceOption::labelled(
            "reaction:sol:HITS_TO_ASSIGN:when",
            "ability",
            "Play Shields Holding",
        )
        .with("card", "sh1")
        .with("card_name", "Shields Holding");
        let inner = ChoiceOption::labelled("sh1", "action_card", "play Shields Holding")
            .with("card", "sh1")
            .with("card_name", "Shields Holding");
        for (subtype, option) in [
            ("reaction_when_HITS_TO_ASSIGN", &sole),
            ("play_reaction_when_HITS_TO_ASSIGN", &inner),
        ] {
            let decision = record(subtype, option);
            assert_eq!(
                public_decision_facts(&decision, Some(option), None)
                    .0
                    .as_deref(),
                Some("p1 played Shields Holding")
            );
            assert_eq!(public_decision_facts(&decision, None, None).0, None);
            assert_eq!(
                public_decision_facts(&decision, Some(&ChoiceOption::decline()), None).0,
                None
            );
        }
        let multiple = ChoiceOption::labelled(
            "reaction:sol:HITS_TO_ASSIGN:when",
            "ability",
            "Choose an action card…",
        );
        assert_eq!(
            public_decision_facts(
                &record("reaction_when_HITS_TO_ASSIGN", &multiple),
                Some(&multiple),
                None
            )
            .0,
            None
        );
        let forged = sole.with("card_name", "Direct Hit");
        assert_eq!(
            public_decision_facts(
                &record("reaction_when_HITS_TO_ASSIGN", &forged),
                Some(&forged),
                None
            )
            .0,
            None
        );
    }

    #[test]
    fn selected_but_unannounced_card_is_not_a_public_play() {
        let card = ChoiceOption::labelled("sh1", "action_card", "play Shields Holding")
            .with("card", "sh1")
            .with("card_name", "Shields Holding");
        let decision = record("play_reaction_when_HITS_TO_ASSIGN", &card);
        let mut state = ti4_model::state::GameState::new(
            &[PlayerId::new("p1")],
            &[],
            std::collections::BTreeMap::new(),
            None,
            42,
        );
        assert_eq!(
            verified_decision_facts(&decision, Some(&card), None, &state).0,
            None
        );
        state
            .action_card_plays
            .push((PlayerId::new("p1"), ti4_model::id::ActionCardId::new("sh1")));
        assert_eq!(
            verified_decision_facts(&decision, Some(&card), None, &state)
                .0
                .as_deref(),
            Some("p1 played Shields Holding")
        );
    }

    #[test]
    fn movement_facts_require_a_matching_offered_option_and_destination() {
        let move_option = ChoiceOption::new("move|16|fighter", "move")
            .with("origin", "16")
            .with("unit", "fighter");
        let record = record("movement_step", &move_option);
        let (detail, movement) = public_decision_facts(&record, Some(&move_option), Some("22"));
        assert_eq!(detail.as_deref(), Some("p1 moved fighter from #16 to #22"));
        assert_eq!(movement.unwrap().destination, "22");
        assert_eq!(
            public_decision_facts(&record, Some(&move_option), None),
            (None, None)
        );
        assert_eq!(
            public_decision_facts(
                &record,
                Some(&ChoiceOption::new("other", "move")),
                Some("22")
            ),
            (None, None)
        );
    }

    #[test]
    fn combat_facts_describe_casualties_sustain_and_retreat() {
        let system = DecisionTarget::System(SystemId::new("22"));
        let planet = DecisionTarget::Planet {
            system: SystemId::new("22"),
            planet: PlanetId::new("jord"),
        };
        let cases = [
            (
                "assign_casualty",
                "casualty",
                "destroy|4",
                Some("dreadnought"),
                system.clone(),
                "p1 lost a damaged dreadnought in #22",
            ),
            (
                "assign_ground_casualty",
                "ground_casualty",
                "destroy|1",
                Some("infantry"),
                planet.clone(),
                "p1 lost a damaged infantry on jord",
            ),
            (
                "sustain_damage",
                "sustain",
                "sustain|2",
                Some("dreadnought"),
                system.clone(),
                "p1 sustained damage on a dreadnought in #22",
            ),
            (
                "sustain_damage",
                "decline",
                "decline",
                None,
                system.clone(),
                "p1 took the hit in #22",
            ),
            (
                "announce_retreat",
                "retreat",
                "stay",
                None,
                system.clone(),
                "p1 stayed to fight in #22",
            ),
            (
                "announce_retreat",
                "retreat",
                "retreat",
                None,
                system.clone(),
                "p1 announced a retreat in #22",
            ),
            (
                "retreat_to",
                "retreat_to",
                "16",
                None,
                system.clone(),
                "p1 retreated from #22 to #16",
            ),
            (
                "fight_ground_combat_round",
                "ground_casualty",
                "fight",
                None,
                planet,
                "p1 fought a ground combat round on jord",
            ),
        ];
        for (subtype, kind, id, unit, target, expected) in cases {
            let mut option = ChoiceOption::new(id, kind);
            if let Some(unit) = unit {
                option = option.with("unit", unit).with("damaged", true);
            }
            if subtype == "retreat_to" {
                option = option.with("system", id);
            }
            let mut record = record(subtype, &option);
            record.context.as_mut().unwrap().target = Some(target);
            assert_eq!(
                public_decision_facts(&record, Some(&option), None)
                    .0
                    .as_deref(),
                Some(expected),
                "{subtype}: {id}"
            );
            assert_eq!(public_decision_facts(&record, None, None).0, None);
        }
    }

    #[test]
    fn combat_facts_ignore_unknown_payloads_and_missing_targets() {
        let option = ChoiceOption::new("destroy|0", "casualty").with("unit", "fighter");
        let mut casualty = record("assign_casualty", &option);
        assert_eq!(
            public_decision_facts(&casualty, Some(&option), None).0,
            None
        );
        casualty.context.as_mut().unwrap().target =
            Some(DecisionTarget::System(SystemId::new("22")));
        assert_eq!(
            public_decision_facts(
                &casualty,
                Some(&ChoiceOption::new("other", "casualty")),
                None
            )
            .0,
            None
        );
        assert_eq!(
            public_decision_facts(
                &record("score_secret_objective", &option),
                Some(&option),
                None
            )
            .0,
            None
        );
    }

    #[test]
    fn invasion_landing_and_bombardment_facts_only_use_verified_offers() {
        let landing = ChoiceOption::new("commit|0|jord", "commit")
            .with("unit", "mech")
            .with("damaged", true)
            .with("planet", "jord");
        let mut decision = record("commit_ground_forces", &landing);
        decision.context.as_mut().unwrap().target =
            Some(DecisionTarget::System(SystemId::new("22")));
        assert_eq!(
            public_decision_facts(&decision, Some(&landing), None)
                .0
                .as_deref(),
            Some("p1 landed damaged mech on jord")
        );
        assert_eq!(public_decision_facts(&decision, None, None).0, None);
        let target = ChoiceOption::new("p2", "bombardment_target");
        let mut bombard = record("bombardment_target", &target);
        bombard.context.as_mut().unwrap().target = Some(DecisionTarget::Planet {
            system: SystemId::new("22"),
            planet: ti4_model::id::PlanetId::new("jord"),
        });
        assert_eq!(
            public_decision_facts(&bombard, Some(&target), None)
                .0
                .as_deref(),
            Some("p1 targeted p2 with bombardment on jord")
        );
    }

    #[test]
    fn public_facts_do_not_echo_unknown_or_private_option_payloads() {
        let secret = ChoiceOption::new("secret_card", "play_card").with("card", "private");
        assert_eq!(
            public_decision_facts(&record("play_card", &secret), Some(&secret), None),
            (None, None)
        );
        let production = ChoiceOption::new("build|fighter|2", "produce")
            .with("unit", "fighter")
            .with("count", 2);
        assert_eq!(
            public_decision_facts(
                &record("produce_unit", &production),
                Some(&production),
                None
            )
            .0
            .as_deref(),
            Some("p1 produced 2 fighter")
        );
    }

    /// The web client decodes this exact file (web/src/protocol/eventVisibility.test.ts).
    #[test]
    fn event_visibility_wire_shape_matches_the_client_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../fixtures/protocol/event_visibility.json"
        ))
        .unwrap();
        let events: Vec<GameEvent> = serde_json::from_value(fixture.clone()).unwrap();
        assert_eq!(events[0].visibility, EventVisibility::Public);
        assert_eq!(
            events[1].visibility,
            EventVisibility::Seat(PlayerId::new("p1"))
        );
        assert_eq!(events[2].visibility, EventVisibility::Referee);
        assert_eq!(serde_json::to_value(&events).unwrap(), fixture);
    }

    #[test]
    fn secret_facts_are_projected_only_to_the_actor() {
        let secret =
            ChoiceOption::new("hidden_objective", "score").with("card", "do not broadcast");
        let record = record("score_secret_objective", &secret);
        let (detail, movement, seat_detail) = decision_facts(&record, Some(&secret), None);
        assert_eq!((detail, movement), (None, None));
        let event = GameEvent {
            id: "event-1".into(),
            timestamp: String::new(),
            version: Some(1),
            visibility: EventVisibility::Public,
            event: GameEventKind::DecisionResolved,
            decision_count: Some(1),
            batch_id: None,
            batch_start_cursor: None,
            batch_end_cursor: None,
            action_id: None,
            action_start_cursor: None,
            actor: None,
            round: None,
            phase: None,
            action_type: None,
            action_actor: None,
            stage: None,
            detail: None,
            movement: None,
            seat_detail,
            private_detail: None,
            roll: None,
        };
        let actor = event
            .for_viewer(&ViewerRole::Player(PlayerId::new("p1")))
            .unwrap();
        assert_eq!(
            actor.private_detail.as_deref(),
            Some("Scored secret objective hidden_objective")
        );
        for viewer in [
            ViewerRole::Player(PlayerId::new("p2")),
            ViewerRole::Spectator,
        ] {
            let projected = event.for_viewer(&viewer).unwrap();
            let json = serde_json::to_string(&projected).unwrap();
            assert!(!json.contains("hidden_objective"));
            assert!(!json.contains("seat_detail"));
            assert!(!json.contains("do not broadcast"));
        }
        assert!(
            serde_json::to_string(&actor)
                .unwrap()
                .contains("private_detail")
        );
        assert!(
            !serde_json::to_string(&actor)
                .unwrap()
                .contains("seat_detail")
        );
        assert_eq!(
            decision_facts(&record, Some(&ChoiceOption::new("other", "score")), None).2,
            None
        );
    }

    #[test]
    fn action_selection_is_included_in_its_own_action_id() {
        let action = DecisionRecord {
            prompt: "action phase".into(),
            ..record("select_action", &ChoiceOption::new("tactical", "action"))
        };
        assert_eq!(
            action_id_for(std::slice::from_ref(&action), 1).as_deref(),
            Some("action_1")
        );
        assert_eq!(
            action_id_for(&[action.clone(), action], 2).as_deref(),
            Some("action_2")
        );
    }

    #[test]
    fn grouping_keeps_reaction_decider_separate_from_action_owner() {
        let selected = ChoiceOption::new("tactical", "action");
        let action = DecisionRecord {
            prompt: "action phase".into(),
            ..record("select_action", &selected)
        };
        let mut reaction = record("reaction_when_HITS_TO_ASSIGN", &ChoiceOption::decline());
        reaction.player = PlayerId::new("p2");
        let records = [action, reaction.clone()];
        let grouping = decision_grouping(&reaction, None, &records, 2);
        assert_eq!(grouping.0, Some(PlayerId::new("p2")));
        assert_eq!(grouping.3.as_deref(), Some("tactical"));
        assert_eq!(grouping.4, Some(PlayerId::new("p1")));
        assert_eq!(grouping.5.as_deref(), Some("reactions"));
    }

    #[test]
    fn pending_turn_prompt_never_reopens_the_previous_action() {
        let player = PlayerId::new("p1");
        let mut state = GameState::new(
            std::slice::from_ref(&player),
            &[],
            std::collections::BTreeMap::new(),
            None,
            42,
        );
        state.phase = Phase::Action;
        let selected = ChoiceOption::new("tactical", "action");
        let action = DecisionRecord {
            prompt: "action phase".into(),
            ..record("select_action", &selected)
        };
        let next_turn = Choice::new(player, "action phase", vec![selected]);
        let path = current_log_path(&state, Some(&next_turn), &[action]).unwrap();
        assert_eq!(path.action_id, None);
        assert_eq!(path.round, state.round);
        state.finished = true;
        assert_eq!(current_log_path(&state, Some(&next_turn), &[]), None);
    }
}

/// Server message carrying a new game event to all subscribers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GameEventMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub entry: GameEvent,
}

/// Versioned state update or replacement snapshot after state transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StateUpdateMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub viewer: ViewerRole,
    pub view: GameView,
    /// The engine's `GameState`, passed through without a projection.
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub state: GameState,
    pub galaxy_layout: GalaxyLayout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_choice: Option<PendingChoiceEnvelope>,
    pub turn_status: PublicTurnStatus,
    #[serde(default, skip_serializing_if = "is_default_history")]
    pub history: HistoryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_path: Option<CurrentLogPath>,
    /// Decisions the engine settled for the receiving seat since the previous update because
    /// exactly one option was legal. Feedback only: never journaled, never sent to other seats.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_resolved: Vec<AutoResolvedNote>,
    /// The receiving seat's own "never offer" choices, by printed card name; absent when it has
    /// none. Always empty for spectators and for every other seat's view.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reaction_modes: BTreeMap<String, ReactionMode>,
    /// What the viewer needs to stage the pending choice of a tactical action
    /// ([`crate::view::tactical`]). Only the seat that is asked has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tactical: Option<crate::view::tactical::TacticalFacts>,
    /// The exploration decks and the planets that are still explored ([`crate::view::exploration`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exploration: Option<crate::view::exploration::ExplorationFacts>,
    /// The action card deck, its discard pile and the hand limit of the viewer
    /// ([`crate::view::action_cards`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_cards: Option<crate::view::action_cards::ActionCardFacts>,
    /// The secret objective deck and what each seat has scored of it
    /// ([`crate::view::secret_objectives`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_objectives: Option<crate::view::secret_objectives::SecretObjectiveFacts>,
    /// Where the victory points of every seat come from ([`crate::view::score`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<crate::view::score::ScoreFacts>,
    /// The agenda deck ([`crate::view::agendas`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agendas: Option<crate::view::agendas::AgendaFacts>,
    /// The strategic action that is resolved now ([`crate::view::secondary`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<crate::view::secondary::SecondaryFacts>,
    /// The reaction window that the viewer is asked in ([`crate::view::reaction`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<crate::view::reaction::ReactionFacts>,
    /// The technology tree of the viewer, while it is asked to research
    /// ([`crate::view::research`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research: Option<crate::view::research::ResearchFacts>,
    /// Who sits in each seat: the name of its player and who plays it
    /// ([`crate::setup::SeatFact`]). Public; absent for a game with no table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<crate::setup::TableSeats>,
}

/// One decision made on a seat's behalf because it had a single legal option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AutoResolvedNote {
    /// Unique per note, so a client can drop a repeat.
    pub id: String,
    /// The question that was not asked.
    pub prompt: String,
    /// What was chosen, as the option was labelled.
    pub selected: String,
    /// Why there was no real choice.
    pub reason: String,
    /// How many identical notes this one stands for (a bill paid in several steps).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub count: u32,
    /// The printed names of the cards of a reaction window that was declined because the seat
    /// set each of them to never offer. Empty for every other note.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passed: Vec<String>,
}

const fn one() -> u32 {
    1
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if signature"
)]
const fn is_one(count: &u32) -> bool {
    *count == 1
}

impl StateUpdateMsg {
    #[must_use]
    pub fn with_reaction_modes(mut self, modes: BTreeMap<String, ReactionMode>) -> Self {
        self.reaction_modes = modes;
        self
    }

    #[must_use]
    pub fn with_history(mut self, cursor: usize, redo_count: usize, generation: u64) -> Self {
        self.history = HistoryStatus {
            cursor,
            redo_count,
            generation,
            can_undo: false,
        };
        self
    }
}

/// Pending choice sent only to the acting seat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PendingChoiceMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub nonce: String,
    pub choice: Choice,
    /// The engine's `GameState`, passed through without a projection.
    #[cfg_attr(feature = "ts", ts(type = "Record<string, unknown>"))]
    pub state: GameState,
    pub galaxy_layout: GalaxyLayout,
}

/// A seat's own bluff settings (private to that seat; carries no game version on purpose, so a
/// stale-version filter never drops it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReactionIntentStateMsg {
    pub protocol_version: u16,
    pub game_id: String,
    /// Declared trigger ids.
    pub triggers: Vec<String>,
    pub max_triggers: usize,
    /// Whether this seat may bluff right now (cards in hand, no Never setting).
    pub eligible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ineligible_reason: Option<String>,
    /// The declaration cannot change before this round starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_until_round: Option<u32>,
    /// A stall budget (per round, per game or table-wide) is used up: no more holds.
    pub budget_used_up: bool,
    /// A hold for this seat is running now: show the Pass control.
    pub holding: bool,
}

/// Why a secondary preview was refused (see [`SecondaryPreviewMsg`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum PreviewRefusal {
    /// Only a seated player can ask.
    NotSeated,
    /// The game is not in a strategic action (or it is not the card the client named).
    NoStrategicAction,
    /// The seat played the card itself.
    IsPrimary,
    /// The seat has already been asked (or is being asked right now).
    AlreadyAsked,
    /// The session is replaying, rewinding, finished or failed.
    Busy,
    /// Too many requests on this connection; retry after the interval in the detail.
    RateLimited,
}

/// What a preview request resolved to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
#[allow(
    clippy::large_enum_variant,
    reason = "one short-lived value per request; the engine value is carried as it is"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum PreviewResult {
    /// The engine's answer: the question, "would not be asked", "complete" or "unavailable".
    Preview {
        preview: ti4_engine::secondary_preview::SecondaryPreview,
    },
    Refused {
        reason: PreviewRefusal,
        detail: String,
    },
}

/// The answer to a `PreviewSecondary` request: what the asking seat would be asked for the
/// secondary in progress if its window opened now. Sent only to the connection that asked;
/// ephemeral (never persisted, logged or broadcast) and carries no game version in the envelope
/// sense, so a stale-version filter never drops it: `as_of_version` says which position it was
/// computed at, and the client re-checks the plan against the real question when it opens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SecondaryPreviewMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub request_id: u64,
    /// The game version of the position the preview was computed from.
    pub as_of_version: u64,
    /// How many decisions the game had recorded then.
    pub as_of_decisions: usize,
    pub card: String,
    pub outcome: PreviewResult,
}

/// Public turn status update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TurnStatusMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub status: PublicTurnStatus,
}

/// Confirmation that an action was validated and accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActionAcceptedMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub option_id: String,
}

/// Notification that an action submission was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActionRejectedMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub reason: RejectionReason,
}

/// Protocol error notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProtocolErrorMsg {
    pub protocol_version: u16,
    pub kind: ErrorKind,
    pub message: String,
}

/// Notification of game completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GameOverMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub winner: Option<PlayerId>,
    pub final_scores: BTreeMap<PlayerId, u32>,
}

/// Keep-alive pong response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PongMsg {
    pub protocol_version: u16,
    pub sequence: u64,
}

/// The answer to `ClientMessage::RunDraft`: where the run of the script ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DraftOutcomeMsg {
    pub protocol_version: u16,
    pub game_id: String,
    /// The `request_id` of the request.
    pub request_id: String,
    /// The version of the game that the script was played on. When the game has a newer one,
    /// the same script may end elsewhere.
    pub base_version: u64,
    /// Boxed: an outcome has up to four updates, and a message is moved through queues.
    pub outcome: Box<crate::planning::draft::DraftOutcome>,
}

/// Where a script that was sent to the live game stopped before its end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ScriptStopped {
    /// How many entries of the script are in the game. 0 when the script was refused: the game
    /// is as before.
    pub applied: usize,
    /// The entries that are not in the game.
    pub remaining: Vec<crate::planning::draft::DraftStep>,
    /// True when the game asked something else first, or went on without the rest; false when
    /// it refused an entry and nothing was sent.
    pub interrupted: bool,
    pub reason: String,
}

/// What became of `ClientMessage::ApplyDraft`, for the seat that sent it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DraftAppliedMsg {
    pub protocol_version: u16,
    pub game_id: String,
    /// The nonce of the request: the choice that the script began with.
    pub nonce: String,
    /// Absent when the game played the whole script.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped: Option<ScriptStopped>,
}

pub use super::staged::SeatStagedMsg;

/// Messages emitted from server to client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ServerMessage {
    InitialSnapshot(InitialSnapshotMsg),
    StateUpdate(StateUpdateMsg),
    PendingChoice(PendingChoiceMsg),
    TurnStatus(TurnStatusMsg),
    ActionAccepted(ActionAcceptedMsg),
    ActionRejected(ActionRejectedMsg),
    Error(ProtocolErrorMsg),
    GameOver(GameOverMsg),
    Pong(PongMsg),
    Event(GameEventMsg),
    DraftOutcome(DraftOutcomeMsg),
    DraftApplied(DraftAppliedMsg),
    /// The seat's own bluff settings. Sent only to the declaring seat, never broadcast.
    ReactionIntentState(ReactionIntentStateMsg),
    /// What a seat would be asked for the secondary in progress, as of now. Sent only to the
    /// connection that asked; ephemeral.
    SecondaryPreview(SecondaryPreviewMsg),
    /// Which ships could move into a system, as of now. Sent only to the connection that asked;
    /// ephemeral.
    ReachInto(super::reach::ReachIntoMsg),
    /// What the seat has staged. Sent only to the connections of that seat that asked for it.
    SeatStaged(SeatStagedMsg),
}

impl ServerMessage {
    /// Returns the protocol version of this server message.
    #[must_use]
    pub fn protocol_version(&self) -> u16 {
        match self {
            Self::InitialSnapshot(m) => m.protocol_version,
            Self::StateUpdate(m) => m.protocol_version,
            Self::PendingChoice(m) => m.protocol_version,
            Self::TurnStatus(m) => m.protocol_version,
            Self::ActionAccepted(m) => m.protocol_version,
            Self::ActionRejected(m) => m.protocol_version,
            Self::Error(m) => m.protocol_version,
            Self::GameOver(m) => m.protocol_version,
            Self::Pong(m) => m.protocol_version,
            Self::Event(m) => m.protocol_version,
            Self::DraftOutcome(m) => m.protocol_version,
            Self::DraftApplied(m) => m.protocol_version,
            Self::ReactionIntentState(m) => m.protocol_version,
            Self::SecondaryPreview(m) => m.protocol_version,
            Self::ReachInto(m) => m.protocol_version,
            Self::SeatStaged(m) => m.protocol_version,
        }
    }

    /// Returns the game ID associated with this message, if applicable.
    #[must_use]
    pub fn game_id(&self) -> Option<&str> {
        match self {
            Self::InitialSnapshot(m) => Some(&m.game_id),
            Self::StateUpdate(m) => Some(&m.game_id),
            Self::PendingChoice(m) => Some(&m.game_id),
            Self::TurnStatus(m) => Some(&m.game_id),
            Self::ActionAccepted(m) => Some(&m.game_id),
            Self::ActionRejected(m) => Some(&m.game_id),
            Self::GameOver(m) => Some(&m.game_id),
            Self::Event(m) => Some(&m.game_id),
            Self::DraftOutcome(m) => Some(&m.game_id),
            Self::DraftApplied(m) => Some(&m.game_id),
            Self::ReactionIntentState(m) => Some(&m.game_id),
            Self::SecondaryPreview(m) => Some(&m.game_id),
            Self::ReachInto(m) => Some(&m.game_id),
            Self::SeatStaged(m) => Some(&m.game_id),
            Self::Error(_) | Self::Pong(_) => None,
        }
    }

    /// Returns the monotonic game version, if applicable.
    #[must_use]
    pub fn game_version(&self) -> Option<u64> {
        match self {
            Self::InitialSnapshot(m) => Some(m.game_version),
            Self::StateUpdate(m) => Some(m.game_version),
            Self::PendingChoice(m) => Some(m.game_version),
            Self::TurnStatus(m) => Some(m.game_version),
            Self::ActionAccepted(m) => Some(m.game_version),
            Self::ActionRejected(m) => Some(m.game_version),
            Self::GameOver(m) => Some(m.game_version),
            Self::Event(m) => m.entry.version,
            // Not versioned on purpose: a stale-version filter must never drop these (the
            // preview names its own `as_of_version`).
            Self::Error(_)
            | Self::Pong(_)
            | Self::DraftOutcome(_)
            | Self::DraftApplied(_)
            | Self::ReactionIntentState(_)
            | Self::SecondaryPreview(_)
            | Self::ReachInto(_)
            | Self::SeatStaged(_) => None,
        }
    }
}
