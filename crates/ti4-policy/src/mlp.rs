//! The decision pipeline of an MLP policy, without the network.
//!
//! One decision of an MLP bot is two calls with the forward pass of a model between them:
//!
//! ```text
//! Pipeline::features(model, choice, seen) -> Features::Score(Prepared)   the options as columns
//! Scorer::logits(faction, head, &prepared.options)                       the model
//! Pipeline::pick(&prepared, &logits, temperature, rng) -> Picked         the softmax and the draw
//! Pipeline::fleet(model, choice, seen, &picked)                          after an activation
//! ```
//!
//! A host that only plays calls [`Pipeline::decide`]. Training (`ti4-mlp`'s `MlpBot`) calls the
//! steps itself and records the columns and the probabilities between them, so play, evaluation
//! and training run the same features, heads and draw whichever model scores them: libtorch in
//! `ti4-mlp`, or the kernels of `ti4-infer`. This crate depends on neither; a model is a
//! [`Scorer`].
//!
//! What the pipeline covers:
//!
//! - the fleet plan: after an activation, a model with a battle predictor of fact version 7 or
//!   later picks one of the candidate fleets ([`crate::tactical_plan::packages`]) and the plan
//!   answers the movement prompts that follow ([`Features::Planned`]);
//! - the features: [`crate::projection::mlp_choice_features`] (with the card-text families when
//!   the vocabulary places them), the battle facts when the model has a predictor, the deal facts
//!   when the vocabulary places [`crate::deal_value::FACT_SCORE`], and the power facts when it
//!   places [`crate::power_facts::NAMES`];
//! - the columns of the vocabulary, memoised per pipeline;
//! - the head ([`crate::learned::decision_head`], folded into the model's [`HeadLayout`]);
//! - the softmax at a temperature ([`probabilities`]) and the cumulative draw ([`draw`]).
//!
//! A pipeline belongs to one seat: it keeps the seat's plan, its memo and its power maps.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use rand::Rng;
use ti4_engine::choice::{Choice, ChoiceOption, Observed, SeatObservation};
use ti4_model::id::{PlayerId, SystemId};

use crate::battle::BattlePredictor;
use crate::intern::FeatureKey;
use crate::progress::Baseline;
use crate::tactical_plan::{Execution, Package, PlanState};
use crate::vocabulary::Vocabulary;

/// The **pinned** roster of faction identities, in the order of their rows in a model.
///
/// A trained row is addressed by index, so this list is frozen: reordering it repoints every
/// faction's residual and embedding. `ti4_infer::FACTION_ROSTER` is the same list.
pub const FACTION_ROSTER: [&str; 33] = [
    "arborec",
    "argent",
    "bastion",
    "cabal",
    "crimson",
    "deepwrought",
    "empyrean",
    "firmament",
    "ghost",
    "hacan",
    "jolnar",
    "keleresa",
    "keleresm",
    "keleresx",
    "l1z1x",
    "letnev",
    "mahact",
    "mentak",
    "muaat",
    "naalu",
    "naaz",
    "nekro",
    "nomad",
    "obsidian",
    "ralnel",
    "saar",
    "sardakk",
    "sol",
    "titans",
    "winnu",
    "xxcha",
    "yin",
    "yssaril",
];

/// The row of a faction alias in [`FACTION_ROSTER`].
#[must_use]
pub fn faction_row(alias: &str) -> Option<usize> {
    FACTION_ROSTER.iter().position(|known| *known == alias)
}

/// The temperature a bot plays a model at when the model names none: the evaluation temperature
/// of `plans/MLP_TEMPERATURE_REGIME.md`.
pub const DEFAULT_TEMPERATURE: f64 = 0.25;

/// The option kind of a fleet decision's synthetic options.
pub const PACKAGE_KIND: &str = "package";

/// The positional readout layout of a model: which head each readout row scores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadLayout {
    /// Schemas 7 and 8: the original fourteen rows.
    Legacy,
    /// Schemas 9 to 12: the same rows plus `diplomacy` at the end.
    Diplomacy,
}

