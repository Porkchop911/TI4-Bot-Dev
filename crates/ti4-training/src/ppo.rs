//! Proximal policy optimisation over retained trajectories.
//!
//! The difference from [`crate::gradient`] is not the advantage -- it is that a batch is used
//! more than once. REINFORCE simulates 96 games, takes one gradient step, and throws the games
//! away; simulation is essentially all of this project's compute, so that one step is the entire
//! return on the expensive part. PPO keeps the batch and takes `epochs` steps from it, holding
//! the update trustworthy with an importance ratio and a clip rather than with a fresh sample.
//!
//! Two facts about this codebase make that cheap:
//!
//! * every option's feature vector is already recorded on the trajectory, so a later epoch
//!   re-reads features rather than re-extracting them, and re-simulates nothing;
//! * the behaviour policy's probability for every option is recorded too, so the ratio needs no
//!   separate bookkeeping.
//!
//! And one fact makes the baseline simpler here than in the general case: **returns do not depend
//! on the policy.** They are a function of the episode, which is fixed once the batch is played.
//! So the centring mean and the scale are computed once, before the first epoch, and every epoch
//! shares them. Only the ratio and the expectation `sum_o p_o phi_o` move.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use ti4_model::id::FactionId;

use ti4_policy::inference::TrajectoryStep;
use ti4_policy::intern::{FeatureKey, name_of};
use ti4_policy::learned::Profile;

use crate::gradient::{ROUND_BUCKET, Telemetry};
use crate::reward::{Episode, Reward, returns};
use crate::rollout::Rollout;

/// How PPO steps, and how far it trusts a re-used batch.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PpoStep {
    /// How much of the gradient to apply.
    pub learning_rate: f64,
    /// How much to pay for keeping the distribution spread out.
    pub entropy: f64,
    /// The largest gradient norm a single update may act on.
    pub gradient_clip: f64,
    /// The trust region: a decision whose ratio leaves `[1-clip, 1+clip]` in the direction that
    /// would improve the surrogate contributes nothing.
    pub clip: f64,
    /// How many gradient steps to take from one batch.
    pub epochs: usize,
    /// Extra entropy for one named head, on top of `entropy`.
    ///
    /// The strategy draft collapses: a converged policy takes one card 100% of the time, six
    /// factions partition the six cards they want, and no faction ever experiences being denied
    /// one. A per-episode reward cannot correct that, because within a single game a faction picks
    /// exactly one card -- "picked more than 75% of the time" is a property of the policy across
    /// games, not of any episode. Entropy is where it can live, and applying it to one head keeps
    /// the pressure off the heads that are supposed to become decisive.
    ///
    /// Preferred over a hard 75% threshold because a threshold is discontinuous -- 74% gets
    /// nothing and 76% gets a shove -- while entropy pushes in proportion to how concentrated the
    /// distribution already is.
    /// Applies to the `strategy` head alone; zero leaves it on the global coefficient.
    pub draft_entropy: f64,
    /// Reinforce what went better than the batch, and do not punish what went worse.
    ///
    /// The advantage is `return - batch mean`, so above-average decisions are already reinforced
    /// and below-average ones already discouraged. Clamping the negative half to zero turns the
    /// update into self-imitation: the policy is pulled toward its own better games and pushed
    /// away from nothing.
    ///
    /// Worth being explicit about what this cannot distinguish. A seat's return depends on its
    /// game as well as its play -- every seat that reached 6+ VP was in a game where the
    /// custodians came off, which is not a property of that seat's decisions -- so reinforcing on
    /// outcome also reinforces having been in a favourable game. Self-imitation is the mild
    /// version of that error; elite filtering, which discards the rest of the batch, is the
    /// severe one.
    pub positive_only: bool,
}

impl Default for PpoStep {
    fn default() -> Self {
        Self {
            learning_rate: 0.03,
            entropy: 0.01,
            gradient_clip: 1.0,
            clip: 0.2,
            epochs: 4,
            draft_entropy: 0.0,
            positive_only: false,
        }
    }
}

