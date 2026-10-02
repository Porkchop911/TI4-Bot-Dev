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

/// Server submission metadata around the engine's wire-serialized choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingChoiceEnvelope {
    pub nonce: String,
    pub choice: Choice,
}

/// Initial per-viewer snapshot sent upon subscription or reconnection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialSnapshotMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub viewer: ViewerRole,
    pub view: GameView,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
pub struct HistoryStatus {
    pub cursor: usize,
    pub redo_count: usize,
    #[serde(default)]
    pub generation: u64,
}

fn is_default_history(value: &HistoryStatus) -> bool {
    *value == HistoryStatus::default()
}

impl InitialSnapshotMsg {
    #[must_use]
    pub fn with_history(mut self, cursor: usize, redo_count: usize, generation: u64) -> Self {
        self.history = HistoryStatus {
            cursor,
            redo_count,
            generation,
        };
        self
    }
}

/// Explicit audience for an authoritative event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "visibility", content = "seat", rename_all = "snake_case")]
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
pub enum GameEventKind {
    GameInitialized {
        round: u32,
        phase: Phase,
        speaker: PlayerId,
    },
    DecisionResolved,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
        "score_objective" | "score_secret_objective" => "scoring",
        "cast_vote" | "vote_exhaust_planet" | "vote_tiebreak" => "voting",
        "propose_transaction" | "answer_transaction" => "transactions",
        "activate_system" => "activation",
        "movement_step" | "load_cargo" => "movement",
        "assign_casualty" | "sustain_damage" | "announce_retreat" | "retreat_to" => "combat",
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

#[must_use]
pub fn decision_grouping(
    record: &ti4_engine::choice::DecisionRecord,
    offered: Option<&ChoiceOption>,
    records: &[ti4_engine::choice::DecisionRecord],
    cursor: usize,
) -> (
    Option<PlayerId>,
    Option<u32>,
    Option<Phase>,
    Option<String>,
    Option<PlayerId>,
    Option<String>,
) {
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
pub(crate) fn played_card_detail(
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
    if detail.is_none()
        && record.prompt == "action phase"
        && let Some(option) = offered.filter(|option| option.id == record.chosen)
    {
        detail = match option.id.as_str() {
            "tactical" => Some(format!("{} began a tactical action", record.player)),
            "pass" => Some(format!("{} passed their turn", record.player)),
            id if id.starts_with("strategic|") => id
                .strip_prefix("strategic|")
                .and_then(strategy_card_name)
                .map(|name| format!("{} played {name}", record.player)),
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
pub struct MovementFact {
    pub actor: PlayerId,
    pub origin: String,
    pub destination: String,
    pub unit: String,
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
            decision_grouping(&selection, Some(&action), &[selection.clone()], 1)
                .5
                .as_deref(),
            Some("action selection")
        );
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
            action_id_for(&[action.clone()], 1).as_deref(),
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
            &[player.clone()],
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
pub struct GameEventMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub entry: GameEvent,
}

/// Versioned state update or replacement snapshot after state transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateUpdateMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub viewer: ViewerRole,
    pub view: GameView,
    pub state: GameState,
    pub galaxy_layout: GalaxyLayout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_choice: Option<PendingChoiceEnvelope>,
    pub turn_status: PublicTurnStatus,
    #[serde(default, skip_serializing_if = "is_default_history")]
    pub history: HistoryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_path: Option<CurrentLogPath>,
}

impl StateUpdateMsg {
    #[must_use]
    pub fn with_history(mut self, cursor: usize, redo_count: usize, generation: u64) -> Self {
        self.history = HistoryStatus {
            cursor,
            redo_count,
            generation,
        };
        self
    }
}

/// Pending choice sent only to the acting seat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingChoiceMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub nonce: String,
    pub choice: Choice,
    pub state: GameState,
    pub galaxy_layout: GalaxyLayout,
}

/// Public turn status update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnStatusMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub status: PublicTurnStatus,
}

/// Confirmation that an action was validated and accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionAcceptedMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub option_id: String,
}

/// Notification that an action submission was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRejectedMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub game_version: u64,
    pub reason: RejectionReason,
}

/// Protocol error notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolErrorMsg {
    pub protocol_version: u16,
    pub kind: ErrorKind,
    pub message: String,
}

/// Notification of game completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
pub struct PongMsg {
    pub protocol_version: u16,
    pub sequence: u64,
}

/// Messages emitted from server to client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
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
            Self::Error(_) | Self::Pong(_) => None,
        }
    }
}