impl HeadLayout {
    /// The heads, in the order of their rows.
    #[must_use]
    pub const fn heads(self) -> &'static [&'static str] {
        match self {
            Self::Legacy => &crate::learned::STAGE1_DECISION_HEADS,
            Self::Diplomacy => &crate::learned::DIPLOMACY_DECISION_HEADS,
        }
    }

    /// The layout whose heads are exactly `heads`, in order.
    #[must_use]
    pub fn of<S: AsRef<str>>(heads: &[S]) -> Option<Self> {
        [Self::Legacy, Self::Diplomacy].into_iter().find(|layout| {
            let known = layout.heads();
            known.len() == heads.len()
                && known
                    .iter()
                    .zip(heads)
                    .all(|(known, head)| *known == head.as_ref())
        })
    }

    /// The head of this layout that scores a requested head: the head itself, or `other` for a
    /// head the layout does not have.
    #[must_use]
    pub fn resolve(self, requested: &str) -> &'static str {
        self.heads()
            .iter()
            .find(|head| **head == requested)
            .copied()
            .unwrap_or("other")
    }

    /// The row of a head.
    #[must_use]
    pub fn index(self, head: &str) -> Option<usize> {
        self.heads().iter().position(|known| *known == head)
    }
}

/// One option as the model reads it: its active `(column, value)` pairs. A column may be named
/// more than once; the model sums its values in a fixed order.
pub type Columns = Vec<(u32, f32)>;

/// What the pipeline reads of a model to build the features of a decision.
pub trait ModelInfo {
    /// The readout layout.
    fn layout(&self) -> HeadLayout;
    /// The frozen battle predictor of an arena-capable model (schemas 11 and 12).
    fn battle_predictor(&self) -> Option<&BattlePredictor>;
    /// The temperature a bot plays the model at.
    fn temperature(&self) -> f64 {
        DEFAULT_TEMPERATURE
    }
}

/// A model: the logits of the options of one decision.
pub trait Scorer: ModelInfo {
    /// One logit per option, for a decision of the faction row `faction` (in [`FACTION_ROSTER`])
    /// on the head row `head` (in [`ModelInfo::layout`]).
    ///
    /// # Errors
    /// [`MlpError::Model`] when the model refuses the decision.
    fn logits(
        &self,
        faction: usize,
        head: usize,
        options: &[Columns],
    ) -> Result<Vec<f32>, MlpError>;
}

/// Anything that stopped a decision of the pipeline.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MlpError {
    /// A decision with no options.
    #[error("the legal set is empty")]
    EmptyLegalSet,
    /// A temperature that is not positive and finite: dividing logits by it is not a softmax.
    #[error("temperature {0} is not positive")]
    Temperature(f64),
    /// A logit, probability or normaliser that is not finite, or a distribution that does not sum
    /// to one. Never returned as a plausible-looking distribution.
    #[error("{what} is not usable: {detail}")]
    NotUsable { what: &'static str, detail: String },
    /// Features that cannot be scored: a value that is not finite, a column that does not fit, or
    /// a vector count that is not the option count.
    #[error("{0}")]
    Features(String),
    /// The model refused the decision.
    #[error("{0}")]
    Model(String),
}

/// Softmax with the maximum subtracted first, validated end to end.
///
/// Subtracting the max is what stops `exp` overflowing on a logit around 90. Every stage is
/// checked, because a model refusal that becomes a legal-looking action is worse than a refusal:
/// a `NaN` logit, an all-`-inf` set, or a normaliser that underflows to zero is an error.
///
/// # Errors
/// [`MlpError::EmptyLegalSet`] for no scores; [`MlpError::NotUsable`] for non-finite scores, a
/// non-finite or non-positive normaliser, or a distribution that is not a distribution.
pub fn softmax(scores: &[f32]) -> Result<Vec<f64>, MlpError> {
    if scores.is_empty() {
        return Err(MlpError::EmptyLegalSet);
    }
    if let Some(bad) = scores.iter().copied().find(|value| !value.is_finite()) {
        return Err(MlpError::NotUsable {
            what: "a logit",
            detail: format!("{bad}"),
        });
    }
    let highest = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let weights: Vec<f64> = scores
        .iter()
        .map(|value| f64::from(*value - highest).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return Err(MlpError::NotUsable {
            what: "the softmax normaliser",
            detail: format!("{total}"),
        });
    }
    let probabilities: Vec<f64> = weights.into_iter().map(|weight| weight / total).collect();
    if let Some(bad) = probabilities
        .iter()
        .copied()
        .find(|value| !value.is_finite() || *value < 0.0)
    {
        return Err(MlpError::NotUsable {
            what: "a probability",
            detail: format!("{bad}"),
        });
    }
    let sum: f64 = probabilities.iter().sum();
    if (sum - 1.0).abs() > 1e-9 {
        return Err(MlpError::NotUsable {
            what: "the distribution total",
            detail: format!("{sum}"),
        });
    }
    Ok(probabilities)
}

