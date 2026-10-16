//! Private, bounded engine replay for a staged workflow confirmation.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::clock::{Clock, Deadline};
use crate::rng_force::{RngForce, RngMarks};

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, DecisionRecord, IllegalChoice, Table};
use ti4_engine::decision_context::{DecisionContext, DecisionTarget};
use ti4_engine::game::Game;
use ti4_model::id::PlayerId;
use ti4_model::state::{GameState, Phase};

use crate::step_snapshot::StepSnapshot;

/// What a simulation or a replay starts from. The server's `SessionConfig` is one; the browser
/// host builds its own.
pub trait ReplayBasis {
    /// The opening state of the game.
    fn state(&self) -> &GameState;
    /// The map of the game, when it has one.
    fn galaxy(&self) -> Option<&ti4_content::galaxy::Galaxy>;
    /// The forced random positions of the timeline.
    fn rng_marks(&self) -> &RngMarks;
    /// The history generation that a [`StepSnapshot`] must have been taken in.
    fn history_generation(&self) -> u64;
    /// Where the replay deadline reads the time.
    fn clock(&self) -> &dyn Clock;
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest {
    pub request_id: String,
    pub expected_version: u64,
    pub nonce: String,
    pub plan: MovementPlan,
}

#[derive(Debug, Clone)]
pub struct MovementPlan {
    pub kind: BatchKind,
    pub destination: String,
    pub steps: Vec<MovementStep>,
}

// Reject cross-workflow steps at the request boundary, before entering private replay.
// The original movement request without a kind remains a supported request shape.
#[derive(Deserialize)]
#[serde(untagged)]
enum WirePlan {
    Typed(TypedPlan),
    Legacy(LegacyMovement),
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TypedPlan {
    TacticalMovement {
        destination: String,
        steps: Vec<MovementOnlyStep>,
    },
    Payment {
        steps: Vec<PaymentStep>,
    },
    AgendaVotePlanets {
        steps: Vec<VoteStep>,
    },
    Production {
        destination: String,
        steps: Vec<ProductionStep>,
    },
    Casualties {
        steps: Vec<CasualtyStep>,
    },
    Tokens {
        steps: Vec<TokenStep>,
    },
    ReadyPlanets {
        steps: Vec<ReadyStep>,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ReadyStep {
    /// Answer one "ready which planet" question with this planet.
    ReadyPlanet { planet: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyMovement {
    destination: String,
    steps: Vec<MovementOnlyStep>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum MovementOnlyStep {
    Move {
        origin: String,
        unit: String,
        damaged: bool,
    },
    Load {
        origin: String,
        unit: String,
        source: Option<String>,
        damaged: bool,
        #[serde(default)]
        galvanized: Option<bool>,
    },
    DoneLoading,
    DoneMoving,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PaymentStep {
    Exhaust { planet: String },
    TradeGood,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum VoteStep {
    VotePlanet { planet: String },
    DoneVoting,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ProductionStep {
    /// `exchange` picks the Amalgamation option (return a captured unit of that type, no cost)
    /// instead of the paid build; both are offered in the same decision with the same unit and
    /// count, so a step that does not say which it means must never be able to match the exchange.
    Produce {
        unit: String,
        count: u32,
        #[serde(default)]
        exchange: bool,
    },
    DoneProducing,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TokenStep {
    /// Place the next command token into this pool (an option id such as `tactic_tokens`).
    Pool { pool: String },
    /// Answer a "spend 3 influence for a command token" question.
    Purchase { buy: bool },
    /// Pay part of a purchase by exhausting a planet.
    Exhaust { planet: String },
    /// Pay part of a purchase with a trade good.
    TradeGood,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum CasualtyStep {
    Sustain { unit: String },
    Destroy { unit: String, damaged: bool },
}

impl<'de> Deserialize<'de> for MovementPlan {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let plan = WirePlan::deserialize(deserializer)?;
        Ok(match plan {
            WirePlan::Legacy(plan) => Self {
                kind: BatchKind::TacticalMovement,
                destination: plan.destination,
                steps: plan.steps.into_iter().map(Into::into).collect(),
            },
            WirePlan::Typed(TypedPlan::TacticalMovement { destination, steps }) => Self {
                kind: BatchKind::TacticalMovement,
                destination,
                steps: steps.into_iter().map(Into::into).collect(),
            },
            WirePlan::Typed(TypedPlan::Payment { steps }) => Self {
                kind: BatchKind::Payment,
                destination: String::new(),
                steps: steps.into_iter().map(Into::into).collect(),
            },
            WirePlan::Typed(TypedPlan::AgendaVotePlanets { steps }) => Self {
                kind: BatchKind::AgendaVotePlanets,
                destination: String::new(),
                steps: steps.into_iter().map(Into::into).collect(),
            },
            WirePlan::Typed(TypedPlan::Production { destination, steps }) => Self {
                kind: BatchKind::Production,
                destination,
                steps: steps.into_iter().map(Into::into).collect(),
            },
            WirePlan::Typed(TypedPlan::Casualties { steps }) => Self {
                kind: BatchKind::Casualties,
                destination: String::new(),
                steps: steps.into_iter().map(Into::into).collect(),
            },
            WirePlan::Typed(TypedPlan::Tokens { steps }) => Self {
                kind: BatchKind::Tokens,
                destination: String::new(),
                steps: steps.into_iter().map(Into::into).collect(),
            },
            WirePlan::Typed(TypedPlan::ReadyPlanets { steps }) => Self {
                kind: BatchKind::ReadyPlanets,
                destination: String::new(),
                steps: steps.into_iter().map(Into::into).collect(),
            },
        })
    }
}

impl From<MovementOnlyStep> for MovementStep {
    fn from(step: MovementOnlyStep) -> Self {
        match step {
            MovementOnlyStep::Move {
                origin,
                unit,
                damaged,
            } => Self::Move {
                origin,
                unit,
                damaged,
            },
            MovementOnlyStep::Load {
                origin,
                unit,
                source,
                damaged,
                galvanized,
            } => Self::Load {
                origin,
                unit,
                source,
                damaged,
                galvanized,
            },
            MovementOnlyStep::DoneLoading => Self::DoneLoading,
            MovementOnlyStep::DoneMoving => Self::DoneMoving,
        }
    }
}
impl From<PaymentStep> for MovementStep {
    fn from(step: PaymentStep) -> Self {
        match step {
            PaymentStep::Exhaust { planet } => Self::Exhaust { planet },
            PaymentStep::TradeGood => Self::TradeGood,
        }
    }
}
impl From<VoteStep> for MovementStep {
    fn from(step: VoteStep) -> Self {
        match step {
            VoteStep::VotePlanet { planet } => Self::VotePlanet { planet },
            VoteStep::DoneVoting => Self::DoneVoting,
        }
    }
}
impl From<ProductionStep> for MovementStep {
    fn from(step: ProductionStep) -> Self {
        match step {
            ProductionStep::Produce {
                unit,
                count,
                exchange,
            } => Self::Produce {
                unit,
                count,
                exchange,
            },
            ProductionStep::DoneProducing => Self::DoneProducing,
        }
    }
}

impl From<ReadyStep> for MovementStep {
    fn from(step: ReadyStep) -> Self {
        match step {
            ReadyStep::ReadyPlanet { planet } => Self::ReadyPlanet { planet },
        }
    }
}

impl From<TokenStep> for MovementStep {
    fn from(step: TokenStep) -> Self {
        match step {
            TokenStep::Pool { pool } => Self::Pool { pool },
            TokenStep::Purchase { buy } => Self::Purchase { buy },
            TokenStep::Exhaust { planet } => Self::Exhaust { planet },
            TokenStep::TradeGood => Self::TradeGood,
        }
    }
}

impl From<CasualtyStep> for MovementStep {
    fn from(step: CasualtyStep) -> Self {
        match step {
            CasualtyStep::Sustain { unit } => Self::Sustain { unit },
            CasualtyStep::Destroy { unit, damaged } => Self::Destroy { unit, damaged },
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchKind {
    #[default]
    TacticalMovement,
    Payment,
    AgendaVotePlanets,
    Production,
    /// Hits assigned in space or ground combat: sustains and casualties of one seat.
    Casualties,
    /// Several command tokens gained at once, each placed into a pool of the seat's choice.
    Tokens,
    /// Diplomacy's "ready up to 2 exhausted planets", answered together.
    ReadyPlanets,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MovementStep {
    Move {
        origin: String,
        unit: String,
        damaged: bool,
    },
    Load {
        origin: String,
        unit: String,
        source: Option<String>,
        damaged: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        galvanized: Option<bool>,
    },
    DoneLoading,
    DoneMoving,
    Exhaust {
        planet: String,
    },
    TradeGood,
    VotePlanet {
        planet: String,
    },
    DoneVoting,
    Produce {
        unit: String,
        count: u32,
        exchange: bool,
    },
    DoneProducing,
    Sustain {
        unit: String,
    },
    Destroy {
        unit: String,
        damaged: bool,
    },
    Pool {
        pool: String,
    },
    /// Leadership's "spend 3 influence for a command token?" answer: `true` is yes.
    Purchase {
        buy: bool,
    },
    /// One "ready which planet" answer (Diplomacy).
    ReadyPlanet {
        planet: String,
    },
}

/// The decisions a casualty plan answers.
const CASUALTY_SUBTYPES: [&str; 3] = [
    "sustain_damage",
    "assign_casualty",
    "assign_ground_casualty",
];

/// Hits still to assign in a casualty decision, when the engine states them.
fn hits_owed(choice: &Choice) -> Option<i64> {
    choice
        .context
        .as_ref()?
        .outstanding
        .iter()
        .find_map(|owed| {
            (owed.kind == ti4_engine::decision_context::ConstraintKind::UnitsToRemove)
                .then_some(owed.amount - owed.paid)
        })
}

impl MovementStep {
    fn subtype(&self) -> &'static str {
        match self {
            Self::Move { .. } | Self::DoneMoving => "movement_step",
            Self::Load { .. } | Self::DoneLoading => "load_cargo",
            Self::Exhaust { .. } | Self::TradeGood => "pay_resources",
            Self::VotePlanet { .. } | Self::DoneVoting => "vote_exhaust_planet",
            Self::Produce { .. } | Self::DoneProducing => "produce_unit",
            Self::Sustain { .. } => "sustain_damage",
            Self::Destroy { .. } => "assign_casualty",
            Self::Pool { .. } => "gain_command_token",
            Self::Purchase { .. } => "buy_token_with_influence",
            Self::ReadyPlanet { .. } => "ready_planet",
        }
    }

    fn matches_option(&self, option: &ChoiceOption) -> bool {
        let value = |key: &str| option.payload.get(key).and_then(serde_json::Value::as_str);
        match self {
            Self::Move {
                origin,
                unit,
                damaged,
            } => {
                option.kind == "move"
                    && value("origin") == Some(origin.as_str())
                    && value("unit") == Some(unit.as_str())
                    && option
                        .payload
                        .get("damaged")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                        == *damaged
            }
            Self::Load {
                origin,
                unit,
                source,
                damaged,
                galvanized,
            } => {
                option.kind == "load"
                    && value("system") == Some(origin.as_str())
                    && value("unit") == Some(unit.as_str())
                    && value("source") == source.as_deref()
                    && galvanized.is_none_or(|expected| {
                        option
                            .payload
                            .get("galvanized")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                            == expected
                    })
                    && option
                        .payload
                        .get("damaged")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                        == *damaged
            }
            Self::DoneLoading => option.id == "done_loading",
            Self::DoneMoving => option.id == "done_moving",
            Self::Exhaust { planet } => {
                option.kind == "pay" && option.id == format!("exhaust|{planet}")
            }
            Self::TradeGood => option.kind == "pay" && option.id == "trade_good",
            Self::VotePlanet { planet } => option.kind == "vote_planet" && option.id == *planet,
            Self::DoneVoting => option.id == "decline",
            Self::DoneProducing => option.id == "done_producing",
            Self::Pool { pool } => option.kind == "pool" && option.id == *pool,
            Self::ReadyPlanet { planet } => option.kind == "ready" && option.id == *planet,
            Self::Purchase { buy } => {
                option.kind == ti4_engine::strategy::STRATEGY_KIND
                    && option.id == if *buy { "yes" } else { "no" }
            }
            Self::Sustain { unit } => {
                option.kind == ti4_engine::combat::SUSTAIN_KIND
                    && value("unit") == Some(unit.as_str())
            }
            Self::Destroy { unit, damaged } => {
                (option.kind == ti4_engine::combat::CASUALTY_KIND
                    || option.kind == ti4_engine::invasion::GROUND_CASUALTY_KIND)
                    && value("unit") == Some(unit.as_str())
                    && option
                        .payload
                        .get("damaged")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                        == *damaged
            }
            Self::Produce {
                unit,
                count,
                exchange,
            } => {
                option.kind == "produce"
                    && option
                        .payload
                        .get("exchange")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                        == *exchange
                    && value("unit") == Some(unit.as_str())
                    && option
                        .payload
                        .get("count")
                        .and_then(serde_json::Value::as_u64)
                        == Some(u64::from(*count))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchFailure {
    pub failed_step: usize,
    /// Stable machine-readable reason.
    pub reason: String,
    pub expected: String,
    /// Kinds of the options offered at the failed step (only for the batch's own seat).
    pub offered_summary: Vec<String>,
    /// Human-readable explanation of what went wrong and where.
    pub message: String,
    pub planned_steps: usize,
    /// The decision the engine offered instead of the planned step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offered: Option<Box<OfferedDecision>>,
}

/// What the engine asked at a failed step. Option ids and the prompt are only filled in when
/// the decision belongs to the batch's own seat, so another seat's private choice never leaks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferedDecision {
    pub subtype: Option<String>,
    pub own_seat: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub option_ids: Vec<String>,
}

impl BatchFailure {
    fn new(step: usize, reason: impl Into<String>, expected: impl Into<String>) -> Self {
        let reason = reason.into();
        let expected = expected.into();
        Self {
            failed_step: step,
            message: format!("step {step}: {reason} (expected {expected})"),
            reason,
            expected,
            offered_summary: Vec::new(),
            planned_steps: 0,
            offered: None,
        }
    }

    fn with_planned_steps(mut self, planned_steps: usize) -> Self {
        self.planned_steps = planned_steps;
        self
    }
}

/// A batch applied to the history: its request, its seat and the decisions it answered
/// (`start_cursor..end_cursor`). Saved with the history and in the timeline a turn redo keeps
/// (`history::turn_redo::AlternateTimeline`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRecord {
    pub request_id: String,
    pub batch_id: String,
    pub actor: PlayerId,
    pub start_cursor: usize,
    pub end_cursor: usize,
    /// Set when the batch stopped at a reaction window; answers a repeated request the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupted: Option<BatchInterruption>,
}

/// A batch that stopped early because the engine opened a reaction window the plan did not
/// plan for. The steps before it are applied and recorded; the window stays pending for its
/// seat, and `remaining_steps` is what the client may offer to re-send once it has resolved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchInterruption {
    /// Plan steps consumed before the interruption.
    pub applied_steps: usize,
    pub remaining_steps: Vec<MovementStep>,
    /// The decision that now waits (its option ids only when it belongs to the batch's seat).
    pub offered: OfferedDecision,
}

const MAX_REPORTED_OPTIONS: usize = 16;

/// Reaction windows are answered by the player holding the card, never by a plan.
fn is_reaction_window(choice: &Choice) -> bool {
    choice
        .context
        .as_ref()
        .is_some_and(|c| c.subtype.starts_with("reaction_"))
}

struct Script {
    prefix: VecDeque<DecisionRecord>,
    /// Forced random positions for the prefix (a timeline made by a turn redo), and its length.
    force: Option<RngForce>,
    prefix_len: usize,
    steps: Vec<MovementStep>,
    next: usize,
    actor: PlayerId,
    destination: String,
    kind: BatchKind,
    payment_subtype: Option<String>,
    workflow_context: Option<DecisionContext>,
    selected: Vec<ChoiceOption>,
    failure: Option<BatchFailure>,
    finished: bool,
    /// Set when the batch ended at a reaction window with planned steps left over.
    interruption: Option<BatchInterruption>,
    /// Casualty plans: the most hits a later decision of the same assignment may still owe.
    casualty_owed: Option<i64>,
    /// Every decision this script answered, in order, as the engine's log records it. The engine
    /// rolls a system activation back as a whole when a nested question is refused, and the
    /// boundary is such a refusal: the decisions that step had already answered (the activation
    /// itself, a reaction, an ability's production) vanish from the game's log, so the log alone
    /// cannot say what the plan recorded.
    answered: Vec<DecisionRecord>,
}

struct PrivateDecider(Arc<Mutex<Script>>);

pub struct Simulation {
    /// How many decisions of the prefix a forked step-boundary copy covered (`None`: the whole
    /// prefix was replayed from the opening state). Not part of the result's meaning.
    pub forked_at: Option<usize>,
    pub interruption: Option<BatchInterruption>,
    pub decisions: Vec<DecisionRecord>,
    pub selected: Vec<ChoiceOption>,
    pub transitions: Vec<(usize, Phase, u32)>,
    pub winner: Option<Option<PlayerId>>,
    pub boundary_state: GameState,
    /// Every die the simulated game had rolled when it stopped: its position in the roll history,
    /// the roll, and how many decisions the log held when the step that rolled it returned. The
    /// caller publishes the ones the live game has not shown yet.
    pub rolls: Vec<(usize, ti4_engine::dice::Roll, usize)>,
    /// A copy of the game at the last step boundary near the end of the prefix, when asked for
    /// (see [`replay_boundary`]).
    pub tail_copy: Option<StepSnapshot>,
}

impl Decider for PrivateDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let option = self.answer(choice)?;
        let mut log = ti4_engine::choice::DecisionLog::default();
        log.record(choice, &option);
        self.0
            .lock()
            .expect("batch script lock")
            .answered
            .extend(log.records);
        Ok(option)
    }
}

impl PrivateDecider {
    fn answer(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let mut script = self.0.lock().expect("batch script lock");
        let prefix_index = script.prefix_len.saturating_sub(script.prefix.len());
        if let Some(record) = script.prefix.pop_front() {
            if let Some(force) = &script.force {
                force.before_answer(prefix_index);
            }
            let matching: Vec<_> = choice
                .options
                .iter()
                .filter(|o| o.id == record.chosen)
                .collect();
            return if choice.player == record.player
                && choice.prompt == record.prompt
                && choice.ids()
                    == record
                        .offered
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                && choice
                    .context
                    .as_ref()
                    .map(ti4_engine::decision_context::DecisionContext::without_display_fields)
                    == record
                        .context
                        .as_ref()
                        .map(ti4_engine::decision_context::DecisionContext::without_display_fields)
                && matching.len() == 1
            {
                Ok(matching[0].clone())
            } else {
                Err(IllegalChoice::DeciderFailed {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                    reason: "batch prefix diverged".into(),
                })
            };
        }
        if let Some(force) = &script.force {
            force.go_live();
        }
        while script.kind == BatchKind::TacticalMovement
            && matches!(
                script.steps.get(script.next),
                Some(MovementStep::DoneLoading)
            )
            && choice
                .context
                .as_ref()
                .is_some_and(|c| c.subtype == "movement_step")
        {
            script.next += 1;
        }
        // The engine opens a cargo hold for every ship that can carry along the path, including
        // pickups the plan never mentioned. A plan that goes straight on to another move (or
        // finishes) means "carry nothing here", so decline the hold instead of interrupting.
        if script.kind == BatchKind::TacticalMovement
            && choice.player == script.actor
            && choice
                .context
                .as_ref()
                .is_some_and(|c| c.actor == script.actor && c.subtype == "load_cargo")
            && matches!(
                script.steps.get(script.next),
                Some(MovementStep::Move { .. } | MovementStep::DoneMoving)
            )
            && let Some(done) = choice
                .options
                .iter()
                .find(|o| MovementStep::DoneLoading.matches_option(o))
        {
            let done = done.clone();
            script.selected.push(done.clone());
            return Ok(done);
        }
        if script.kind == BatchKind::Casualties
            && let Some(answer) = casualty_shortcut(&mut script, choice)
        {
            return answer;
        }
        // A token plan names every payment of a purchase, but the engine takes a lone payment
        // option without asking. Payment steps that were settled that way are skipped once the
        // engine moves on to a decision that is not a payment.
        if script.kind == BatchKind::Tokens
            && !is_reaction_window(choice)
            && !(choice.player == script.actor
                && choice
                    .context
                    .as_ref()
                    .is_some_and(|c| c.subtype == "pay_influence"))
        {
            while matches!(
                script.steps.get(script.next),
                Some(MovementStep::Exhaust { .. } | MovementStep::TradeGood)
            ) {
                script.next += 1;
            }
        }
        if script.next == script.steps.len() {
            script.finished = true;
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "batch boundary reached".into(),
            });
        }
        // A reaction window belongs to the seat that holds the card: stop here, keep what the
        // plan already answered, and leave the window pending. The reaction decision is never
        // recorded by this batch, so replay reaches it as the first unrecorded choice.
        if !script.selected.is_empty()
            && matches!(
                script.kind,
                BatchKind::TacticalMovement
                    | BatchKind::AgendaVotePlanets
                    | BatchKind::Production
                    | BatchKind::Tokens
                    | BatchKind::ReadyPlanets
            )
            && is_reaction_window(choice)
        {
            let own_seat = choice.player == script.actor;
            let applied = script.next;
            let remaining = script.steps[applied..].to_vec();
            script.interruption = Some(BatchInterruption {
                applied_steps: applied,
                remaining_steps: remaining,
                offered: offered_decision(choice, own_seat),
            });
            script.finished = true;
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "batch boundary reached".into(),
            });
        }
        let index = script.next;
        let step = &script.steps[index];
        let expected = format!("{step:?}");
        let subtype = step.subtype();
        let context_ok = choice.context.as_ref().is_some_and(|c| {
            c.actor == script.actor && (c.subtype == subtype ||
                (script.kind == BatchKind::Casualties && subtype == "assign_casualty" && c.subtype == "assign_ground_casualty") ||
                (matches!(script.kind, BatchKind::Payment | BatchKind::Tokens) && c.subtype == "pay_influence" && subtype == "pay_resources")) &&
                script.payment_subtype.as_ref().is_none_or(|first| *first == c.subtype) &&
                (matches!(script.kind, BatchKind::TacticalMovement | BatchKind::Casualties | BatchKind::Tokens) || script.workflow_context.as_ref().is_none_or(|first| {
                    first.source == c.source && first.phase == c.phase && first.round == c.round
                        && first.target == c.target && first.subtype == c.subtype
                        && first.outstanding.iter().all(|owed| c.outstanding.iter().any(|next| {
                            next.kind == owed.kind && next.amount == owed.amount && next.paid >= owed.paid
                        }))
                })) &&
                (if let MovementStep::Load { origin, .. } = step {
                    matches!(&c.target, Some(DecisionTarget::System(id)) if id.as_str() == origin)
                } else if script.kind == BatchKind::Production {
                    matches!(&c.target, Some(DecisionTarget::System(id)) if id.as_str() == script.destination)
                } else if subtype == "load_cargo" || script.kind != BatchKind::TacticalMovement { true } else {
                    c.target.as_ref().is_none_or(|target| matches!(target, DecisionTarget::System(id) if id.as_str() == script.destination))
                })
        });
        let matches: Vec<_> = if choice.player == script.actor && context_ok {
            choice
                .options
                .iter()
                .filter(|o| step.matches_option(o))
                .collect()
        } else {
            Vec::new()
        };
        let own_seat = choice.player == script.actor;
        let offered_subtype = choice.context.as_ref().map(|c| c.subtype.clone());
        // The engine takes a lone payment option without asking, so a payment can settle before
        // every planned step is consumed. Once the actor is no longer being asked to pay, the
        // remaining payment steps were already covered and the batch ends at this boundary.
        let payment_settled = script.kind == BatchKind::Payment
            && index > 0
            && !(own_seat
                && offered_subtype
                    .as_deref()
                    .is_some_and(|s| s == "pay_resources" || s == "pay_influence"))
            && script.steps[index..]
                .iter()
                .all(|s| matches!(s, MovementStep::Exhaust { .. } | MovementStep::TradeGood));
        if matches.len() != 1 && payment_settled {
            script.steps.truncate(index);
            script.finished = true;
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "batch boundary reached".into(),
            });
        }
        if matches.len() != 1 {
            let reason = if !context_ok || !own_seat {
                "workflow interrupted"
            } else if matches.is_empty() {
                "option unavailable"
            } else {
                "ambiguous option"
            };
            script.failure = Some(step_rejection(
                choice,
                own_seat,
                reason,
                index,
                expected,
                matches.len(),
                script.steps.len(),
            ));
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "batch step rejected".into(),
            });
        }
        let selected = (*matches[0]).clone();
        if script.kind == BatchKind::Payment && script.payment_subtype.is_none() {
            script.payment_subtype = choice.context.as_ref().map(|c| c.subtype.clone());
        }
        if script.workflow_context.is_none() {
            script.workflow_context = choice.context.clone();
        }
        if script.kind == BatchKind::Casualties {
            script.casualty_owed = hits_owed(choice).map(|owed| owed - 1);
        }
        script.selected.push(selected.clone());
        script.next += 1;
        Ok(selected)
    }
}

/// A batch that is answered from the live game, one question at a time, by the thread that runs
/// the engine: what [`simulate`] does on a private copy, done on the game itself. The browser
/// host has no network between the request and its answers, so it applies the plan as the
/// engine asks, and goes back to the question the plan began at when a step fails.
pub struct LiveBatch {
    script: Arc<Mutex<Script>>,
}

/// What a live batch says to a question of the engine.
pub enum LiveStep {
    /// Answer the question with this option.
    Answer(ChoiceOption),
    /// Every step is answered, or the engine asks something the plan does not cover and the
    /// plan has nothing left to do: the question is not the batch's.
    Done,
    /// The engine asked a reaction window between two steps: what was applied stays and the
    /// rest is for later.
    Interrupted(BatchInterruption),
    /// The engine does not offer what the next step asks for.
    Failed(BatchFailure),
}

impl LiveBatch {
    /// # Errors
    /// A failure when the plan is not one the server would accept (`plan_problem`).
    pub fn new(plan: &MovementPlan, actor: &PlayerId) -> Result<Self, BatchFailure> {
        if let Some(problem) = plan_problem(plan) {
            let mut failure =
                BatchFailure::new(0, "invalid batch plan", "valid workflow steps required")
                    .with_planned_steps(plan.steps.len());
            failure.message = format!("invalid batch plan: {problem}");
            return Err(failure);
        }
        Ok(Self {
            script: Arc::new(Mutex::new(Script {
                prefix: VecDeque::new(),
                force: None,
                prefix_len: 0,
                steps: plan.steps.clone(),
                next: 0,
                actor: actor.clone(),
                destination: plan.destination.clone(),
                kind: plan.kind,
                payment_subtype: None,
                workflow_context: None,
                selected: Vec::new(),
                failure: None,
                finished: false,
                interruption: None,
                casualty_owed: None,
                answered: Vec::new(),
            })),
        })
    }

    /// The answer of the plan to `choice`.
    #[must_use]
    pub fn step(&self, choice: &Choice) -> LiveStep {
        let answer = PrivateDecider(self.script.clone()).answer(choice);
        match answer {
            Ok(option) => LiveStep::Answer(option),
            Err(_) => {
                let mut script = self.script.lock().expect("batch script lock");
                if let Some(failure) = script.failure.take() {
                    LiveStep::Failed(failure)
                } else if let Some(interruption) = script.interruption.take() {
                    LiveStep::Interrupted(interruption)
                } else {
                    LiveStep::Done
                }
            }
        }
    }

    /// How many steps of the plan are answered.
    #[must_use]
    pub fn applied(&self) -> usize {
        self.script.lock().expect("batch script lock").next
    }

    /// How many steps the plan has.
    #[must_use]
    pub fn planned(&self) -> usize {
        self.script.lock().expect("batch script lock").steps.len()
    }
}

/// Casualty plans: end the batch where the assignment ends, and answer a sustain question with
/// "take the hit" when the plan's next step destroys a ship. `None` lets the planned step run.
fn casualty_shortcut(
    script: &mut Script,
    choice: &Choice,
) -> Option<Result<ChoiceOption, IllegalChoice>> {
    let subtype = choice.context.as_ref().map(|c| c.subtype.as_str());
    let owed = hits_owed(choice);
    // Same seat, a sustain or casualty question, about the same place, and owing fewer
    // hits than before: anything else means this assignment is over (the engine took the
    // remaining hits itself, another seat reacts, or a new round's hits arrived).
    let same_assignment = choice.player == script.actor
        && subtype.is_some_and(|s| CASUALTY_SUBTYPES.contains(&s))
        && script.workflow_context.as_ref().is_none_or(|first| {
            choice
                .context
                .as_ref()
                .is_some_and(|c| c.target == first.target)
        })
        && match (script.casualty_owed, owed) {
            (Some(most), Some(now)) => now <= most,
            _ => true,
        };
    if !same_assignment && !script.selected.is_empty() {
        let next = script.next;
        script.steps.truncate(next);
        script.finished = true;
        return Some(Err(IllegalChoice::DeciderFailed {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            reason: "batch boundary reached".into(),
        }));
    }
    // A sustain question before a planned destroy is answered "take the hit": the plan
    // chose to lose a ship for this hit.
    if same_assignment
        && subtype == Some("sustain_damage")
        && matches!(
            script.steps.get(script.next),
            Some(MovementStep::Destroy { .. })
        )
        && let Some(take) = choice.options.iter().find(|o| o.is_decline())
    {
        let take = take.clone();
        if script.workflow_context.is_none() {
            script.workflow_context.clone_from(&choice.context);
        }
        script.selected.push(take.clone());
        return Some(Ok(take));
    }
    None
}

/// Describes why the engine's offer could not take a planned step, without exposing another
/// seat's private options.
fn step_rejection(
    choice: &Choice,
    own_seat: bool,
    reason: &str,
    index: usize,
    expected: String,
    matching: usize,
    planned_steps: usize,
) -> BatchFailure {
    let subtype = choice.context.as_ref().map(|c| c.subtype.clone());
    let asked = match (&subtype, own_seat) {
        (Some(subtype), true) => format!("your {subtype} decision"),
        (Some(subtype), false) => format!("another seat's {subtype} decision"),
        (None, true) => format!("your decision \"{}\"", choice.prompt),
        (None, false) => "another seat's decision".to_owned(),
    };
    let detail = match reason {
        "workflow interrupted" => format!(
            "the engine moved on to {asked} before planned step {index} ({expected}) could be applied"
        ),
        "option unavailable" => format!(
            "planned step {index} ({expected}) is not among the {} options offered for {asked}",
            choice.options.len()
        ),
        _ => format!(
            "planned step {index} ({expected}) matches {matching} options offered for {asked}"
        ),
    };
    let own_options = |field: fn(&ChoiceOption) -> String| -> Vec<String> {
        if own_seat {
            choice
                .options
                .iter()
                .take(MAX_REPORTED_OPTIONS)
                .map(field)
                .collect()
        } else {
            Vec::new()
        }
    };
    BatchFailure {
        failed_step: index,
        reason: reason.into(),
        message: format!("{reason}: {detail}"),
        expected,
        offered_summary: own_options(|o| o.kind.clone()),
        planned_steps,
        offered: Some(Box::new(offered_decision(choice, own_seat))),
    }
}

fn offered_decision(choice: &Choice, own_seat: bool) -> OfferedDecision {
    OfferedDecision {
        subtype: choice.context.as_ref().map(|c| c.subtype.clone()),
        own_seat,
        prompt: own_seat.then(|| choice.prompt.clone()),
        option_ids: if own_seat {
            choice
                .options
                .iter()
                .take(MAX_REPORTED_OPTIONS)
                .map(|o| o.id.clone())
                .collect()
        } else {
            Vec::new()
        },
    }
}

/// Diagnostic: how many batch checks (process-wide) started from a step-boundary copy instead
/// of replaying the whole game. Tests read the difference around one request.
pub static FORKED_CHECKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How a batch check obtains the position the staged choices start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationMode {
    /// Fork the worker's step-boundary copy when it is valid for the history, else replay in
    /// full. `TI4_BATCH_VERIFY=1` upgrades this to [`SimulationMode::Verify`].
    Auto,
    /// Always replay the whole recorded prefix from the opening state (the reference).
    FullReplay,
    /// Run the forked path (when a valid copy exists) AND the full replay, and panic when
    /// their results differ in any way. Used by tests and `TI4_BATCH_VERIFY=1`.
    Verify,
}

/// Replays the prefix and proves every staged choice against a fresh engine offer.
///
/// With a valid step-boundary `snapshot` only the decisions recorded after it are replayed
/// (see [`crate::step_snapshot`]); otherwise the whole prefix is.
pub fn simulate(
    config: &dyn ReplayBasis,
    prefix: &[DecisionRecord],
    actor: &PlayerId,
    plan: &MovementPlan,
    snapshot: Option<&StepSnapshot>,
) -> Result<Simulation, BatchFailure> {
    simulate_with(config, prefix, actor, plan, snapshot, SimulationMode::Auto)
}

/// [`simulate`] with an explicit [`SimulationMode`].
pub fn simulate_with(
    config: &dyn ReplayBasis,
    prefix: &[DecisionRecord],
    actor: &PlayerId,
    plan: &MovementPlan,
    snapshot: Option<&StepSnapshot>,
    mode: SimulationMode,
) -> Result<Simulation, BatchFailure> {
    if let Some(problem) = plan_problem(plan) {
        let mut failure =
            BatchFailure::new(0, "invalid batch plan", "valid workflow steps required")
                .with_planned_steps(plan.steps.len());
        failure.message = format!("invalid batch plan: {problem}");
        return Err(failure);
    }
    let mode = if mode == SimulationMode::Auto
        && std::env::var_os("TI4_BATCH_VERIFY").is_some_and(|value| value != "0")
    {
        SimulationMode::Verify
    } else {
        mode
    };
    let usable = snapshot.filter(|copy| copy.is_valid_for(config.history_generation(), prefix));
    let planned = plan.steps.len();
    let finish = |result: Result<Simulation, BatchFailure>| {
        result.map_err(|failure| {
            if failure.planned_steps == 0 {
                failure.with_planned_steps(planned)
            } else {
                failure
            }
        })
    };
    match (mode, usable) {
        (SimulationMode::Auto, Some(copy)) => {
            FORKED_CHECKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            finish(simulate_script(
                config,
                prefix,
                actor,
                plan,
                Some(copy),
                None,
            ))
        }
        (SimulationMode::Verify, Some(copy)) => {
            FORKED_CHECKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let forked = finish(simulate_script(
                config,
                prefix,
                actor,
                plan,
                Some(copy),
                None,
            ));
            let full = finish(simulate_script(config, prefix, actor, plan, None, None));
            if let Some(difference) = simulation_difference(&forked, &full) {
                panic!(
                    "batch check from a step-boundary copy (log {} of {}) disagrees with the full replay: {difference}",
                    copy.log_len,
                    prefix.len()
                );
            }
            forked
        }
        _ => finish(simulate_script(config, prefix, actor, plan, None, None)),
    }
}

fn json<T: Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).expect("serializable")
}

/// How two batch results differ, or `None` when they mean the same (the verifier's comparison:
/// selected options, recorded decisions, boundary state, transitions, winner, interruption, and
/// for failures every field the client sees).
#[must_use]
pub fn simulation_difference(
    a: &Result<Simulation, BatchFailure>,
    b: &Result<Simulation, BatchFailure>,
) -> Option<String> {
    match (a, b) {
        (Ok(a), Ok(b)) => {
            if a.decisions != b.decisions {
                return Some("recorded decisions differ".to_owned());
            }
            if a.selected != b.selected {
                return Some("selected options differ".to_owned());
            }
            if a.transitions != b.transitions {
                return Some(format!(
                    "transitions differ: {:?} vs {:?}",
                    a.transitions, b.transitions
                ));
            }
            if a.winner != b.winner {
                return Some("winner differs".to_owned());
            }
            if json(&a.interruption) != json(&b.interruption) {
                return Some("interruption differs".to_owned());
            }
            if !a.boundary_state.identical(&b.boundary_state)
                || json(&a.boundary_state) != json(&b.boundary_state)
            {
                return Some("boundary state differs".to_owned());
            }
            None
        }
        (Err(a), Err(b)) => {
            (json(a) != json(b)).then(|| format!("failures differ: {a:?} vs {b:?}"))
        }
        (Ok(_), Err(b)) => Some(format!("only the other path failed: {b:?}")),
        (Err(a), Ok(_)) => Some(format!("only one path failed: {a:?}")),
    }
}

/// Explains why a plan cannot be a batch of its kind, or `None` when its shape is valid.
fn plan_problem(plan: &MovementPlan) -> Option<String> {
    if plan.steps.is_empty() {
        return Some("the plan has no steps".to_owned());
    }
    if plan.steps.len() > 100 {
        return Some(format!(
            "the plan has {} steps; at most 100 are allowed",
            plan.steps.len()
        ));
    }
    let body = &plan.steps[..plan.steps.len() - 1];
    let last = plan.steps.last();
    let destination_needed = matches!(
        plan.kind,
        BatchKind::TacticalMovement | BatchKind::Production
    );
    if destination_needed && (plan.destination.is_empty() || plan.destination.len() > 64) {
        return Some("a destination system id of 1 to 64 characters is required".to_owned());
    }
    if !destination_needed && !plan.destination.is_empty() {
        return Some(format!("{:?} plans must not name a destination", plan.kind));
    }
    let problem = match plan.kind {
        BatchKind::TacticalMovement => (!matches!(last, Some(MovementStep::DoneMoving)))
            .then_some("a movement plan must end with done_moving")
            .or_else(|| {
                (!body.iter().all(|s| {
                    matches!(
                        s,
                        MovementStep::Move { .. } | MovementStep::Load { .. } | MovementStep::DoneLoading
                    )
                }))
                .then_some("a movement plan may only contain move, load and done_loading before done_moving")
            }),
        BatchKind::Payment => (!plan
            .steps
            .iter()
            .all(|s| matches!(s, MovementStep::Exhaust { .. } | MovementStep::TradeGood)))
        .then_some("a payment plan may only contain exhaust and trade_good steps"),
        BatchKind::AgendaVotePlanets => (!plan
            .steps
            .iter()
            .all(|s| matches!(s, MovementStep::VotePlanet { .. } | MovementStep::DoneVoting)))
        .then_some("a vote plan may only contain vote_planet and done_voting steps")
        .or_else(|| {
            body.iter()
                .any(|s| matches!(s, MovementStep::DoneVoting))
                .then_some("done_voting may only be the last step")
        }),
        BatchKind::Production => (!plan.steps.iter().all(|s| {
            matches!(
                s,
                MovementStep::Produce { count: 1..=100, .. } | MovementStep::DoneProducing
            )
        }))
        .then_some("a production plan may only contain produce steps with a count of 1 to 100 and done_producing")
        .or_else(|| {
            body.iter()
                .any(|s| matches!(s, MovementStep::DoneProducing))
                .then_some("done_producing may only be the last step")
        }),
        BatchKind::ReadyPlanets => (!plan
            .steps
            .iter()
            .all(|s| matches!(s, MovementStep::ReadyPlanet { .. })))
        .then_some("a ready plan may only contain ready_planet steps")
        .or_else(|| {
            (plan.steps.len() > 2).then_some("a ready plan readies at most 2 planets")
        })
        .or_else(|| {
            let mut seen = std::collections::BTreeSet::new();
            plan.steps
                .iter()
                .any(|s| match s {
                    MovementStep::ReadyPlanet { planet } => !seen.insert(planet.as_str()),
                    _ => false,
                })
                .then_some("a ready plan names each planet once")
        }),
        BatchKind::Casualties => (!plan
            .steps
            .iter()
            .all(|s| matches!(s, MovementStep::Sustain { .. } | MovementStep::Destroy { .. })))
        .then_some("a casualty plan may only contain sustain and destroy steps"),
        BatchKind::Tokens => (!plan.steps.iter().all(|s| {
            matches!(
                s,
                MovementStep::Pool { .. }
                    | MovementStep::Purchase { .. }
                    | MovementStep::Exhaust { .. }
                    | MovementStep::TradeGood
            )
        }))
        .then_some("a token plan may only contain pool, purchase, exhaust and trade_good steps")
        .or_else(|| {
            let pools = plan
                .steps
                .iter()
                .filter(|s| matches!(s, MovementStep::Pool { .. }))
                .count();
            (pools > usize::try_from(ti4_model::state::TOKENS_PER_FACTION).unwrap_or(16))
                .then_some("a token plan places at most 16 tokens")
        }),
    };
    problem.map(str::to_owned)
}

/// Reconstruct the view at the first unrecorded choice when a session is recovered
/// without the in-memory boundary state from a batch simulation.
pub fn replay_boundary_state(
    config: &dyn ReplayBasis,
    prefix: &[DecisionRecord],
) -> Result<GameState, BatchFailure> {
    let plan = MovementPlan {
        kind: BatchKind::TacticalMovement,
        destination: String::new(),
        steps: Vec::new(),
    };
    let actor = prefix
        .last()
        .map(|record| record.player.clone())
        .ok_or_else(|| BatchFailure::new(0, "empty replay", "recorded prefix"))?;
    simulate_script(config, prefix, &actor, &plan, None, None)
        .map(|simulation| simulation.boundary_state)
}

/// How close to the end of the prefix the replays copy the game at step boundaries
/// ([`replay_boundary`], `replay::replay_session_with_copy`). A copy costs about as much as a step,
/// so only the last few boundaries are copied; a final step that answers more recorded decisions
/// than this leaves an earlier copy (or none, and the worker replays the whole prefix as before).
pub(crate) const TAIL_COPY_WINDOW: usize = 64;

/// [`replay_boundary_state`], replaying only what `from` (a copy of the game at an earlier step
/// boundary of this same prefix) does not cover, and also returning a copy of the game at the last
/// step boundary of the replay when `generation` is given: a new session worker forks it
/// (`SessionConfig::resume` in the server) instead of replaying the whole prefix a second time.
///
/// # Errors
/// As [`replay_boundary_state`].
pub fn replay_boundary(
    config: &dyn ReplayBasis,
    prefix: &[DecisionRecord],
    from: Option<&StepSnapshot>,
    generation: Option<u64>,
) -> Result<(GameState, Option<StepSnapshot>), BatchFailure> {
    let plan = MovementPlan {
        kind: BatchKind::TacticalMovement,
        destination: String::new(),
        steps: Vec::new(),
    };
    let actor = prefix
        .last()
        .map(|record| record.player.clone())
        .ok_or_else(|| BatchFailure::new(0, "empty replay", "recorded prefix"))?;
    simulate_script(config, prefix, &actor, &plan, from, generation)
        .map(|simulation| (simulation.boundary_state, simulation.tail_copy))
}

/// Wall-clock budget for replaying `decisions` recorded choices from the opening state.
///
/// Every batch replays the whole game, so the cost grows with it: a debug server spends a few
/// milliseconds per recorded choice, more on a loaded machine. Runaway loops are still caught by
/// the step cap in [`simulate_script`].
pub fn replay_budget(decisions: usize) -> Duration {
    let per_decision = Duration::from_millis(20);
    Duration::from_secs(15) + per_decision * u32::try_from(decisions).unwrap_or(u32::MAX / 1000)
}

fn simulate_script(
    config: &dyn ReplayBasis,
    prefix: &[DecisionRecord],
    actor: &PlayerId,
    plan: &MovementPlan,
    from: Option<&StepSnapshot>,
    copy_generation: Option<u64>,
) -> Result<Simulation, BatchFailure> {
    let force = RngForce::new(config.rng_marks());
    // Decisions the copy already covers are not replayed; the rest of the prefix is.
    let covered = from.map_or(0, |copy| copy.log_len);
    let script = Arc::new(Mutex::new(Script {
        prefix: prefix[covered..].iter().cloned().collect(),
        force: force.clone(),
        prefix_len: prefix.len(),
        steps: plan.steps.clone(),
        next: 0,
        actor: actor.clone(),
        destination: plan.destination.clone(),
        kind: plan.kind,
        payment_subtype: None,
        workflow_context: None,
        selected: Vec::new(),
        failure: None,
        finished: false,
        interruption: None,
        casualty_owed: None,
        answered: Vec::new(),
    }));
    let mut game = if let Some(copy) = from {
        let mut game = copy.fork(prefix);
        game.table
            .set_default(Box::new(PrivateDecider(script.clone())));
        game
    } else {
        let table = Table::with_default(Box::new(PrivateDecider(script.clone())));
        let mut game = Game::with_table(config.state().clone(), ContentStore::embedded(), table);
        if let Some(galaxy) = config.galaxy() {
            game = game.with_galaxy(galaxy.clone());
        }
        game
    };
    if let Some(force) = &force {
        force.attach(&mut game);
    }
    let replayed = prefix.len() - covered;
    let clock = config.clock();
    let deadline = Deadline::after(
        clock,
        u64::try_from(replay_budget(replayed).as_millis()).unwrap_or(u64::MAX),
    );
    let mut phase = game.state.phase;
    let mut round = game.state.round;
    let mut transitions = Vec::new();
    let mut tail_copy: Option<StepSnapshot> = None;
    let mut rolls: Vec<(usize, ti4_engine::dice::Roll, usize)> = Vec::new();
    for _ in 0..(replayed
        .saturating_add(plan.steps.len())
        .saturating_mul(8)
        .saturating_add(128))
    {
        if deadline.passed(clock) {
            return Err(BatchFailure::new(
                script.lock().expect("script").next,
                "simulation timed out",
                "next engine choice",
            ));
        }
        if let Some(generation) = copy_generation {
            // Between steps: copy the game once the prefix is nearly answered (each boundary
            // that recorded something since the last copy).
            let remaining = script.lock().expect("script").prefix.len();
            if remaining <= TAIL_COPY_WINDOW
                && tail_copy
                    .as_ref()
                    .is_none_or(|copy| copy.log_len < game.table.log.records.len())
                && let Some(copy) = StepSnapshot::capture(&mut game, generation)
            {
                tail_copy = Some(copy);
            }
        }
        let result = game.step();
        let known = rolls.len();
        rolls.extend(
            game.rolls()
                .iter()
                .enumerate()
                .skip(known)
                .map(|(index, roll)| (index, roll.clone(), game.table.log.records.len())),
        );
        if game.state.phase != phase || game.state.round != round {
            phase = game.state.phase;
            round = game.state.round;
            if game.table.log.records.len() > prefix.len() {
                transitions.push((game.table.log.records.len(), phase, round));
            }
        }
        let guard = script.lock().expect("script");
        if let Some(failure) = &guard.failure {
            return Err(failure.clone());
        }
        if let Some(error) = &result.error
            && !(guard.finished && error.to_string().contains("batch boundary reached"))
        {
            return Err(BatchFailure::new(
                guard.next,
                format!("engine error: {error}"),
                "next planned choice",
            ));
        }
        if guard.finished || result.finished {
            if (guard.next != guard.steps.len() && guard.interruption.is_none())
                || !guard.prefix.is_empty()
            {
                return Err(BatchFailure::new(
                    guard.next,
                    "game ended before plan completed",
                    "next planned choice",
                ));
            }
            // Every decision after the prefix must be one this batch answered. That is not one
            // per planned step: a declined cargo hold adds an answer, a skipped DoneLoading
            // removes one. `selected` is what the caller pairs with each recorded decision.
            //
            // A boundary inside a step the engine rolls back as a whole (a system activation
            // with its reactions and an ability's production) takes that step's earlier answers
            // out of the game's log; the script's own list has them.
            let logged = &game.table.log.records;
            let rebuilt: Vec<DecisionRecord>;
            let records: &[DecisionRecord] = if logged.len() < covered + guard.answered.len() {
                rebuilt = logged
                    .iter()
                    .take(covered)
                    .chain(&guard.answered)
                    .cloned()
                    .collect();
                &rebuilt
            } else {
                logged
            };
            if records.get(..prefix.len()) != Some(prefix)
                || records.len() != prefix.len() + guard.selected.len()
            {
                return Err(BatchFailure::new(0, "replay diverged", "recorded prefix"));
            }
            let winner = (game.state.finished || result.finished)
                .then(|| ti4_engine::objectives::leader(&game.state))
                .flatten();
            return Ok(Simulation {
                forked_at: from.map(|copy| copy.log_len),
                interruption: guard.interruption.clone(),
                decisions: records[prefix.len()..].to_vec(),
                selected: guard.selected.clone(),
                transitions,
                winner: (game.state.finished || result.finished).then_some(winner),
                boundary_state: game.state.clone(),
                rolls,
                tail_copy,
            });
        }
        if let Some(err) = result.error {
            return Err(BatchFailure::new(
                guard.next,
                format!("engine error: {err}"),
                "next planned choice",
            ));
        }
    }
    Err(BatchFailure::new(
        0,
        "simulation step limit exceeded",
        "next engine choice",
    ))
}

#[cfg(test)]
mod tests {
    /// Run 04-0023 (2026-10-07): a round 7 batch replayed 1650 recorded choices in a debug
    /// server on a loaded machine and hit a flat 15 s budget ("simulation timed out"). Idle,
    /// the same replay took 4.5 to 10 s, so the budget must grow with the game.
    #[test]
    fn replay_budget_grows_with_the_recorded_prefix() {
        assert!(super::replay_budget(0) >= std::time::Duration::from_secs(15));
        assert!(super::replay_budget(1650) >= std::time::Duration::from_secs(45));
        assert!(super::replay_budget(3000) > super::replay_budget(1650));
    }

    use super::*;
    use ti4_engine::decision_context::{DecisionContext, DecisionSource};

    struct TestBasis {
        state: GameState,
        galaxy: ti4_content::galaxy::Galaxy,
        marks: RngMarks,
        clock: crate::clock::ManualClock,
    }

    impl ReplayBasis for TestBasis {
        fn state(&self) -> &GameState {
            &self.state
        }
        fn galaxy(&self) -> Option<&ti4_content::galaxy::Galaxy> {
            Some(&self.galaxy)
        }
        fn rng_marks(&self) -> &RngMarks {
            &self.marks
        }
        fn history_generation(&self) -> u64 {
            0
        }
        fn clock(&self) -> &dyn Clock {
            &self.clock
        }
    }

    /// The batch simulator's game end (`Simulation::winner`, which `submit_batch` turns into
    /// `GameFinished`) breaks a three-way VP tie by initiative, not by seat or player order.
    #[test]
    fn a_three_way_tie_ending_inside_a_batch_goes_to_the_first_in_initiative() {
        let players: Vec<PlayerId> = ["p1", "p2", "p3"].into_iter().map(PlayerId::new).collect();
        let (mut state, galaxy) =
            crate::map::create_game_with_template(ContentStore::embedded(), &players, 42, None)
                .unwrap();
        let mut by_initiative: Vec<_> = state
            .card_initiative
            .iter()
            .map(|(card, initiative)| (card.clone(), *initiative))
            .collect();
        by_initiative.sort_by_key(|(_, initiative)| *initiative);
        // Seating p1, p2, p3; initiative p2, p3, p1. The winner is neither the first nor the
        // last tied seat in player order.
        let cards = [
            by_initiative.last().unwrap().0.clone(),
            by_initiative[0].0.clone(),
            by_initiative[4].0.clone(),
        ];
        // The status phase's mandatory reveal (81.2) finds an empty deck and ends the game.
        state.phase = Phase::Status;
        state.round = 4;
        state.objective_deck.clear();
        state.seating_order.clone_from(&players);
        state
            .unclaimed_strategy_cards
            .retain(|card| !cards.contains(card));
        for (seat, card) in players.iter().zip(cards.iter()) {
            let player = state.player_mut(seat).unwrap();
            player.victory_points = 7;
            player.strategy_cards = vec![card.clone()];
        }
        // The status windows still ask their questions; every seat declines. Those answers are
        // the recorded prefix the simulator replays before it reaches the end.
        let mut game = Game::with_table(
            state.clone(),
            ContentStore::embedded(),
            Table::with_default(Box::new(ti4_engine::choice::AlwaysDecline)),
        )
        .with_galaxy(galaxy.clone());
        for _ in 0..64 {
            if game.state.finished {
                break;
            }
            assert_eq!(game.step().error, None);
        }
        assert!(game.state.finished, "the status phase should end the game");
        let prefix = game.table.log.records.clone();
        let config = TestBasis {
            state,
            galaxy,
            marks: RngMarks::new(),
            clock: crate::clock::ManualClock::new(0, 0),
        };
        let plan = MovementPlan {
            kind: BatchKind::TacticalMovement,
            destination: String::new(),
            steps: Vec::new(),
        };
        let simulation = simulate_script(&config, &prefix, &players[0], &plan, None, None).unwrap();
        assert!(simulation.boundary_state.finished);
        assert!(
            simulation
                .boundary_state
                .players
                .iter()
                .all(|p| p.victory_points == 7),
            "the tie must hold at the end"
        );
        assert_eq!(simulation.winner, Some(Some(players[1].clone())));
    }

    #[test]
    fn request_schemas_reject_steps_from_other_baskets() {
        for (kind, step) in [
            (
                "payment",
                "{\"kind\":\"produce\",\"unit\":\"fighter\",\"count\":1}",
            ),
            ("agenda_vote_planets", "{\"kind\":\"trade_good\"}"),
            (
                "production",
                "{\"kind\":\"vote_planet\",\"planet\":\"jord\"}",
            ),
            (
                "tactical_movement",
                "{\"kind\":\"exhaust\",\"planet\":\"jord\"}",
            ),
        ] {
            let destination = if kind == "production" || kind == "tactical_movement" {
                ",\"destination\":\"22\""
            } else {
                ""
            };
            let json = format!("{{\"kind\":\"{kind}\"{destination},\"steps\":[{step}]}}");
            assert!(
                serde_json::from_str::<MovementPlan>(&json).is_err(),
                "{json}"
            );
        }
        let legacy = serde_json::from_str::<MovementPlan>(
            r#"{"destination":"22","steps":[{"kind":"done_moving"}]}"#,
        )
        .unwrap();
        assert_eq!(legacy.kind, BatchKind::TacticalMovement);
        let payment = serde_json::from_str::<MovementPlan>(
            r#"{"kind":"payment","steps":[{"kind":"trade_good"}]}"#,
        )
        .unwrap();
        assert!(matches!(payment.steps[0], MovementStep::TradeGood));
    }

    #[test]
    fn movement_load_matches_the_explicit_cargo_variant_from_the_wire() {
        for galvanized in [false, true] {
            let plan: MovementPlan = serde_json::from_value(serde_json::json!({
                "kind": "tactical_movement",
                "destination": "42",
                "steps": [{
                    "kind": "load", "origin": "24", "unit": "infantry", "source": "home",
                    "damaged": false, "galvanized": galvanized
                }, {"kind": "done_moving"}]
            }))
            .expect("browser cargo variant must deserialize");
            let options = [true, false].map(|variant| {
                let mut option = ChoiceOption::labelled(
                    if variant {
                        "load-galvanized"
                    } else {
                        "load-normal"
                    },
                    "load",
                    "Load infantry",
                );
                option.payload = serde_json::from_value(serde_json::json!({
                    "system": "24", "unit": "infantry", "source": "home",
                    "damaged": false, "galvanized": variant
                }))
                .unwrap();
                option
            });
            let mut choice = offered("load_cargo", options.to_vec());
            choice.context.as_mut().unwrap().target =
                Some(DecisionTarget::System(ti4_model::id::SystemId::new("24")));
            let mut script = decider(plan.kind, plan.steps);
            let selected = script
                .choose(&choice)
                .expect("explicit cargo is unambiguous");
            assert_eq!(selected.payload["galvanized"], galvanized);
        }
    }

    #[test]
    fn legacy_load_without_a_cargo_variant_still_rejects_ambiguity() {
        let plan: MovementPlan = serde_json::from_str(
            r#"{"destination":"42","steps":[{"kind":"load","origin":"24","unit":"infantry","source":"home","damaged":false},{"kind":"done_moving"}]}"#,
        )
        .unwrap();
        let options = [false, true].map(|galvanized| {
            let mut option = ChoiceOption::labelled(
                if galvanized {
                    "load-galvanized"
                } else {
                    "load-normal"
                },
                "load",
                "Load infantry",
            );
            option.payload = serde_json::from_value(serde_json::json!({
                "system": "24", "unit": "infantry", "source": "home",
                "damaged": false, "galvanized": galvanized
            }))
            .unwrap();
            option
        });
        let mut choice = offered("load_cargo", options.to_vec());
        choice.context.as_mut().unwrap().target =
            Some(DecisionTarget::System(ti4_model::id::SystemId::new("24")));
        let mut script = decider(plan.kind, plan.steps);
        assert!(script.choose(&choice).is_err());
        assert_eq!(
            script.0.lock().unwrap().failure.as_ref().unwrap().reason,
            "ambiguous option"
        );
    }

    fn offered(subtype: &str, options: Vec<ChoiceOption>) -> Choice {
        let player = PlayerId::new("p1");
        Choice::new(player.clone(), "basket", options).contextualized(DecisionContext::new(
            player,
            DecisionSource::Rule("test".into()),
            subtype,
            Phase::Action,
            1,
        ))
    }

    fn decider(kind: BatchKind, steps: Vec<MovementStep>) -> PrivateDecider {
        PrivateDecider(Arc::new(Mutex::new(Script {
            prefix: VecDeque::new(),
            force: None,
            prefix_len: 0,
            steps,
            next: 0,
            actor: PlayerId::new("p1"),
            destination: String::new(),
            kind,
            payment_subtype: None,
            workflow_context: None,
            selected: Vec::new(),
            failure: None,
            finished: false,
            interruption: None,
            casualty_owed: None,
            answered: Vec::new(),
        })))
    }

    #[test]
    fn unplanned_cargo_hold_is_declined_when_the_plan_moves_on() {
        // A carrier with nothing at its origin still gets a load_cargo offer, because the
        // active system holds own infantry. The plan [Move, DoneMoving] carries nothing.
        let mut decider = decider(
            BatchKind::TacticalMovement,
            vec![
                MovementStep::Move {
                    origin: "20".into(),
                    unit: "carrier".into(),
                    damaged: false,
                },
                MovementStep::DoneMoving,
            ],
        );
        let mut mv = ChoiceOption::labelled("move|20|0", "move", "Carrier");
        mv.payload.insert("origin".into(), "20".into());
        mv.payload.insert("unit".into(), "carrier".into());
        let step = offered("movement_step", vec![mv]);
        assert_eq!(decider.choose(&step).unwrap().id, "move|20|0");

        let hold = offered(
            "load_cargo",
            vec![
                ChoiceOption::labelled("load|0", "load", "load infantry"),
                ChoiceOption::labelled("done_loading", "decline", "done loading"),
            ],
        );
        assert_eq!(decider.choose(&hold).unwrap().id, "done_loading");

        let finish = offered(
            "movement_step",
            vec![ChoiceOption::labelled("done_moving", "decline", "finish")],
        );
        assert_eq!(decider.choose(&finish).unwrap().id, "done_moving");
        let script = decider.0.lock().unwrap();
        assert!(script.failure.is_none());
        assert_eq!(script.next, 2);
    }

    #[test]
    fn cargo_hold_still_interrupts_a_plan_that_expected_to_load() {
        let mut decider = decider(
            BatchKind::TacticalMovement,
            vec![MovementStep::Load {
                origin: "20".into(),
                unit: "infantry".into(),
                source: None,
                damaged: false,
                galvanized: None,
            }],
        );
        let hold = offered(
            "load_cargo",
            vec![ChoiceOption::labelled(
                "done_loading",
                "decline",
                "done loading",
            )],
        );
        assert!(decider.choose(&hold).is_err());
    }

    #[test]
    fn payment_checks_each_repeated_trade_good_against_a_fresh_offer() {
        let mut decider = decider(
            BatchKind::Payment,
            vec![MovementStep::TradeGood, MovementStep::TradeGood],
        );
        let pay = offered(
            "pay_resources",
            vec![ChoiceOption::labelled("trade_good", "pay", "spend")],
        );
        assert_eq!(decider.choose(&pay).unwrap().id, "trade_good");
        let gone = offered(
            "pay_resources",
            vec![ChoiceOption::labelled("exhaust|jord", "pay", "exhaust")],
        );
        assert!(decider.choose(&gone).is_err());
        let script = decider.0.lock().unwrap();
        assert_eq!(script.next, 1);
        assert_eq!(script.failure.as_ref().unwrap().failed_step, 1);
    }

    #[test]
    fn payment_ends_at_the_boundary_when_the_engine_settles_the_rest() {
        // Jord pays 2 of 3; the engine then spends the lone remaining trade good itself and
        // moves on, so the planned trade good step must not reject the batch.
        let mut decider = decider(
            BatchKind::Payment,
            vec![
                MovementStep::Exhaust {
                    planet: "jord".into(),
                },
                MovementStep::TradeGood,
            ],
        );
        let pay = offered(
            "pay_influence",
            vec![
                ChoiceOption::labelled("exhaust|jord", "pay", "exhaust"),
                ChoiceOption::labelled("trade_good", "pay", "spend"),
            ],
        );
        assert_eq!(decider.choose(&pay).unwrap().id, "exhaust|jord");
        let next = offered(
            "gain_command_token",
            vec![ChoiceOption::labelled("tactic", "pool", "tactic pool")],
        );
        assert!(decider.choose(&next).is_err());
        let script = decider.0.lock().unwrap();
        assert!(script.finished);
        assert!(script.failure.is_none());
        assert_eq!(script.steps.len(), 1);
    }

    #[test]
    fn payment_that_never_starts_still_reports_the_interruption() {
        let mut decider = decider(BatchKind::Payment, vec![MovementStep::TradeGood]);
        let next = offered(
            "gain_command_token",
            vec![ChoiceOption::labelled("tactic", "pool", "tactic pool")],
        );
        assert!(decider.choose(&next).is_err());
        let script = decider.0.lock().unwrap();
        let failure = script.failure.as_ref().unwrap();
        assert_eq!(failure.reason, "workflow interrupted");
        let offered = failure.offered.as_ref().unwrap();
        assert_eq!(offered.subtype.as_deref(), Some("gain_command_token"));
        assert_eq!(offered.option_ids, vec!["tactic".to_owned()]);
        assert!(
            failure.message.contains("gain_command_token"),
            "{}",
            failure.message
        );
    }

    #[test]
    fn prefix_rejects_an_offer_change_before_consuming_a_planned_step() {
        let pay = offered(
            "pay_resources",
            vec![ChoiceOption::labelled("trade_good", "pay", "spend")],
        );
        let mut script = decider(BatchKind::Payment, vec![MovementStep::TradeGood]);
        script.0.lock().unwrap().prefix.push_back(DecisionRecord {
            player: pay.player.clone(),
            prompt: pay.prompt.clone(),
            chosen: "trade_good".into(),
            offered: vec!["trade_good".into()],
            context: pay.context.clone(),
        });
        let mut changed = pay;
        changed.options.push(ChoiceOption::decline());
        assert!(script.choose(&changed).is_err());
        assert_eq!(script.0.lock().unwrap().next, 0);
    }

    #[test]
    fn payment_cannot_change_currency_mid_batch() {
        let mut decider = decider(
            BatchKind::Payment,
            vec![MovementStep::TradeGood, MovementStep::TradeGood],
        );
        let pay = offered(
            "pay_resources",
            vec![ChoiceOption::labelled("trade_good", "pay", "spend")],
        );
        decider.choose(&pay).unwrap();
        assert!(
            decider
                .choose(&offered("pay_influence", pay.options))
                .is_err()
        );
        assert_eq!(
            decider.0.lock().unwrap().failure.as_ref().unwrap().reason,
            "workflow interrupted"
        );
    }

    #[test]
    fn payment_cannot_spill_into_a_different_payment_offer() {
        let mut decider = decider(
            BatchKind::Payment,
            vec![MovementStep::TradeGood, MovementStep::TradeGood],
        );
        let pay = offered(
            "pay_resources",
            vec![ChoiceOption::labelled("trade_good", "pay", "spend")],
        );
        decider.choose(&pay).unwrap();
        let mut next_bill = pay.clone();
        next_bill.context.as_mut().unwrap().source = DecisionSource::Rule("different bill".into());
        assert!(decider.choose(&next_bill).is_err());
        assert_eq!(
            decider.0.lock().unwrap().failure.as_ref().unwrap().reason,
            "workflow interrupted"
        );
    }

    #[test]
    fn late_mismatch_reports_step_fifty_without_consuming_it() {
        let mut decider = decider(BatchKind::Payment, vec![MovementStep::TradeGood; 50]);
        let pay = offered(
            "pay_resources",
            vec![ChoiceOption::labelled("trade_good", "pay", "spend")],
        );
        for _ in 0..49 {
            decider.choose(&pay).unwrap();
        }
        let unavailable = offered(
            "pay_resources",
            vec![ChoiceOption::labelled("exhaust|jord", "pay", "exhaust")],
        );
        assert!(decider.choose(&unavailable).is_err());
        let script = decider.0.lock().unwrap();
        assert_eq!(script.failure.as_ref().unwrap().failed_step, 49);
        assert_eq!(script.selected.len(), 49);
    }

    #[test]
    fn vote_stops_before_another_seat_answers() {
        let mut decider = decider(
            BatchKind::AgendaVotePlanets,
            vec![MovementStep::VotePlanet {
                planet: "jord".into(),
            }],
        );
        let choice = offered(
            "vote_exhaust_planet",
            vec![ChoiceOption::labelled("jord", "vote_planet", "vote")],
        );
        let mut other = choice.clone();
        other.player = PlayerId::new("p2");
        assert!(decider.choose(&other).is_err());
        assert_eq!(
            decider
                .0
                .lock()
                .unwrap()
                .failure
                .as_ref()
                .unwrap()
                .failed_step,
            0
        );
    }

    fn reaction(subtype: &str, seat: &str) -> Choice {
        let mut choice = offered(
            subtype,
            vec![
                ChoiceOption::labelled("play_card", "ability", "Play a card"),
                ChoiceOption::decline(),
            ],
        );
        choice.player = PlayerId::new(seat);
        choice
    }

    #[test]
    fn movement_stops_at_a_ship_moved_reaction_and_keeps_the_applied_steps() {
        let mv = |origin: &str| MovementStep::Move {
            origin: origin.into(),
            unit: "carrier".into(),
            damaged: false,
        };
        for seat in ["p1", "p2"] {
            let mut decider = decider(
                BatchKind::TacticalMovement,
                vec![mv("20"), mv("21"), MovementStep::DoneMoving],
            );
            let mut option = ChoiceOption::labelled("move|20|0", "move", "Carrier");
            option.payload.insert("origin".into(), "20".into());
            option.payload.insert("unit".into(), "carrier".into());
            assert_eq!(
                decider
                    .choose(&offered("movement_step", vec![option]))
                    .unwrap()
                    .id,
                "move|20|0"
            );
            let window = reaction("reaction_after_SHIP_MOVED", seat);
            assert!(decider.choose(&window).is_err());
            let script = decider.0.lock().unwrap();
            assert!(
                script.failure.is_none(),
                "a reaction is a boundary, not a rejection"
            );
            assert!(script.finished);
            let stop = script.interruption.as_ref().expect("interruption recorded");
            assert_eq!(stop.applied_steps, 1);
            assert_eq!(stop.remaining_steps.len(), 2);
            assert_eq!(stop.offered.own_seat, seat == "p1");
            assert_eq!(
                stop.offered.option_ids.is_empty(),
                seat != "p1",
                "another seat's options stay private"
            );
            assert_eq!(script.selected.len(), 1);
        }
    }

    #[test]
    fn vote_stops_at_a_votes_cast_reaction() {
        let planet = |id: &str| MovementStep::VotePlanet { planet: id.into() };
        let mut decider = decider(
            BatchKind::AgendaVotePlanets,
            vec![
                planet("jord"),
                planet("arc_prime"),
                MovementStep::DoneVoting,
            ],
        );
        let ask = offered(
            "vote_exhaust_planet",
            vec![ChoiceOption::labelled("jord", "vote_planet", "Jord")],
        );
        assert_eq!(decider.choose(&ask).unwrap().id, "jord");
        assert!(
            decider
                .choose(&reaction("reaction_after_VOTES_CAST", "p1"))
                .is_err()
        );
        let script = decider.0.lock().unwrap();
        assert!(script.failure.is_none());
        let stop = script.interruption.as_ref().unwrap();
        assert_eq!(stop.applied_steps, 1);
        assert_eq!(stop.remaining_steps.len(), 2);
    }

    #[test]
    fn a_reaction_before_any_step_is_applied_still_rejects_the_plan() {
        let mut decider = decider(
            BatchKind::AgendaVotePlanets,
            vec![MovementStep::VotePlanet {
                planet: "jord".into(),
            }],
        );
        assert!(
            decider
                .choose(&reaction("reaction_after_VOTES_CAST", "p1"))
                .is_err()
        );
        let script = decider.0.lock().unwrap();
        assert!(script.interruption.is_none());
        assert_eq!(
            script.failure.as_ref().unwrap().reason,
            "workflow interrupted"
        );
    }

    #[test]
    fn a_spent_plan_meeting_a_reaction_is_a_plain_boundary() {
        let mut decider = decider(
            BatchKind::AgendaVotePlanets,
            vec![MovementStep::VotePlanet {
                planet: "jord".into(),
            }],
        );
        let ask = offered(
            "vote_exhaust_planet",
            vec![ChoiceOption::labelled("jord", "vote_planet", "Jord")],
        );
        decider.choose(&ask).unwrap();
        assert!(
            decider
                .choose(&reaction("reaction_after_VOTES_CAST", "p1"))
                .is_err()
        );
        let script = decider.0.lock().unwrap();
        assert!(script.interruption.is_none(), "nothing was left over");
        assert!(script.finished);
    }

    #[test]
    fn a_non_reaction_surprise_still_rejects_the_plan() {
        let mut decider = decider(
            BatchKind::AgendaVotePlanets,
            vec![
                MovementStep::VotePlanet {
                    planet: "jord".into(),
                },
                MovementStep::VotePlanet {
                    planet: "arc_prime".into(),
                },
            ],
        );
        let ask = offered(
            "vote_exhaust_planet",
            vec![ChoiceOption::labelled("jord", "vote_planet", "Jord")],
        );
        decider.choose(&ask).unwrap();
        let other = offered(
            "ready_planet",
            vec![ChoiceOption::labelled("jord", "ready", "Jord")],
        );
        assert!(decider.choose(&other).is_err());
        let script = decider.0.lock().unwrap();
        assert!(script.interruption.is_none());
        assert!(script.failure.is_some());
    }

    fn hit_option(id: &str, kind: &str, unit: &str, damaged: bool) -> ChoiceOption {
        let mut option = ChoiceOption::labelled(id, kind, format!("{kind} {unit}"));
        option.payload.insert("unit".into(), unit.into());
        option.payload.insert("damaged".into(), damaged.into());
        option
    }

    fn hit_ask(subtype: &str, owed: i64, options: Vec<ChoiceOption>) -> Choice {
        use ti4_engine::decision_context::{ConstraintKind, OutstandingConstraint};
        let mut choice = offered(subtype, options);
        let context = choice.context.take().unwrap();
        choice.context = Some(
            context
                .about(DecisionTarget::System(ti4_model::id::SystemId::new("18")))
                .owing(OutstandingConstraint::new(
                    ConstraintKind::UnitsToRemove,
                    owed,
                    0,
                )),
        );
        choice
    }

    fn sustain_ask(owed: i64) -> Choice {
        hit_ask(
            "sustain_damage",
            owed,
            vec![
                hit_option("sustain|0", "sustain", "dreadnought", false),
                ChoiceOption::labelled("decline", "decline", "take the hit"),
            ],
        )
    }

    fn casualty_ask(owed: i64, units: &[&str]) -> Choice {
        hit_ask(
            "assign_casualty",
            owed,
            units
                .iter()
                .enumerate()
                .map(|(i, unit)| hit_option(&format!("destroy|{i}"), "casualty", unit, false))
                .collect(),
        )
    }

    fn destroy(unit: &str) -> MovementStep {
        MovementStep::Destroy {
            unit: unit.into(),
            damaged: false,
        }
    }

    #[test]
    fn casualty_plan_assigns_hits_across_several_ships() {
        let mut decider = decider(
            BatchKind::Casualties,
            vec![
                MovementStep::Sustain {
                    unit: "dreadnought".into(),
                },
                destroy("fighter"),
                destroy("carrier"),
            ],
        );
        assert_eq!(decider.choose(&sustain_ask(3)).unwrap().id, "sustain|0");
        // The next hit is offered for sustaining again; the plan loses a ship instead.
        assert_eq!(decider.choose(&sustain_ask(2)).unwrap().id, "decline");
        assert_eq!(
            decider
                .choose(&casualty_ask(2, &["fighter", "carrier"]))
                .unwrap()
                .id,
            "destroy|0"
        );
        assert_eq!(
            decider
                .choose(&casualty_ask(1, &["fighter", "carrier"]))
                .unwrap()
                .id,
            "destroy|1"
        );
        let script = decider.0.lock().unwrap();
        assert!(script.failure.is_none());
        assert_eq!(script.next, 3);
        assert_eq!(
            script.selected.len(),
            4,
            "the take-the-hit answer is recorded too"
        );
    }

    #[test]
    fn casualty_plan_sustains_a_ship_then_destroys_the_same_ship() {
        // Two hits on one undamaged dreadnought: the engine asks to sustain it, finds it damaged
        // and no longer offers a sustain, then offers it as a damaged casualty.
        let mut decider = decider(
            BatchKind::Casualties,
            vec![
                MovementStep::Sustain {
                    unit: "dreadnought".into(),
                },
                MovementStep::Destroy {
                    unit: "dreadnought".into(),
                    damaged: true,
                },
            ],
        );
        assert_eq!(decider.choose(&sustain_ask(2)).unwrap().id, "sustain|0");
        let damaged_ask = hit_ask(
            "assign_casualty",
            1,
            vec![hit_option("destroy|0", "casualty", "dreadnought", true)],
        );
        assert_eq!(decider.choose(&damaged_ask).unwrap().id, "destroy|0");
        let script = decider.0.lock().unwrap();
        assert!(script.failure.is_none());
        assert_eq!(script.next, 2);
        assert_eq!(script.selected.len(), 2);
    }

    #[test]
    fn casualty_plan_with_one_hit_stops_after_the_sustain() {
        // One hit only: the sustain absorbs it, the engine asks nothing more about this
        // assignment, and the destroy step must not answer whatever comes next.
        let mut decider = decider(
            BatchKind::Casualties,
            vec![
                MovementStep::Sustain {
                    unit: "dreadnought".into(),
                },
                MovementStep::Destroy {
                    unit: "dreadnought".into(),
                    damaged: true,
                },
            ],
        );
        assert_eq!(decider.choose(&sustain_ask(1)).unwrap().id, "sustain|0");
        let next = offered(
            "end_turn",
            vec![ChoiceOption::labelled("end", "end_turn", "end your turn")],
        );
        assert!(decider.choose(&next).is_err());
        let script = decider.0.lock().unwrap();
        assert!(script.failure.is_none());
        assert!(script.finished);
        assert_eq!(script.steps.len(), 1);
    }

    #[test]
    fn casualty_plan_does_not_destroy_an_undamaged_ship_as_damaged() {
        // The engine offers the fresh dreadnought only as an intact casualty; a plan that
        // destroys it "as damaged" without sustaining first is rejected, not silently changed.
        let mut decider = decider(
            BatchKind::Casualties,
            vec![MovementStep::Destroy {
                unit: "dreadnought".into(),
                damaged: true,
            }],
        );
        let ask = hit_ask(
            "assign_casualty",
            1,
            vec![hit_option("destroy|0", "casualty", "dreadnought", false)],
        );
        assert!(decider.choose(&ask).is_err());
        let script = decider.0.lock().unwrap();
        assert_eq!(
            script.failure.as_ref().unwrap().reason,
            "option unavailable"
        );
    }

    #[test]
    fn casualty_plan_rejects_a_ship_the_engine_does_not_offer() {
        let mut decider = decider(BatchKind::Casualties, vec![destroy("war_sun")]);
        assert!(
            decider
                .choose(&casualty_ask(1, &["fighter", "carrier"]))
                .is_err()
        );
        let script = decider.0.lock().unwrap();
        let failure = script.failure.as_ref().unwrap();
        assert_eq!(failure.reason, "option unavailable");
        assert_eq!(failure.failed_step, 0);
        assert!(script.selected.is_empty());
    }

    #[test]
    fn casualty_plan_stops_at_another_seats_reaction() {
        // Direct Hit: the opponent may react after the sustain. The sustained hit stands; the
        // rest of the plan is dropped for the seat to re-plan once the window closes.
        let mut decider = decider(
            BatchKind::Casualties,
            vec![
                MovementStep::Sustain {
                    unit: "dreadnought".into(),
                },
                destroy("fighter"),
            ],
        );
        decider.choose(&sustain_ask(2)).unwrap();
        let mut reaction = offered(
            "play_reaction_direct_hit",
            vec![ChoiceOption::labelled("decline", "decline", "pass")],
        );
        reaction.player = PlayerId::new("p2");
        assert!(decider.choose(&reaction).is_err());
        let script = decider.0.lock().unwrap();
        assert!(script.failure.is_none());
        assert!(script.finished);
        assert_eq!(script.steps.len(), 1);
    }

    #[test]
    fn casualty_plan_never_spills_into_the_next_rounds_hits() {
        // The engine took the second hit itself (only fighters were left), and the next question
        // is a new round owing two hits again: the leftover step must not answer it.
        let mut decider = decider(
            BatchKind::Casualties,
            vec![destroy("carrier"), destroy("fighter")],
        );
        decider
            .choose(&casualty_ask(2, &["fighter", "carrier"]))
            .unwrap();
        assert!(
            decider
                .choose(&casualty_ask(2, &["fighter", "carrier"]))
                .is_err()
        );
        let script = decider.0.lock().unwrap();
        assert!(script.failure.is_none());
        assert!(script.finished);
        assert_eq!(script.steps.len(), 1);
    }

    #[test]
    fn casualty_plan_answers_ground_combat_hits() {
        let mut decider = decider(BatchKind::Casualties, vec![destroy("infantry")]);
        let ground = offered(
            "assign_ground_casualty",
            vec![
                hit_option("destroy|0", "ground_casualty", "mech", true),
                hit_option("destroy|1", "ground_casualty", "infantry", false),
            ],
        );
        assert_eq!(decider.choose(&ground).unwrap().id, "destroy|1");
    }

    #[test]
    fn casualty_plans_accept_only_casualty_steps() {
        let plan = serde_json::from_str::<MovementPlan>(
            r#"{"kind":"casualties","steps":[{"kind":"sustain","unit":"dreadnought"},{"kind":"destroy","unit":"fighter","damaged":false}]}"#,
        )
        .unwrap();
        assert_eq!(plan.kind, BatchKind::Casualties);
        assert!(plan_problem(&plan).is_none());
        assert!(
            serde_json::from_str::<MovementPlan>(
                r#"{"kind":"casualties","steps":[{"kind":"trade_good"}]}"#
            )
            .is_err()
        );
    }

    fn pool_option(id: &str) -> ChoiceOption {
        ChoiceOption::labelled(id, "pool", id.replace("_tokens", " pool"))
    }

    fn gain_ask() -> Choice {
        offered(
            "gain_command_token",
            vec![
                pool_option("tactic_tokens"),
                pool_option("fleet_tokens"),
                pool_option("strategic_tokens"),
            ],
        )
    }

    fn pool(id: &str) -> MovementStep {
        MovementStep::Pool { pool: id.into() }
    }

    #[test]
    fn token_plan_places_each_token_into_the_planned_pool_in_order() {
        let mut decider = decider(
            BatchKind::Tokens,
            vec![
                pool("fleet_tokens"),
                pool("fleet_tokens"),
                pool("tactic_tokens"),
            ],
        );
        assert_eq!(decider.choose(&gain_ask()).unwrap().id, "fleet_tokens");
        assert_eq!(decider.choose(&gain_ask()).unwrap().id, "fleet_tokens");
        assert_eq!(decider.choose(&gain_ask()).unwrap().id, "tactic_tokens");
        // The plan is spent: the next question, whatever it is, ends the batch cleanly.
        assert!(decider.choose(&gain_ask()).is_err());
        let script = decider.0.lock().unwrap();
        assert!(
            script.failure.is_none(),
            "an exhausted plan is a boundary, not a failure"
        );
        assert!(script.finished);
        assert_eq!(script.selected.len(), 3);
    }

    #[test]
    fn token_plan_is_interrupted_when_the_engine_asks_something_else() {
        let mut decider = decider(
            BatchKind::Tokens,
            vec![pool("tactic_tokens"), pool("fleet_tokens")],
        );
        assert_eq!(decider.choose(&gain_ask()).unwrap().id, "tactic_tokens");
        let other = offered(
            "ready_planet",
            vec![ChoiceOption::labelled("jord", "ready", "Jord")],
        );
        assert!(decider.choose(&other).is_err());
        assert!(
            decider.0.lock().unwrap().failure.is_some(),
            "a plan with a token left over must not end silently"
        );
    }

    #[test]
    fn token_plan_rejects_a_pool_the_engine_does_not_offer() {
        let mut decider = decider(BatchKind::Tokens, vec![pool("moon_tokens")]);
        assert!(decider.choose(&gain_ask()).is_err());
        assert!(decider.0.lock().unwrap().failure.is_some());
    }

    #[test]
    fn token_plan_is_for_the_planning_seat_only() {
        let mut decider = decider(BatchKind::Tokens, vec![pool("tactic_tokens")]);
        let player = PlayerId::new("p2");
        let theirs = Choice::new(player.clone(), "basket", vec![pool_option("tactic_tokens")])
            .contextualized(DecisionContext::new(
                player,
                DecisionSource::Rule("test".into()),
                "gain_command_token",
                Phase::Action,
                1,
            ));
        assert!(decider.choose(&theirs).is_err());
    }

    #[test]
    fn token_plans_accept_only_pool_steps_and_at_most_sixteen() {
        let plan = serde_json::from_str::<MovementPlan>(
            r#"{"kind":"tokens","steps":[{"kind":"pool","pool":"tactic_tokens"},{"kind":"pool","pool":"fleet_tokens"}]}"#,
        )
        .unwrap();
        assert_eq!(plan.kind, BatchKind::Tokens);
        assert!(plan_problem(&plan).is_none());
        assert!(
            serde_json::from_str::<MovementPlan>(
                r#"{"kind":"tokens","steps":[{"kind":"produce","unit":"fighter","count":1}]}"#
            )
            .is_err()
        );
        let many = MovementPlan {
            kind: BatchKind::Tokens,
            destination: String::new(),
            steps: (0..17).map(|_| pool("tactic_tokens")).collect(),
        };
        assert!(plan_problem(&many).unwrap().contains("at most 16"));
        let empty = MovementPlan {
            kind: BatchKind::Tokens,
            destination: String::new(),
            steps: Vec::new(),
        };
        assert!(plan_problem(&empty).is_some());
    }

    fn buy_ask() -> Choice {
        offered(
            "buy_token_with_influence",
            vec![
                ChoiceOption::labelled("no", "strategy", "spend nothing further"),
                ChoiceOption::labelled("yes", "strategy", "spend 3 influence"),
            ],
        )
    }

    fn pay_ask() -> Choice {
        offered(
            "pay_influence",
            vec![
                ChoiceOption::labelled(
                    "exhaust|arcturus",
                    "pay",
                    "exhaust arcturus for 4 influence",
                ),
                ChoiceOption::labelled("trade_good", "pay", "spend a trade good"),
            ],
        )
    }

    fn buy(buy: bool) -> MovementStep {
        MovementStep::Purchase { buy }
    }

    fn exhaust(planet: &str) -> MovementStep {
        MovementStep::Exhaust {
            planet: planet.into(),
        }
    }

    #[test]
    fn purchase_plan_answers_the_questions_in_the_order_the_engine_asks_them() {
        // Primary: a free token, then per purchase: yes, its payment, its pool; then "no".
        let mut decider = decider(
            BatchKind::Tokens,
            vec![
                pool("fleet_tokens"),
                buy(true),
                exhaust("arcturus"),
                pool("tactic_tokens"),
                buy(false),
            ],
        );
        assert_eq!(decider.choose(&gain_ask()).unwrap().id, "fleet_tokens");
        assert_eq!(decider.choose(&buy_ask()).unwrap().id, "yes");
        assert_eq!(decider.choose(&pay_ask()).unwrap().id, "exhaust|arcturus");
        assert_eq!(decider.choose(&gain_ask()).unwrap().id, "tactic_tokens");
        assert_eq!(decider.choose(&buy_ask()).unwrap().id, "no");
        let script = decider.0.lock().unwrap();
        assert!(script.failure.is_none());
        assert_eq!(script.selected.len(), 5);
    }

    #[test]
    fn payments_the_engine_settled_without_asking_are_skipped() {
        // With only trade goods to spend the engine takes each lone option silently, so the
        // planned trade_good steps never meet a question and must not block the pool step.
        let mut decider = decider(
            BatchKind::Tokens,
            vec![
                buy(true),
                MovementStep::TradeGood,
                MovementStep::TradeGood,
                MovementStep::TradeGood,
                pool("strategic_tokens"),
            ],
        );
        assert_eq!(decider.choose(&buy_ask()).unwrap().id, "yes");
        assert_eq!(decider.choose(&gain_ask()).unwrap().id, "strategic_tokens");
        let script = decider.0.lock().unwrap();
        assert!(script.failure.is_none());
        assert_eq!(script.next, 5);
        assert_eq!(script.selected.len(), 2);
    }

    #[test]
    fn a_purchase_answer_the_engine_does_not_ask_is_rejected() {
        // The seat can no longer afford the purchase: the engine moves straight to the next
        // decision, so the planned `yes` fails instead of being skipped.
        let mut decider = decider(BatchKind::Tokens, vec![buy(true), pool("tactic_tokens")]);
        assert!(decider.choose(&gain_ask()).is_err());
        assert!(decider.0.lock().unwrap().failure.is_some());
    }

    #[test]
    fn a_payment_the_engine_offers_differently_is_rejected() {
        let mut decider = decider(
            BatchKind::Tokens,
            vec![buy(true), exhaust("arinam"), pool("tactic_tokens")],
        );
        assert_eq!(decider.choose(&buy_ask()).unwrap().id, "yes");
        assert!(decider.choose(&pay_ask()).is_err());
        assert!(decider.0.lock().unwrap().failure.is_some());
    }

    #[test]
    fn a_purchase_plan_stops_at_a_reaction_window_keeping_the_rest() {
        let mut decider = decider(
            BatchKind::Tokens,
            vec![
                buy(true),
                exhaust("arcturus"),
                pool("tactic_tokens"),
                buy(false),
            ],
        );
        assert_eq!(decider.choose(&buy_ask()).unwrap().id, "yes");
        assert_eq!(decider.choose(&pay_ask()).unwrap().id, "exhaust|arcturus");
        let reaction = offered(
            "reaction_after_planet_exhausted",
            vec![ChoiceOption::labelled("decline", "decline", "decline")],
        );
        assert!(decider.choose(&reaction).is_err());
        let script = decider.0.lock().unwrap();
        assert!(script.failure.is_none());
        let interruption = script.interruption.as_ref().expect("interrupted");
        assert_eq!(interruption.applied_steps, 2);
        assert_eq!(interruption.remaining_steps.len(), 2);
        assert!(matches!(
            interruption.remaining_steps[0],
            MovementStep::Pool { .. }
        ));
    }

    #[test]
    fn token_plans_with_purchases_parse_and_validate() {
        let plan = serde_json::from_str::<MovementPlan>(
            r#"{"kind":"tokens","steps":[{"kind":"pool","pool":"tactic_tokens"},{"kind":"purchase","buy":true},{"kind":"exhaust","planet":"arcturus"},{"kind":"trade_good"},{"kind":"pool","pool":"fleet_tokens"},{"kind":"purchase","buy":false}]}"#,
        )
        .unwrap();
        assert_eq!(plan.kind, BatchKind::Tokens);
        assert!(plan_problem(&plan).is_none());
        // Payments and purchases do not count against the 16 tokens a faction can hold.
        let many = MovementPlan {
            kind: BatchKind::Tokens,
            destination: String::new(),
            steps: (0..16)
                .flat_map(|_| [buy(true), MovementStep::TradeGood, pool("tactic_tokens")])
                .collect(),
        };
        assert!(plan_problem(&many).is_none());
    }

    fn ready_ask(planets: &[&str]) -> Choice {
        offered(
            "ready_planet",
            planets
                .iter()
                .map(|p| ChoiceOption::labelled(*p, "ready", format!("ready {p}")))
                .collect(),
        )
    }

    fn ready(planet: &str) -> MovementStep {
        MovementStep::ReadyPlanet {
            planet: planet.into(),
        }
    }

    #[test]
    fn ready_plan_answers_both_questions_in_the_chosen_order() {
        let mut decider = decider(
            BatchKind::ReadyPlanets,
            vec![ready("jord"), ready("arcturus")],
        );
        assert_eq!(
            decider
                .choose(&ready_ask(&["arcturus", "jord"]))
                .unwrap()
                .id,
            "jord"
        );
        assert_eq!(
            decider.choose(&ready_ask(&["arcturus"])).unwrap().id,
            "arcturus"
        );
        assert!(decider.choose(&ready_ask(&[])).is_err());
        let script = decider.0.lock().unwrap();
        assert!(script.failure.is_none());
        assert!(script.finished);
        assert_eq!(script.selected.len(), 2);
    }

    #[test]
    fn ready_plan_with_one_planet_ends_at_the_boundary() {
        let mut decider = decider(BatchKind::ReadyPlanets, vec![ready("jord")]);
        assert_eq!(
            decider
                .choose(&ready_ask(&["arcturus", "jord"]))
                .unwrap()
                .id,
            "jord"
        );
        assert!(decider.choose(&ready_ask(&["arcturus"])).is_err());
        assert!(decider.0.lock().unwrap().failure.is_none());
    }

    #[test]
    fn ready_plan_rejects_a_planet_that_is_no_longer_exhausted() {
        let mut decider = decider(BatchKind::ReadyPlanets, vec![ready("jord")]);
        assert!(decider.choose(&ready_ask(&["arcturus"])).is_err());
        let script = decider.0.lock().unwrap();
        assert_eq!(
            script.failure.as_ref().unwrap().reason,
            "option unavailable"
        );
    }

    #[test]
    fn ready_plan_is_interrupted_when_the_engine_asks_something_else() {
        let mut decider = decider(
            BatchKind::ReadyPlanets,
            vec![ready("jord"), ready("arcturus")],
        );
        assert_eq!(
            decider
                .choose(&ready_ask(&["arcturus", "jord"]))
                .unwrap()
                .id,
            "jord"
        );
        let other = offered(
            "gain_command_token",
            vec![ChoiceOption::labelled("tactic_tokens", "pool", "tactic")],
        );
        assert!(decider.choose(&other).is_err());
        assert!(decider.0.lock().unwrap().failure.is_some());
    }

    #[test]
    fn ready_plans_parse_and_validate_their_shape() {
        let plan = serde_json::from_str::<MovementPlan>(
            r#"{"kind":"ready_planets","steps":[{"kind":"ready_planet","planet":"jord"},{"kind":"ready_planet","planet":"arcturus"}]}"#,
        )
        .unwrap();
        assert_eq!(plan.kind, BatchKind::ReadyPlanets);
        assert!(plan_problem(&plan).is_none());
        let with = |steps: Vec<MovementStep>| MovementPlan {
            kind: BatchKind::ReadyPlanets,
            destination: String::new(),
            steps,
        };
        assert!(plan_problem(&with(vec![])).is_some(), "zero is not a plan");
        assert!(plan_problem(&with(vec![ready("a"), ready("b"), ready("c")])).is_some());
        assert!(plan_problem(&with(vec![ready("a"), ready("a")])).is_some());
        assert!(plan_problem(&with(vec![MovementStep::TradeGood])).is_some());
        assert!(
            serde_json::from_str::<MovementPlan>(
                r#"{"kind":"ready_planets","steps":[{"kind":"trade_good"}]}"#
            )
            .is_err(),
            "cross-workflow steps are refused at the boundary"
        );
    }

    /// Cabal Amalgamation: `build|carrier|1` and `exchange|carrier` are both `produce` options of
    /// one `produce_unit` decision with the same unit and count. A plan step names which one it
    /// means; a plain step never matches the exchange (it used to match both: "ambiguous option").
    #[test]
    fn a_produce_step_picks_the_build_or_the_exchange_but_never_both() {
        let build = ChoiceOption::labelled("build|carrier|1", "produce", "produce 1x carrier")
            .with("unit", "carrier")
            .with("count", 1i64);
        let exchange = ChoiceOption::labelled(
            "exchange|carrier",
            "produce",
            "produce 1x carrier by returning",
        )
        .with("unit", "carrier")
        .with("count", 1i64)
        .with("exchange", true);
        let plain = |exchange| MovementStep::Produce {
            unit: "carrier".into(),
            count: 1,
            exchange,
        };
        let offered = [build.clone(), exchange.clone()];
        let hits = |step: &MovementStep| {
            offered
                .iter()
                .filter(|o| step.matches_option(o))
                .map(|o| o.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(hits(&plain(false)), vec!["build|carrier|1".to_owned()]);
        assert_eq!(hits(&plain(true)), vec!["exchange|carrier".to_owned()]);
        let parsed = serde_json::from_str::<MovementPlan>(
            r#"{"kind":"production","destination":"27","steps":[{"kind":"produce","unit":"carrier","count":1,"exchange":true},{"kind":"produce","unit":"carrier","count":1}]}"#,
        )
        .unwrap();
        assert!(plan_problem(&parsed).is_none());
        assert!(matches!(
            parsed.steps[0],
            MovementStep::Produce { exchange: true, .. }
        ));
        assert!(matches!(
            parsed.steps[1],
            MovementStep::Produce {
                exchange: false,
                ..
            }
        ));
    }
}