/// The first two moments of one bucket's returns.
///
/// Separated from the gradient because they are the part that is computed once: no features are
/// touched here, so the pre-pass costs a walk over the returns and nothing else.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Moments {
    /// Decisions in this bucket.
    pub actions: usize,
    /// Sum of their returns.
    pub sum: f64,
    /// Sum of the squares.
    pub square_sum: f64,
}

impl Moments {
    /// Add another partial's moments.
    pub fn merge(&mut self, other: Self) {
        self.actions += other.actions;
        self.sum += other.sum;
        self.square_sum += other.square_sum;
    }

    /// The centring mean and the scale the advantage is divided by.
    ///
    /// A bucket whose returns were all identical has nothing to say about which decision was
    /// better; its scale falls back to one so that rounding error does not become a gradient.
    #[must_use]
    pub fn baseline(self) -> (f64, f64) {
        if self.actions == 0 {
            return (0.0, 1.0);
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "an action count is far below 2^53"
        )]
        let divisor = self.actions as f64;
        let mean = self.sum / divisor;
        let variance = (self.square_sum / divisor - mean * mean).max(0.0);
        let scale = if variance > 1e-12 {
            variance.sqrt()
        } else {
            1.0
        };
        (mean, scale)
    }
}

/// One epoch's accumulation for a single head.
///
/// Unlike [`crate::gradient::Statistics`] this carries the gradient already weighted. REINFORCE
/// had to keep `sum A phi` and `sum phi` apart because the centring mean was not known until the
/// batch closed; here it is known before the epoch begins, so the weight is applied per decision
/// and one map suffices.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EpochStatistics {
    /// Decisions this head saw.
    pub actions: usize,
    /// Sum over decisions of `weight * (phi_chosen - sum_o p_o phi_o) / T`.
    pub gradient: BTreeMap<FeatureKey, f64>,
    /// The entropy bonus's gradient, under the *current* policy.
    pub entropy_gradient: BTreeMap<FeatureKey, f64>,
    /// Decisions whose ratio left the trust region and contributed no gradient.
    pub clipped: usize,
    /// Sum of the approximate KL from behaviour to current policy, for telemetry.
    pub kl_sum: f64,
    /// Sum of the current policy's entropies.
    pub entropy_sum: f64,
    /// Sum of the returns, for telemetry only -- the baseline comes from [`Moments`].
    pub return_sum: f64,
    /// Sum of their squares, so the reported return spread stays live under PPO.
    ///
    /// The trainer reads return standard deviation as its headline "is anything being learned"
    /// gauge: a head whose returns are all equal credits every decision alike and can move no
    /// weight. Reporting a constant zero here would look exactly like that failure.
    pub return_square_sum: f64,
}

impl EpochStatistics {
    /// Add another partial's accumulation.
    pub fn merge(&mut self, other: &Self) {
        self.actions += other.actions;
        self.clipped += other.clipped;
        self.kl_sum += other.kl_sum;
        self.entropy_sum += other.entropy_sum;
        self.return_sum += other.return_sum;
        self.return_square_sum += other.return_square_sum;
        for (target, source) in [
            (&mut self.gradient, &other.gradient),
            (&mut self.entropy_gradient, &other.entropy_gradient),
        ] {
            for (slot, value) in source {
                accumulate(target, *slot, *value);
            }
        }
    }
}

/// What one PPO epoch did, beyond what [`Telemetry`] reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ClipTelemetry {
    /// Fraction of decisions whose ratio left the trust region.
    ///
    /// The number to watch. At zero the clip is doing nothing and the epochs are unconstrained;
    /// approaching one the batch has been exhausted and further epochs are wasted work.
    pub clip_fraction: f64,
    /// Mean approximate KL from the behaviour policy to the current one.
    pub kl_mean: f64,
}

/// Add into an accumulator without normalising negative zero.
///
/// `entry(..).or_insert(0.0) += value` turns `-0.0` into `+0.0`; a plain insert does not. The
/// difference is invisible to every test and visible to a bit-level digest, which is exactly the
/// kind of divergence a parity gate exists to catch.
fn accumulate(into: &mut BTreeMap<FeatureKey, f64>, slot: FeatureKey, value: f64) {
    if let Some(existing) = into.get_mut(&slot) {
        *existing += value;
    } else {
        into.insert(slot, 0.0 + value);
    }
}