/// The distribution of a decision: `softmax(logits / temperature)`.
///
/// Each logit is divided in `f32` by the temperature rounded to `f32`, as libtorch divides an
/// `f32` tensor by a scalar, so these are the bits of `ti4-mlp`'s `Actor::probabilities`.
///
/// # Errors
/// [`MlpError::Temperature`], then as [`softmax`].
pub fn probabilities(logits: &[f32], temperature: f64) -> Result<Vec<f64>, MlpError> {
    if logits.is_empty() {
        return Err(MlpError::EmptyLegalSet);
    }
    if !temperature.is_finite() || temperature <= 0.0 {
        return Err(MlpError::Temperature(temperature));
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the division is in f32, as libtorch's"
    )]
    let divisor = temperature as f32;
    let scores: Vec<f32> = logits.iter().map(|logit| logit / divisor).collect();
    softmax(&scores)
}

/// One draw from a distribution: a uniform `[0, 1)` and the cumulative walk, the last option when
/// rounding leaves the draw above the total.
pub fn draw<R: Rng>(probabilities: &[f64], rng: &mut R) -> usize {
    let draw: f64 = rng.random_range(0.0..1.0);
    let mut cumulative = 0.0;
    for (index, probability) in probabilities.iter().enumerate() {
        cumulative += *probability;
        if draw < cumulative {
            return index;
        }
    }
    probabilities.len().saturating_sub(1)
}

/// One fleet on a decision's menu, as a trace shows it: label, probability, facts.
pub type PackageOptionTrace = (String, f64, Vec<(String, f64)>);

/// One entry of a seat's plan trace.
#[derive(Debug, Clone)]
pub enum PlanTrace {
    /// A fleet decision: the menu, what the model made of it, and the pick.
    Package {
        player: String,
        system: String,
        /// The menu, the manual option last.
        options: Vec<PackageOptionTrace>,
        chosen: usize,
    },
    /// An engine prompt answered by the plan rather than the model.
    Planned {
        player: String,
        prompt: String,
        chosen: String,
    },
    /// The plan stopped before it was finished.
    Stopped { player: String, reason: String },
}

/// What [`Pipeline::features`] makes of a choice.
#[derive(Debug)]
pub enum Features {
    /// The fleet plan of the seat answers the choice; the model is not asked.
    Planned(ChoiceOption),
    /// The model scores the options.
    Score(Prepared),
}

/// A decision ready for the model: the faction row, the head row and one column list per option.
#[derive(Debug, Clone)]
pub struct Prepared {
    /// The faction row of the seat.
    pub faction: usize,
    /// The head the decision asks for ([`crate::learned::decision_head`]).
    pub requested_head: &'static str,
    /// The head of the model's layout that scores it.
    pub head: &'static str,
    /// The row of [`Self::head`].
    pub head_index: usize,
    /// One column list per option.
    pub options: Vec<Columns>,
    /// The options they stand for, in the same order: the choice's, or a fleet menu's.
    pub choices: Vec<ChoiceOption>,
    /// Feature names that found a column of their own.
    pub assigned: usize,
    /// Feature names that fell to an out-of-vocabulary column.
    pub oov: usize,
    stage: Stage,
}

impl Prepared {
    /// Whether this is a fleet decision ([`Pipeline::fleet`]) rather than an engine choice.
    #[must_use]
    pub const fn is_fleet(&self) -> bool {
        matches!(self.stage, Stage::Fleet { .. })
    }
}

