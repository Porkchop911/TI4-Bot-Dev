//! What a disposable attempt may show its planner.
//!
//! An offer is admitted by its producer and the meaning of its options, and a position by what
//! the planner already knew. [`super::draft`] asks here before it publishes anything.

use serde::{Deserialize, Serialize};
use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_engine::decision_context::{CONTEXT_VERSION, DecisionSource};
use ti4_engine::preview::{Outcome, Quantity};
use ti4_model::id::{PlayerId, StrategyCardId};

use crate::protocol::view::GameView;

pub const ASSUMPTION: &str = "Other players take no optional reactions in this hypothetical turn.";
pub const SECONDARY_ASSUMPTION: &str =
    "The primary ability and earlier players' secondaries may not have resolved yet.";

/// What a disposable attempt previews: the next tactical action of the seat, or its secondary
/// of the strategic action that is resolved (`secondary` of an update). The scope picks the
/// engine preparation and the audited offers; the gate, script and lifecycle are shared.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum PlanScope {
    /// The planner's next tactical action.
    #[default]
    Tactical,
    /// The planner's secondary of `card`, played by `primary`.
    Secondary {
        primary: PlayerId,
        card: StrategyCardId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum StopReason {
    Uncertainty,
    UnsupportedOffer,
    OtherPlayerRequired,
    UnsupportedParticipation,
    UnsupportedSegment,
    KnowledgeChanged,
    ReplayMismatch,
    StepLimit,
    MovementComplete,
    SecondaryComplete,
}

/// Contains only the server's player projection, never an internal GameState.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafePublication {
    pub position: GameView,
    pub choice: Option<Choice>,
    // Driver events are validated internally, but never revealed by a draft.
    pub events: Vec<String>,
}