/// The bucket key a decision's baseline is looked up under.
fn bucket_of(step: &TrajectoryStep, reward: &Reward) -> String {
    if reward.round_baseline {
        format!("{}{ROUND_BUCKET}{}", step.head, step.progress.round_number)
    } else {
        step.head.clone()
    }
}

/// The return moments one seat's episode contributes, per bucket.
///
/// Touches no features: this is the cheap pre-pass whose result every epoch then shares.
#[must_use]
pub fn moments(
    trajectory: &[TrajectoryStep],
    episode: &Episode,
    reward: &Reward,
) -> BTreeMap<String, Moments> {
    let credited = returns(episode, reward);
    let mut collected: BTreeMap<String, Moments> = BTreeMap::new();
    for (step, credit) in trajectory.iter().zip(&credited) {
        let row = collected.entry(bucket_of(step, reward)).or_default();
        row.actions += 1;
        row.sum += credit;
        row.square_sum += credit * credit;
    }
    collected
}

/// Score every legal option of a step under the current weights, in `legal` order.
///
/// Returns probabilities positionally aligned with `step.legal`, which is the same order as
/// `step.probabilities` -- the recorded behaviour policy. Deliberately a `Vec` rather than the
/// `BTreeMap<String, f64>` inference builds: this runs once per option per decision per epoch,
/// and cloning an option id every time would make the epochs cost more than the simulation they
/// exist to amortise.
fn current_probabilities(step: &TrajectoryStep, profile: &Profile, temperature: f64) -> Vec<f64> {
    let options = step.legal.len();
    if options == 0 {
        return Vec::new();
    }
    if options == 1 {
        return vec![1.0];
    }
    let head = profile.resolved_head(&step.head).to_owned();
    let scores: Vec<f64> = step
        .legal
        .values()
        .map(|vector| profile.score_vector(&head, vector))
        .collect();
    let best = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let weights: Vec<f64> = scores
        .iter()
        .map(|score| ((score - best) / temperature).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        #[expect(clippy::cast_precision_loss, reason = "option counts are small")]
        let share = 1.0 / options as f64;
        return vec![share; options];
    }
    weights.into_iter().map(|weight| weight / total).collect()
}