#[derive(Debug, Clone)]
enum Stage {
    Choice,
    Fleet {
        player: PlayerId,
        system: SystemId,
        menu: Vec<Package>,
        facts: Vec<Vec<(&'static str, f64)>>,
    },
}

/// The option drawn for a [`Prepared`] decision.
#[derive(Debug, Clone)]
pub struct Picked {
    /// Its index in the options.
    pub index: usize,
    /// The option: of the choice, or the synthetic option of a fleet.
    pub option: ChoiceOption,
    /// The distribution it was drawn from.
    pub probabilities: Vec<f64>,
    /// What a training note records: the option's id, or the strategy of a fleet (`manual`).
    pub label: String,
    /// The option declines, or the fleet moves nothing.
    pub declined: bool,
}

/// A fleet being carried out, tied to the activation it was chosen for.
#[derive(Debug, Clone)]
struct PlanSlot {
    player: PlayerId,
    system: SystemId,
    execution: Execution,
}

/// The pipeline of one seat: its vocabulary, its faction row, and the state its decisions carry
/// from one to the next (the fleet plan, the memo of resolved columns, the power maps).
#[derive(Debug)]
pub struct Pipeline {
    vocabulary: Arc<Vocabulary>,
    faction: usize,
    baseline: Baseline,
    /// Whether the card-text families (`card-tag:*`, `card-tag-opt:*`) are part of the input: on
    /// iff the vocabulary places **every** name of both families.
    card_tags: bool,
    /// Whether the vocabulary places the deal values of diplomacy options.
    deal_facts: bool,
    /// Whether the vocabulary places the power-projection facts.
    power_facts: bool,
    /// Resolved `FeatureKey -> (column, assigned)`. Safe because the vocabulary is immutable
    /// behind its `Arc`.
    resolved: HashMap<FeatureKey, (usize, bool)>,
    /// The fleet the seat chose at its last activation, while it is being moved.
    plan: Option<PlanSlot>,
    /// The seat's power map, refreshed only where the board changed.
    power_cache: RefCell<crate::power_map::PowerCache>,
    /// Every seat's power map as it stood at the seat's latest activation decision.
    last_table: Option<crate::power_facts::TablePower>,
    trace: Vec<PlanTrace>,
}

impl Pipeline {
    /// The pipeline of a seat of the faction row `faction`, against a model's vocabulary.
    #[must_use]
    pub fn new(vocabulary: Arc<Vocabulary>, faction: usize) -> Self {
        Self {
            card_tags: crate::projection::card_tags_enabled_for(&vocabulary),
            deal_facts: vocabulary.is_assigned(crate::deal_value::FACT_SCORE),
            power_facts: vocabulary.is_assigned(crate::power_facts::NAMES[0]),
            vocabulary,
            faction,
            baseline: Baseline::default(),
            resolved: HashMap::new(),
            plan: None,
            power_cache: RefCell::new(crate::power_map::PowerCache::default()),
            last_table: None,
            trace: Vec::new(),
        }
    }

    /// The seat's setup baseline, which the opening-progress facts are deltas against.
    pub const fn set_baseline(&mut self, baseline: Baseline) {
        self.baseline = baseline;
    }

    /// The seat's setup baseline.
    #[must_use]
    pub const fn baseline(&self) -> Baseline {
        self.baseline
    }

    /// The vocabulary.
    #[must_use]
    pub fn vocabulary(&self) -> &Vocabulary {
        &self.vocabulary
    }

    /// The faction row.
    #[must_use]
    pub const fn faction(&self) -> usize {
        self.faction
    }

    /// Whether the card-text families are part of the input.
    #[must_use]
    pub const fn card_tags(&self) -> bool {
        self.card_tags
    }

    /// The seat's power summary, from the cache the power facts use.
    pub fn power_summary(
        &self,
        seen: &Observed<'_>,
        player: &PlayerId,
    ) -> crate::power_map::Summary {
        self.power_cache.borrow_mut().summary(seen, player)
    }

    /// The plan trace written since the last call: fleet decisions, planned answers and stopped
    /// plans.
    pub fn take_trace(&mut self) -> Vec<PlanTrace> {
        std::mem::take(&mut self.trace)
    }