/// Exact producer identity and option semantics, never prompt text alone.
///
/// Game::turn_options makes the menu. Game::tactical_choice uses the normal
/// activation and movement helpers; CargoWindow::pending_choice supplies loads.
/// These options use public units, tokens and routes, plus the planner's known
/// movement boosts. Applying a move uses Dice at a rift, so its outcome is
/// marked before the next publication. Invasion and production admit only their
/// deterministic public choices; unknown outcomes stop the publication gate.
pub fn audit_offer(choice: &Choice) -> Result<Vec<String>, StopReason> {
    let context = choice
        .context
        .as_ref()
        .ok_or(StopReason::UnsupportedOffer)?;
    // The engine presents a one-option round trigger before rolling ground
    // combat. Stop at that boundary without recording an automatic fight.
    if context.version == CONTEXT_VERSION
        && context.actor == choice.player
        && context.phase == ti4_model::state::Phase::Action
        && context.source == DecisionSource::Rule("42".into())
        && context.subtype == "fight_ground_combat_round"
    {
        return Err(StopReason::Uncertainty);
    }
    // The planner's own abilities, cards and reaction windows: whatever asks
    // outside the core rules. It is the question the live game would send this
    // seat, built by the same code from the same position, so it is shown as
    // offered. Combat and invasion windows stay outside the slice.
    if context.version == CONTEXT_VERSION
        && context.actor == choice.player
        && context.phase == ti4_model::state::Phase::Action
        && !matches!(context.source, DecisionSource::Rule(_))
        && !context.space_battle
        && context.invasion_seq.is_none()
    {
        return Ok(choice
            .options
            .iter()
            .map(|option| option.id.clone())
            .collect());
    }
    if context.version != CONTEXT_VERSION
        || context.actor != choice.player
        || context.phase != ti4_model::state::Phase::Action
        || context.space_battle
        || (context.invasion_seq.is_some() && context.subtype != "commit_ground_forces")
        || context.optional
    {
        return Err(StopReason::UnsupportedOffer);
    }
    let DecisionSource::Rule(rule) = &context.source else {
        return Err(StopReason::UnsupportedOffer);
    };
    let rule = rule.as_str();
    // Constraints are admitted only for the producer that owns their semantics.
    use ti4_engine::decision_context::ConstraintKind;
    if context.outstanding.iter().any(|constraint| {
        !matches!(
            (context.subtype.as_str(), &constraint.kind),
            ("produce_unit", ConstraintKind::ProductionCapacity)
                | ("pay_resources", ConstraintKind::Resources)
                | (
                    "place_unit",
                    ConstraintKind::FleetSupply | ConstraintKind::TransportCapacity
                )
        )
    }) {
        return Err(StopReason::UnsupportedOffer);
    }
    let mut enabled = Vec::new();
    for option in &choice.options {
        let permitted = match (rule, context.subtype.as_str()) {
            ("22", "action_menu") => {
                // Publish a tactical-only wrapper. Other menu options may carry
                // unaudited previews, so `publication` removes them below.
                option.kind == "action" && option.id == "tactical"
            }
            ("89.1", "activate_system") => option.kind == ti4_engine::tactical::ACTIVATE_KIND,
            ("89.2", "movement_step") => {
                option.kind == ti4_engine::tactical::MOVE_KIND
                    || (option.kind == "decline" && option.id == "done_moving")
            }
            ("95", "load_cargo") => {
                option.kind == ti4_engine::transit::LOAD_KIND
                    || (option.kind == "decline" && option.id == "done_loading")
            }
            ("49", "commit_ground_forces") => {
                option.kind == ti4_engine::invasion::COMMIT_KIND
                    || (option.kind == "decline" && option.id == "done_committing")
            }
            ("68", "produce_unit") => {
                option.kind == ti4_engine::production::PRODUCE_KIND
                    || (option.kind == "decline" && option.id == "done_producing")
            }
            ("68", "place_unit") => option.kind == ti4_engine::production::PLACE_KIND,
            ("34.3/75.2/75.3", "pay_resources") => option.kind == ti4_engine::production::PAY_KIND,
            ("tactical action", "mid_action_pause") => {
                option.id == ti4_engine::game::CONTINUE_ACTION_ID
                    && option.kind == ti4_engine::game::CONTINUE_ACTION_KIND
            }
            _ => return Err(StopReason::UnsupportedOffer),
        };
        if permitted {
            // Payload keys are part of the audit too. A familiar option kind
            // must not smuggle a card identity in a newly added field.
            let keys: &[&str] = match context.subtype.as_str() {
                "action_menu" | "activate_system" => &[],
                "movement_step" if option.id == "done_moving" => &[],
                "movement_step" => &[
                    "origin",
                    "unit",
                    "damaged",
                    "capacity",
                    "gravity_drive",
                    "ionian",
                ],
                "load_cargo" => &[
                    "unit",
                    "source",
                    "damaged",
                    "galvanized",
                    "capacity_remaining",
                    "loaded_ground",
                    "loaded_fighters",
                    "ground_available",
                    "system",
                    "pickup_system",
                ],
                "commit_ground_forces" => &["planet", "unit", "damaged"],
                "produce_unit" => &[
                    "cost",
                    "printed_cost",
                    "discount",
                    "count",
                    "placed",
                    "yield",
                    "credit",
                    "available_resources",
                    "free_this_use",
                    "credit_used",
                    "owed",
                    "production_spent",
                    "unit",
                    "system",
                    "destination",
                    "placement_pending",
                    "capacity_used",
                    "fleet_headroom_after",
                    "capacity_free_after",
                    "fleet_excess_after",
                    "capacity_excess_after",
                ],
                "place_unit" => &[
                    "system",
                    "unit",
                    "destination",
                    "count",
                    "placed",
                    "capacity_used",
                    "fleet_headroom_after",
                    "capacity_free_after",
                    "fleet_excess_after",
                    "capacity_excess_after",
                ],
                "pay_resources" => &["worth", "owed", "kind", "source"],
                "mid_action_pause" => &[],
                _ => return Err(StopReason::UnsupportedOffer),
            };
            if option
                .payload
                .keys()
                .any(|key| !keys.contains(&key.as_str()))
            {
                return Err(StopReason::UnsupportedOffer);
            }
            audit_preview(&context.subtype, option)?;
            enabled.push(option.id.clone());
        } else if context.subtype != "action_menu"
            && option.kind != ti4_engine::diplomacy::candidates::OPEN_KIND
        {
            return Err(StopReason::UnsupportedOffer);
        }
    }
    if enabled.is_empty() {
        return Err(StopReason::UnsupportedOffer);
    }
    Ok(enabled)
}

/// The secondary slice: one follower resolving one secondary to its end.
///
/// The window question must be the one for this card and this primary player.
/// Every other question asked of the follower inside the preview window is the
/// question the live game would send that seat, built by the same code from the
/// same position, so it is shown as offered. What an answer leads to is judged
/// elsewhere: dice and draws latch uncertainty, and validate_knowledge stops on
/// anything newly learned (Politics and Imperial end at their draw).
pub fn audit_secondary_offer(
    choice: &Choice,
    primary: &PlayerId,
    card: &StrategyCardId,
) -> Result<Vec<String>, StopReason> {
    let detail = |key: &str| choice.details.get(key).and_then(serde_json::Value::as_str);
    if detail("kind") == Some("strategy_secondary")
        && (detail("card") != Some(card.as_str()) || detail("played_by") != Some(primary.as_str()))
    {
        return Err(StopReason::UnsupportedOffer);
    }
    Ok(choice
        .options
        .iter()
        .map(|option| option.id.clone())
        .collect())
}