/// One epoch's statistics for one seat's episode.
///
/// `baselines` is keyed by bucket and comes from [`moments`]; it is the same for every epoch.
#[must_use]
pub fn epoch_statistics(
    trajectory: &[TrajectoryStep],
    episode: &Episode,
    profile: &Profile,
    reward: &Reward,
    baselines: &BTreeMap<String, (f64, f64)>,
    clip: f64,
    positive_only: bool,
) -> BTreeMap<String, EpochStatistics> {
    let credited = returns(episode, reward);
    let mut collected: BTreeMap<String, EpochStatistics> = BTreeMap::new();

    for (step, credit) in trajectory.iter().zip(&credited) {
        let temperature = profile
            .head(&step.head)
            .map_or(1.0, |head| head.temperature)
            .max(1e-6);
        let (mean, scale) = baselines
            .get(&bucket_of(step, reward))
            .copied()
            .unwrap_or((0.0, 1.0));
        let mut advantage = (credit - mean) / scale;
        if positive_only {
            advantage = advantage.max(0.0);
        }

        let current = current_probabilities(step, profile, temperature);
        if current.len() != step.legal.len() {
            continue;
        }

        // The ratio is over the option actually taken. `legal` and `probabilities` share their
        // ordering, so the chosen option's index is found once and indexes both.
        let Some(index) = step.legal.keys().position(|option| option == &step.chosen) else {
            continue;
        };
        let behaviour = step
            .probabilities
            .values()
            .nth(index)
            .copied()
            .unwrap_or(0.0);
        let ratio = if behaviour > 1e-12 {
            current[index] / behaviour
        } else {
            1.0
        };

        // The clipped surrogate is `min(rA, clip(r)A)`. Where the clip binds, the objective is
        // flat in the weights and the decision contributes no gradient at all; elsewhere the
        // gradient is the ordinary one scaled by the ratio.
        let outside =
            (advantage > 0.0 && ratio > 1.0 + clip) || (advantage < 0.0 && ratio < 1.0 - clip);
        let weight = if outside { 0.0 } else { ratio * advantage };

        if !collected.contains_key(&step.head) {
            collected.insert(step.head.clone(), EpochStatistics::default());
        }
        let Some(row) = collected.get_mut(&step.head) else {
            continue; // unreachable: inserted immediately above
        };

        let entropy: f64 = -current
            .iter()
            .map(|chance| chance * chance.max(1e-12).ln())
            .sum::<f64>();
        row.actions += 1;
        row.return_sum += credit;
        row.return_square_sum += credit * credit;
        row.entropy_sum += entropy;
        row.kl_sum += (ratio - 1.0) - ratio.max(1e-12).ln();
        if outside {
            row.clipped += 1;
        }

        // The entropy bonus is a property of the current distribution and is taken every epoch,
        // clipped or not: the clip bounds how far the *policy* is trusted to have moved, and says
        // nothing about keeping it spread out.
        for (chance, vector) in current.iter().zip(step.legal.values()) {
            let coefficient = -chance * (chance.max(1e-12).ln() + entropy) / temperature;
            for (slot, value) in vector {
                accumulate(&mut row.entropy_gradient, *slot, coefficient * value);
            }
        }

        if weight == 0.0 {
            continue;
        }

        let mut expected: BTreeMap<FeatureKey, f64> = BTreeMap::new();
        for (chance, vector) in current.iter().zip(step.legal.values()) {
            for (slot, value) in vector {
                *expected.entry(*slot).or_insert(0.0) += chance * value;
            }
        }
        let slots: BTreeSet<FeatureKey> = expected
            .keys()
            .copied()
            .chain(step.features().keys().copied())
            .collect();
        for slot in slots {
            let difference = (step.features().get(&slot).copied().unwrap_or(0.0)
                - expected.get(&slot).copied().unwrap_or(0.0))
                / temperature;
            accumulate(&mut row.gradient, slot, weight * difference);
        }
    }
    collected
}

/// Apply one epoch's accumulated gradient to a profile's heads.
///
/// The shape mirrors [`crate::gradient::apply`] -- mean over decisions, one norm clip per head,
/// one write per slot -- so that a PPO run and a REINFORCE run differ in what the gradient is and
/// in nothing else about how it lands.
pub fn apply(
    profile: &mut Profile,
    statistics: &BTreeMap<String, EpochStatistics>,
    step: PpoStep,
) -> BTreeMap<String, (Telemetry, ClipTelemetry)> {
    let mut told = BTreeMap::new();
    for (head, row) in statistics {
        if row.actions == 0 {
            continue;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "an action count is far below 2^53"
        )]
        let divisor = row.actions as f64;

        let slots: BTreeSet<FeatureKey> = row
            .gradient
            .keys()
            .chain(row.entropy_gradient.keys())
            .copied()
            .collect();
        let combined: BTreeMap<FeatureKey, f64> = slots
            .into_iter()
            .map(|slot| {
                let entropy = step.entropy
                    + if head == "strategy" {
                        step.draft_entropy
                    } else {
                        0.0
                    };
                let value = row.gradient.get(&slot).copied().unwrap_or(0.0)
                    + entropy * row.entropy_gradient.get(&slot).copied().unwrap_or(0.0);
                (slot, value)
            })
            .collect();

        let norm = combined
            .values()
            .map(|value| (value / divisor).powi(2))
            .sum::<f64>()
            .sqrt();
        let shrink = if norm > 0.0 {
            (step.gradient_clip / norm).min(1.0)
        } else {
            1.0
        };

        let mut squared = 0.0;
        if let Some(weights) = profile.head_mut(head) {
            for (slot, value) in &combined {
                let delta = step.learning_rate * shrink * value / divisor;
                if delta.abs() > 1e-15 && delta.is_finite() {
                    *weights.weights.entry(name_of(*slot)).or_insert(0.0) += delta;
                    squared += delta * delta;
                }
            }
            weights.invalidate();
        }

        #[expect(
            clippy::cast_precision_loss,
            reason = "a clipped count is far below 2^53"
        )]
        let clipped = row.clipped as f64;
        let return_mean = row.return_sum / divisor;
        let return_variance =
            (row.return_square_sum / divisor - return_mean * return_mean).max(0.0);
        told.insert(
            head.clone(),
            (
                Telemetry {
                    actions: row.actions,
                    return_mean,
                    return_std: return_variance.sqrt(),
                    entropy_mean: row.entropy_sum / divisor,
                    gradient_norm: norm,
                    update_norm: squared.sqrt(),
                },
                ClipTelemetry {
                    clip_fraction: clipped / divisor,
                    kl_mean: row.kl_sum / divisor,
                },
            ),
        );
    }
    told
}