    /// The features of a choice, or the answer of the seat's fleet plan.
    ///
    /// # Errors
    /// [`MlpError::EmptyLegalSet`], or [`MlpError::Features`] for features that cannot be scored.
    pub fn features<M: ModelInfo + ?Sized>(
        &mut self,
        model: &M,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<Features, MlpError> {
        if choice.options.is_empty() {
            return Err(MlpError::EmptyLegalSet);
        }
        if let Some(option) = self.planned(choice, seen) {
            return Ok(Features::Planned(option));
        }
        let held = seen.held_secret_progress();
        // The seat's own setup baseline goes in with the features: the opening-progress facts are
        // deltas against it.
        let vectors = self.project(seen, choice, &held);
        // An arena-capable model adds each movement option's battle facts.
        let vectors = match model.battle_predictor() {
            Some(predictor) => {
                let facts = crate::battle::decision_facts(
                    seen.observed(),
                    choice,
                    &choice.player,
                    predictor,
                );
                crate::battle::append_facts(vectors, &facts)
            }
            None => vectors,
        };
        // Deal values on diplomacy options, for a vocabulary that places them.
        let vectors = if self.deal_facts {
            let facts = crate::deal_value::deal_facts(seen.observed(), choice);
            crate::battle::append_facts(vectors, &facts)
        } else {
            vectors
        };
        // Power projection per option, for a vocabulary that places the names. The whole table's
        // projection is computed at the activation decision and reused for the moves of that
        // action, which see the board as it stood when the system was chosen.
        let vectors = if self.power_facts {
            let summary = self
                .power_cache
                .borrow_mut()
                .summary(seen.observed(), &choice.player);
            let needs_table = choice.options.iter().any(|option| {
                option.kind == ti4_engine::tactical::ACTIVATE_KIND
                    || option.kind == ti4_engine::tactical::MOVE_KIND
            });
            if needs_table && (seen.active_system().is_none() || self.last_table.is_none()) {
                self.last_table = Some(crate::power_facts::TablePower::of(seen.observed()));
            }
            let facts = crate::power_facts::decision_facts(
                seen.observed(),
                choice,
                &choice.player,
                &summary,
                if needs_table {
                    self.last_table.as_ref()
                } else {
                    None
                },
            );
            crate::battle::append_facts(vectors, &facts)
        } else {
            vectors
        };
        let (options, assigned, oov) = self.columns(&vectors)?;
        if options.len() != choice.options.len() {
            return Err(MlpError::Features(format!(
                "MLP projection produced {} vectors for {} legal options",
                options.len(),
                choice.options.len()
            )));
        }
        let requested_head = crate::learned::decision_head(choice);
        let layout = model.layout();
        let head = layout.resolve(requested_head);
        let head_index = layout.index(head).ok_or_else(|| {
            MlpError::Features(format!("resolved MLP head {head} is not in the layout"))
        })?;
        Ok(Features::Score(Prepared {
            faction: self.faction,
            requested_head,
            head,
            head_index,
            options,
            choices: choice.options.clone(),
            assigned,
            oov,
            stage: Stage::Choice,
        }))
    }

    /// The drawn option of a prepared decision: `softmax(logits / temperature)`, then [`draw`].
    ///
    /// # Errors
    /// As [`probabilities`] and [`Self::pick_with`].
    pub fn pick<R: Rng>(
        &mut self,
        prepared: &Prepared,
        logits: &[f32],
        temperature: f64,
        rng: &mut R,
    ) -> Result<Picked, MlpError> {
        let probabilities = probabilities(logits, temperature)?;
        self.pick_with(prepared, probabilities, rng)
    }

    /// The drawn option of a prepared decision from a distribution computed elsewhere (a batched
    /// scorer that returns probabilities). Picking an activation drops the seat's plan; picking a
    /// fleet makes it the plan and writes the trace.
    ///
    /// # Errors
    /// [`MlpError::NotUsable`] for a distribution of the wrong length or with a probability that
    /// is negative or not finite.
    pub fn pick_with<R: Rng>(
        &mut self,
        prepared: &Prepared,
        probabilities: Vec<f64>,
        rng: &mut R,
    ) -> Result<Picked, MlpError> {
        if probabilities.len() != prepared.choices.len()
            || probabilities
                .iter()
                .any(|probability| !probability.is_finite() || *probability < 0.0)
        {
            return Err(MlpError::NotUsable {
                what: "the distribution",
                detail: format!(
                    "{} probabilities for {} options",
                    probabilities.len(),
                    prepared.choices.len()
                ),
            });
        }
        let index = draw(&probabilities, rng);
        let option = prepared.choices[index].clone();
        let (label, declined) = match &prepared.stage {
            Stage::Choice => {
                if option.kind == ti4_engine::tactical::ACTIVATE_KIND {
                    self.plan = None;
                }
                (option.id.clone(), option.is_decline())
            }
            Stage::Fleet {
                player,
                system,
                menu,
                facts,
            } => {
                self.trace.push(PlanTrace::Package {
                    player: player.to_string(),
                    system: system.to_string(),
                    options: prepared
                        .choices
                        .iter()
                        .zip(&probabilities)
                        .zip(facts)
                        .map(|((option, p), facts)| {
                            (
                                option.label.clone(),
                                *p,
                                facts.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect(),
                            )
                        })
                        .collect(),
                    chosen: index,
                });
                let manual = index == menu.len();
                self.plan = (!manual).then(|| PlanSlot {
                    player: player.clone(),
                    system: system.clone(),
                    execution: Execution::new(menu[index].clone()),
                });
                if manual {
                    ("manual".to_owned(), true)
                } else {
                    (
                        menu[index].strategy.label().to_owned(),
                        menu[index].moves.is_empty(),
                    )
                }
            }
        };
        Ok(Picked {
            index,
            option,
            probabilities,
            label,
            declined,
        })
    }

    /// After an activation was picked: the fleet decision of fact version 7, when the model has a
    /// predictor that samples fleets and the menu is not empty. Its pick becomes the seat's plan,
    /// which answers the movement prompts of the activation ([`Features::Planned`]).
    ///
    /// # Errors
    /// [`MlpError::Features`] for features that cannot be scored.
    pub fn fleet<M: ModelInfo + ?Sized>(
        &mut self,
        model: &M,
        choice: &Choice,
        seen: &SeatObservation<'_>,
        picked: &Picked,
    ) -> Result<Option<Prepared>, MlpError> {
        if picked.option.kind != ti4_engine::tactical::ACTIVATE_KIND {
            return Ok(None);
        }
        self.plan = None;
        let system = SystemId::new(picked.option.id.clone());
        let Some(predictor) = model.battle_predictor() else {
            return Ok(None);
        };
        if !crate::battle::samples_packages(predictor.feature_version) {
            return Ok(None);
        }
        let menu = crate::tactical_plan::packages(
            seen.observed(),
            &choice.player,
            &system,
            Some(predictor),
        );
        if menu.is_empty() {
            return Ok(None);
        }
        // The menu, then building the fleet step by step.
        let mut options: Vec<ChoiceOption> = menu
            .iter()
            .enumerate()
            .map(|(index, package)| {
                ChoiceOption::labelled(
                    format!("package|{index}"),
                    PACKAGE_KIND,
                    format!(
                        "{}: {} ships, {} carried",
                        package.strategy.label(),
                        package.moves.len(),
                        package.loads.len()
                    ),
                )
            })
            .collect();
        options.push(ChoiceOption::labelled(
            "package|manual",
            PACKAGE_KIND,
            "build the fleet step by step",
        ));
        let synthetic = Choice::new(choice.player.clone(), "movement", options);
        let held = seen.held_secret_progress();
        let vectors = self.project(seen, &synthetic, &held);
        let facts: Vec<Vec<(&'static str, f64)>> = menu
            .iter()
            .map(|package| crate::battle::package_facts(Some(package)))
            .chain(std::iter::once(crate::battle::package_facts(None)))
            .collect();
        let vectors = crate::battle::append_facts(vectors, &facts);
        let (columns, assigned, oov) = self.columns(&vectors)?;
        let requested_head = crate::learned::decision_head(&synthetic);
        let layout = model.layout();
        let head = layout.resolve(requested_head);
        let head_index = layout.index(head).ok_or_else(|| {
            MlpError::Features(format!("package head {head} is not in the layout"))
        })?;
        Ok(Some(Prepared {
            faction: self.faction,
            requested_head,
            head,
            head_index,
            options: columns,
            choices: synthetic.options,
            assigned,
            oov,
            stage: Stage::Fleet {
                player: choice.player.clone(),
                system,
                menu,
                facts,
            },
        }))
    }

    /// One whole decision on a scorer: features, logits, the draw, and the fleet decision after
    /// an activation.
    ///
    /// # Errors
    /// Whatever stopped a step.
    pub fn decide<S: Scorer + ?Sized, R: Rng>(
        &mut self,
        scorer: &S,
        choice: &Choice,
        seen: &SeatObservation<'_>,
        temperature: f64,
        rng: &mut R,
    ) -> Result<ChoiceOption, MlpError> {
        let prepared = match self.features(scorer, choice, seen)? {
            Features::Planned(option) => return Ok(option),
            Features::Score(prepared) => prepared,
        };
        let logits = scorer.logits(prepared.faction, prepared.head_index, &prepared.options)?;
        let picked = self.pick(&prepared, &logits, temperature, rng)?;
        if let Some(fleet) = self.fleet(scorer, choice, seen, &picked)? {
            let logits = scorer.logits(fleet.faction, fleet.head_index, &fleet.options)?;
            self.pick(&fleet, &logits, temperature, rng)?;
        }
        Ok(picked.option)
    }

    /// Answer from the current plan when this prompt is the plan's; drop a plan that no longer
    /// applies. `None` hands the prompt to the model.
    fn planned(&mut self, choice: &Choice, seen: &SeatObservation<'_>) -> Option<ChoiceOption> {
        let slot = self.plan.as_mut()?;
        let starts_over = choice.options.iter().any(|option| {
            option.kind == ti4_engine::tactical::ACTIVATE_KIND
                || option.id == ti4_engine::game::TACTICAL_ACTION_ID
        });
        let elsewhere = seen.observed().active_system() != Some(&slot.system);
        if starts_over || (elsewhere && choice.player == slot.player) {
            self.plan = None;
            return None;
        }
        if choice.player != slot.player || !Execution::handles(choice) {
            return None;
        }
        let answer = slot.execution.answer(choice);
        let player = slot.player.to_string();
        match (&answer, &slot.execution.state) {
            (Some(id), state) => {
                let option = choice
                    .options
                    .iter()
                    .find(|option| &option.id == id)?
                    .clone();
                self.trace.push(PlanTrace::Planned {
                    player,
                    prompt: choice.prompt.clone(),
                    chosen: option.id.clone(),
                });
                if *state == PlanState::Complete {
                    self.plan = None;
                }
                Some(option)
            }
            (None, PlanState::NotOffered(reason)) => {
                self.trace.push(PlanTrace::Stopped {
                    player,
                    reason: reason.clone(),
                });
                self.plan = None;
                None
            }
            (None, _) => None,
        }
    }

    /// The MLP projection of a choice, with the card-text families when the vocabulary places
    /// them. The hands come from the bound seat's own observation, so no other seat's cards are
    /// reachable.
    fn project(
        &self,
        seen: &SeatObservation<'_>,
        choice: &Choice,
        held: &[ti4_engine::objectives::CardProgress],
    ) -> Vec<crate::features::FeatureVector> {
        if self.card_tags {
            let cards = seen.held_action_cards();
            let notes = seen.held_promissory_notes();
            let input = crate::projection::CardTagInput {
                held_action_cards: &cards,
                held_promissory_notes: &notes,
            };
            crate::projection::mlp_choice_features_with(
                seen.observed(),
                choice,
                &choice.player,
                held,
                self.baseline,
                Some(&input),
            )
        } else {
            crate::projection::mlp_choice_features(
                seen.observed(),
                choice,
                &choice.player,
                held,
                self.baseline,
            )
        }
    }

    /// Each option's feature vector as columns, and how many names found a column of their own
    /// and how many fell to an out-of-vocabulary one.
    ///
    /// A name with no column of its own is **not dropped**: it routes to its family's
    /// out-of-vocabulary column, or the global one, so an unknown option word stays
    /// distinguishable from its absence.
    fn columns(
        &mut self,
        vectors: &[crate::features::FeatureVector],
    ) -> Result<(Vec<Columns>, usize, usize), MlpError> {
        let (mut assigned_count, mut oov) = (0, 0);
        let mut options = Vec::with_capacity(vectors.len());
        for vector in vectors {
            let mut columns = Vec::with_capacity(vector.len());
            for (key, value) in vector {
                let (column, assigned) = match self.resolved.get(key) {
                    Some(hit) => *hit,
                    None => {
                        let answer = self.vocabulary.resolve_key(*key);
                        self.resolved.insert(*key, answer);
                        answer
                    }
                };
                if assigned {
                    assigned_count += 1;
                } else {
                    oov += 1;
                }
                let column = u32::try_from(column).map_err(|_| {
                    MlpError::Features(format!("feature column {column} does not fit u32"))
                })?;
                #[expect(clippy::cast_possible_truncation, reason = "features are f32-scale")]
                let value = *value as f32;
                if !value.is_finite() {
                    return Err(MlpError::Features(
                        "a projected feature is not finite f32".to_owned(),
                    ));
                }
                columns.push((column, value));
            }
            options.push(columns);
        }
        Ok((options, assigned_count, oov))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layout_is_known_by_its_heads_and_folds_what_it_lacks() {
        assert_eq!(
            HeadLayout::of(HeadLayout::Legacy.heads()),
            Some(HeadLayout::Legacy)
        );
        let diplomacy: Vec<String> = HeadLayout::Diplomacy
            .heads()
            .iter()
            .map(|head| (*head).to_owned())
            .collect();
        assert_eq!(HeadLayout::of(&diplomacy), Some(HeadLayout::Diplomacy));
        assert_eq!(HeadLayout::of(&diplomacy[1..]), None);
        assert_eq!(HeadLayout::Legacy.resolve("diplomacy"), "other");
        assert_eq!(HeadLayout::Diplomacy.resolve("diplomacy"), "diplomacy");
        assert_eq!(HeadLayout::Legacy.index("other"), Some(13));
        assert_eq!(HeadLayout::Diplomacy.index("diplomacy"), Some(14));
        assert_eq!(HeadLayout::Legacy.index("diplomacy"), None);
    }

    #[test]
    fn the_softmax_refuses_what_is_not_a_distribution() {
        let p = probabilities(&[1.0, 2.0, 3.0], 1.0).expect("finite");
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(p[2] > p[1] && p[1] > p[0]);
        // A large logit does not overflow: the max is subtracted first.
        assert!(probabilities(&[90.0, 0.0], 0.25).is_ok());
        assert_eq!(probabilities(&[], 1.0), Err(MlpError::EmptyLegalSet));
        assert_eq!(probabilities(&[1.0], 0.0), Err(MlpError::Temperature(0.0)));
        assert!(matches!(
            probabilities(&[f32::NAN, 1.0], 1.0),
            Err(MlpError::NotUsable { .. })
        ));
    }

    #[test]
    fn the_draw_is_the_cumulative_walk() {
        use rand::SeedableRng;
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(5);
        let mut again = rand_chacha::ChaCha8Rng::seed_from_u64(5);
        let p = [0.2, 0.3, 0.5];
        for _ in 0..50 {
            let index = draw(&p, &mut rng);
            let u: f64 = again.random_range(0.0..1.0);
            let expected = if u < 0.2 {
                0
            } else if u < 0.2 + 0.3 {
                1
            } else {
                2
            };
            assert_eq!(index, expected);
        }
        assert_eq!(draw(&[0.0, 1.0], &mut rng), 1);
    }

    #[test]
    fn the_roster_has_thirty_three_distinct_rows() {
        let mut sorted = FACTION_ROSTER.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 33);
        assert_eq!(faction_row("sol"), Some(27));
        assert_eq!(faction_row("nobody"), None);
    }
}