pub fn audit_preview(subtype: &str, option: &ChoiceOption) -> Result<(), StopReason> {
    // A movement preview currently contains only public fleet/capacity numbers
    // or the fixed rift warning. Do not admit a new chance label or quantity
    // just because it was attached to an otherwise familiar move option.
    let permitted = match option.preview.as_ref() {
        None => {
            matches!(subtype, "action_menu" | "mid_action_pause")
                || matches!(
                    option.id.as_str(),
                    "done_moving" | "done_loading" | "done_committing" | "done_producing"
                )
        }
        Some(preview) if !preview.truncated => match &preview.outcome {
            Outcome::Certain { deltas } => {
                !deltas.is_empty()
                    && deltas.iter().all(|delta| match subtype {
                        "activate_system" => delta.quantity == Quantity::TacticTokens,
                        "movement_step" => matches!(
                            delta.quantity,
                            Quantity::FleetSupplyHeadroom | Quantity::CapacityFree
                        ),
                        "load_cargo" => delta.quantity == Quantity::CapacityFree,
                        "commit_ground_forces" => delta.quantity == Quantity::GroundForcesOnPlanet,
                        "produce_unit" => matches!(
                            delta.quantity,
                            Quantity::ProductionRemaining
                                | Quantity::ProductionFreeCapacity
                                | Quantity::FleetSupplyHeadroom
                                | Quantity::CapacityFree
                        ),
                        "place_unit" => matches!(
                            delta.quantity,
                            Quantity::FleetSupplyHeadroom | Quantity::CapacityFree
                        ),
                        "pay_resources" => {
                            matches!(delta.quantity, Quantity::Resources | Quantity::TradeGoods)
                        }
                        _ => false,
                    })
            }
            Outcome::Unknown { reason } => {
                subtype == "movement_step" && *reason == "gravity-rift survival is unresolved"
            }
            _ => false,
        },
        _ => false,
    };
    if permitted {
        Ok(())
    } else {
        Err(StopReason::UnsupportedOffer)
    }
}

pub fn validate_knowledge(
    baseline: &GameView,
    publication: &SafePublication,
    actor: &PlayerId,
    scope: &PlanScope,
) -> Result<(), StopReason> {
    // Researching is the planner's own public, deterministic choice from an
    // audited offer. Nobody else's technologies may change in a preview.
    let may_research = matches!(scope, PlanScope::Secondary { .. });
    let view = &publication.position;
    // Public movement and token costs may change. Newly learned identities may
    // not. Counts use multisets so duplicating a known card is also rejected.
    if learned_identity(baseline, view) {
        return Err(StopReason::KnowledgeChanged);
    }
    for player in &view.players {
        let known = baseline
            .players
            .iter()
            .find(|known| known.id == player.id)
            .ok_or(StopReason::KnowledgeChanged)?;
        if (!player.technologies.is_subset(&known.technologies)
            && !(may_research && &player.id == actor))
            || player.action_cards_count > known.action_cards_count
            || player.secret_objectives_count > known.secret_objectives_count
            || (&player.id != actor
                && (player.action_cards_count != known.action_cards_count
                    || player.secret_objectives_count != known.secret_objectives_count))
        {
            return Err(StopReason::KnowledgeChanged);
        }
    }
    if view.table.scored_objectives != baseline.table.scored_objectives
        || view.board.combat.is_some()
    {
        return Err(StopReason::KnowledgeChanged);
    }
    Ok(())
}

/// Whether `view` shows its seat a card, a relic, an attachment or a public objective that
/// `baseline`, an earlier view of the same seat, does not: something that was hidden and is
/// known now. A draft may not show it ([`validate_knowledge`]), and a seat may not undo a
/// decision that showed it.
#[must_use]
pub fn learned_identity(baseline: &GameView, view: &GameView) -> bool {
    for player in &view.players {
        let Some(known) = baseline.players.iter().find(|known| known.id == player.id) else {
            return true;
        };
        if !contained(&player.held_action_cards, &known.held_action_cards)
            || !contained(
                &player.held_secret_objectives,
                &known.held_secret_objectives,
            )
            || !contained(
                &player.scored_secret_objectives,
                &known.scored_secret_objectives,
            )
            || !contained(&player.relics, &known.relics)
        {
            return true;
        }
    }
    for (system, current) in &view.board.systems {
        for (planet, current) in &current.planets {
            let known = baseline
                .board
                .systems
                .get(system)
                .and_then(|system| system.planets.get(planet));
            if !contained(
                &current.attachments,
                known.map_or(&[], |planet| planet.attachments.as_slice()),
            ) {
                return true;
            }
        }
    }
    view.table.revealed_objectives != baseline.table.revealed_objectives
}

pub fn contained<T: PartialEq>(values: &[T], known: &[T]) -> bool {
    let mut available: Vec<&T> = known.iter().collect();
    values.iter().all(|value| {
        available
            .iter()
            .position(|known| *known == value)
            .is_some_and(|index| {
                available.remove(index);
                true
            })
    })
}