/// Run `epochs` PPO steps over one retained batch, updating every faction's profile in place.
///
/// The loop the trainer inverts to: roll out once, then `{statistics, apply} x epochs`. The
/// rollouts are borrowed throughout and re-read, never re-simulated.
pub fn update(
    profiles: &mut BTreeMap<FactionId, Profile>,
    rollouts: &[Rollout],
    reward: &Reward,
    step: PpoStep,
) -> Vec<BTreeMap<FactionId, BTreeMap<String, (Telemetry, ClipTelemetry)>>> {
    // Pre-pass: returns are a function of the episode, so this is computed once and every epoch
    // centres against it. Recomputing it per epoch would let the baseline drift with the policy
    // and quietly turn the advantage into something that is not an advantage.
    let mut totals: BTreeMap<FactionId, BTreeMap<String, Moments>> = BTreeMap::new();
    for rollout in rollouts {
        if rollout.error.is_some() {
            continue;
        }
        for seat in &rollout.seats {
            let per_head = totals.entry(seat.faction.clone()).or_default();
            for (bucket, row) in moments(&seat.trajectory, &seat.episode, reward) {
                per_head.entry(bucket).or_default().merge(row);
            }
        }
    }
    let baselines: BTreeMap<FactionId, BTreeMap<String, (f64, f64)>> = totals
        .into_iter()
        .map(|(faction, heads)| {
            (
                faction,
                heads
                    .into_iter()
                    .map(|(bucket, row)| (bucket, row.baseline()))
                    .collect(),
            )
        })
        .collect();

    let mut reported = Vec::with_capacity(step.epochs);
    for _ in 0..step.epochs {
        let partials: Vec<BTreeMap<FactionId, BTreeMap<String, EpochStatistics>>> = rollouts
            .par_iter()
            .map(|rollout| {
                let mut per_faction: BTreeMap<FactionId, BTreeMap<String, EpochStatistics>> =
                    BTreeMap::new();
                if rollout.error.is_some() {
                    return per_faction;
                }
                for seat in &rollout.seats {
                    let (Some(profile), Some(bases)) =
                        (profiles.get(&seat.faction), baselines.get(&seat.faction))
                    else {
                        continue;
                    };
                    let rows = epoch_statistics(
                        &seat.trajectory,
                        &seat.episode,
                        profile,
                        reward,
                        bases,
                        step.clip,
                        step.positive_only,
                    );
                    let target = per_faction.entry(seat.faction.clone()).or_default();
                    for (head, row) in rows {
                        target.entry(head).or_default().merge(&row);
                    }
                }
                per_faction
            })
            .collect();

        let mut merged: BTreeMap<FactionId, BTreeMap<String, EpochStatistics>> = BTreeMap::new();
        for partial in &partials {
            for (faction, heads) in partial {
                let target = merged.entry(faction.clone()).or_default();
                for (head, row) in heads {
                    target.entry(head.clone()).or_default().merge(row);
                }
            }
        }

        let mut epoch_report = BTreeMap::new();
        for (faction, heads) in &merged {
            if let Some(profile) = profiles.get_mut(faction) {
                epoch_report.insert(faction.clone(), apply(profile, heads, step));
            }
        }
        reported.push(epoch_report);
    }
    reported
}
