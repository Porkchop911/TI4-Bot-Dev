//! One centered REINFORCE update, normalised per seat and head (M10-013, M10-014).
//!
//! Ported from the oracle's `gradient_statistics` and `update_profiles_from_statistics`.
//!
//! # What the update is
//!
//! For each decision the policy took, the gradient of its log-probability with respect to the
//! weights is `(features(chosen) − E_p[features]) / temperature`. Scaled by how good the outcome
//! was and summed, that is REINFORCE. Three things make it usable rather than merely correct:
//!
//! - **Centering.** Returns are measured against their own mean, so a decision is credited for
//!   being better than the seat's average rather than for being positive. Without it, every
//!   decision in a good game is reinforced — including the bad ones — because the whole game
//!   scored well.
//! - **Normalising per `(seat, head)`.** A head that fires on every turn and one that fires twice
//!   a game have wildly different return scales; one shared normaliser lets the loud head set the
//!   step size for the quiet one.
//! - **Clipping the gradient norm.** One rare, enormous return would otherwise move the weights
//!   far enough that the policy that produced the data no longer resembles the one being updated,
//!   and the next batch is drawn from something the update never saw.
//!
//! # Entropy
//!
//! A small bonus on the distribution's entropy, because a policy that collapses onto one option
//! stops producing the variety a policy gradient needs and cannot recover — every subsequent batch
//! confirms what it already believes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::features::FeatureVector;
use ti4_policy::inference::TrajectoryStep;
use ti4_policy::learned::Profile;

use crate::reward::{Episode, Reward, returns};

/// How large a step to take, and how far to let it go.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// How much of the gradient to apply.
    pub learning_rate: f64,
    /// How much to pay for keeping the distribution spread out.
    pub entropy: f64,
    /// The largest gradient norm a single update may act on.
    pub gradient_clip: f64,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            learning_rate: 0.03,
            entropy: 0.01,
            gradient_clip: 1.0,
        }
    }
}

/// Sufficient statistics for one `(seat, head)` pair.
///
/// Kept as sums rather than as the decisions themselves, so batches computed on separate threads
/// or machines can be added together before a single update is applied.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Statistics {
    /// How many decisions this pair saw.
    pub actions: usize,
    /// The sum of their returns.
    pub return_sum: f64,
    /// The sum of the squares, for the variance the centering divides by.
    pub return_square_sum: f64,
    /// The sum of the distributions' entropies, for telemetry.
    pub entropy_sum: f64,
    /// Σ of `(chosen − expected) / temperature`, per bucket.
    pub feature_difference_sum: BTreeMap<String, f64>,
    /// The same, weighted by each decision's return.
    pub return_feature_difference_sum: BTreeMap<String, f64>,
    /// The gradient of the entropy bonus, per bucket.
    pub entropy_gradient_sum: BTreeMap<String, f64>,
}

impl Statistics {
    /// Add another batch's statistics into this one.
    pub fn merge(&mut self, other: &Self) {
        self.actions += other.actions;
        self.return_sum += other.return_sum;
        self.return_square_sum += other.return_square_sum;
        self.entropy_sum += other.entropy_sum;
        for (target, source) in [
            (
                &mut self.feature_difference_sum,
                &other.feature_difference_sum,
            ),
            (
                &mut self.return_feature_difference_sum,
                &other.return_feature_difference_sum,
            ),
            (&mut self.entropy_gradient_sum, &other.entropy_gradient_sum),
        ] {
            for (slot, value) in source {
                *target.entry(slot.clone()).or_insert(0.0) += value;
            }
        }
    }
}

/// What one update did, per head. Reported rather than inferred, because a training run that
/// silently learns nothing looks exactly like one that is still early.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Telemetry {
    /// Decisions this head saw.
    pub actions: usize,
    /// Mean return.
    pub return_mean: f64,
    /// Its standard deviation. Zero means every decision was credited alike and nothing was
    /// learned from them, however many there were.
    pub return_std: f64,
    /// Mean entropy of the distributions.
    pub entropy_mean: f64,
    /// The gradient's norm before clipping.
    pub gradient_norm: f64,
    /// How far the weights actually moved.
    pub update_norm: f64,
}

/// Collect the statistics one seat's episode contributes.
#[must_use]
pub fn statistics(
    trajectory: &[TrajectoryStep],
    episode: &Episode,
    profile: &Profile,
    reward: &Reward,
) -> BTreeMap<String, Statistics> {
    let credited = returns(episode, reward);
    let mut collected: BTreeMap<String, Statistics> = BTreeMap::new();

    for (step, credit) in trajectory.iter().zip(&credited) {
        let temperature = profile
            .head(&step.head)
            .map_or(1.0, |head| head.temperature)
            .max(1e-6);
        let row = collected.entry(step.head.clone()).or_default();

        let entropy: f64 = -step
            .probabilities
            .values()
            .map(|chance| chance * chance.max(1e-12).ln())
            .sum::<f64>();

        // What the policy expected to see, over the options it might have taken.
        let mut expected: FeatureVector = BTreeMap::new();
        for (option, chance) in &step.probabilities {
            let Some(vector) = step.legal.get(option) else {
                continue;
            };
            for (slot, value) in vector {
                *expected.entry(slot.clone()).or_insert(0.0) += chance * value;
            }
        }

        row.actions += 1;
        row.return_sum += credit;
        row.return_square_sum += credit * credit;
        row.entropy_sum += entropy;

        let slots: BTreeSet<&String> = expected.keys().chain(step.features().keys()).collect();
        for slot in slots {
            let difference = (step.features().get(slot).copied().unwrap_or(0.0)
                - expected.get(slot).copied().unwrap_or(0.0))
                / temperature;
            *row.feature_difference_sum
                .entry(slot.clone())
                .or_insert(0.0) += difference;
            *row.return_feature_difference_sum
                .entry(slot.clone())
                .or_insert(0.0) += credit * difference;
        }

        for (option, chance) in &step.probabilities {
            let Some(vector) = step.legal.get(option) else {
                continue;
            };
            let coefficient = -chance * (chance.max(1e-12).ln() + entropy) / temperature;
            for (slot, value) in vector {
                *row.entropy_gradient_sum.entry(slot.clone()).or_insert(0.0) += coefficient * value;
            }
        }
    }
    collected
}

/// Apply the centered gradient to a profile's heads.
///
/// Returns what each head's update did. A head with no statistics is left alone rather than
/// zeroed: no data is not the same as evidence that its weights are wrong.
pub fn apply(
    profile: &mut Profile,
    statistics: &BTreeMap<String, Statistics>,
    step: Step,
) -> BTreeMap<String, Telemetry> {
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
        let mean = row.return_sum / divisor;
        let variance = (row.return_square_sum / divisor - mean * mean).max(0.0);
        // A batch where every return was identical has nothing to say about which decision was
        // better. Dividing by its (zero) spread would turn rounding error into a gradient, so the
        // scale falls back to one and the centered returns are all zero — no update, correctly.
        let scale = if variance > 1e-12 {
            variance.sqrt()
        } else {
            1.0
        };

        let slots: BTreeSet<&String> = row
            .feature_difference_sum
            .keys()
            .chain(row.return_feature_difference_sum.keys())
            .chain(row.entropy_gradient_sum.keys())
            .collect();
        let gradient: BTreeMap<String, f64> = slots
            .into_iter()
            .map(|slot| {
                let centered = (row
                    .return_feature_difference_sum
                    .get(slot)
                    .copied()
                    .unwrap_or(0.0)
                    - mean * row.feature_difference_sum.get(slot).copied().unwrap_or(0.0))
                    / scale;
                let bonus =
                    step.entropy * row.entropy_gradient_sum.get(slot).copied().unwrap_or(0.0);
                (slot.clone(), centered + bonus)
            })
            .collect();

        let norm = gradient
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
            for (slot, value) in &gradient {
                let delta = step.learning_rate * shrink * value / divisor;
                if delta.abs() > 1e-15 && delta.is_finite() {
                    *weights.weights.entry(slot.clone()).or_insert(0.0) += delta;
                    squared += delta * delta;
                }
            }
        }

        told.insert(
            head.clone(),
            Telemetry {
                actions: row.actions,
                return_mean: mean,
                return_std: variance.sqrt(),
                entropy_mean: row.entropy_sum / divisor,
                gradient_norm: norm,
                update_norm: squared.sqrt(),
            },
        );
    }
    told
}

/// Statistics for every seat in a batch of rollouts, keyed by seat.
#[must_use]
pub fn batch_statistics(
    rollouts: &[crate::rollout::Rollout],
    profiles: &BTreeMap<PlayerId, Profile>,
    reward: &Reward,
) -> BTreeMap<PlayerId, BTreeMap<String, Statistics>> {
    let mut collected: BTreeMap<PlayerId, BTreeMap<String, Statistics>> = BTreeMap::new();
    for rollout in rollouts {
        if rollout.error.is_some() {
            continue; // counted by the caller; a failed game has nothing to teach
        }
        for seat in &rollout.seats {
            let Some(profile) = profiles.get(&seat.player) else {
                continue;
            };
            let rows = statistics(&seat.trajectory, &seat.episode, profile, reward);
            let target = collected.entry(seat.player.clone()).or_default();
            for (head, row) in rows {
                target.entry(head).or_default().merge(&row);
            }
        }
    }
    collected
}

/// Statistics from a rotated panel, keyed by faction rather than physical seat.
///
/// A fixed-seat accumulator silently teaches the Letnev profile from whichever faction happened
/// to occupy its original chair after rotation. Keeping the key in the rollout makes that wiring
/// error observable and lets each policy follow its faction around the table.
#[must_use]
pub fn faction_batch_statistics(
    rollouts: &[crate::rollout::Rollout],
    profiles: &BTreeMap<FactionId, Profile>,
    reward: &Reward,
) -> BTreeMap<FactionId, BTreeMap<String, Statistics>> {
    let mut collected: BTreeMap<FactionId, BTreeMap<String, Statistics>> = BTreeMap::new();
    for rollout in rollouts {
        if rollout.error.is_some() {
            continue;
        }
        for seat in &rollout.seats {
            let Some(profile) = profiles.get(&seat.faction) else {
                continue;
            };
            let rows = statistics(&seat.trajectory, &seat.episode, profile, reward);
            let target = collected.entry(seat.faction.clone()).or_default();
            for (head, row) in rows {
                target.entry(head).or_default().merge(&row);
            }
        }
    }
    collected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reward::Stage;
    use ti4_policy::learned::{DEFAULT_DIMENSIONS, blank_profile, bucket};
    use ti4_policy::progress::Progress;

    fn vector(pairs: &[(&str, f64)]) -> FeatureVector {
        pairs
            .iter()
            .map(|(slot, value)| ((*slot).to_owned(), *value))
            .collect()
    }

    /// One decision between two options, with the given chances and the first one taken.
    fn step(head: &str, chosen: &str, chances: &[(&str, f64)], round: u32) -> TrajectoryStep {
        let legal: BTreeMap<String, FeatureVector> = chances
            .iter()
            .map(|(id, _)| {
                let (slot, sign) = bucket(&format!("option:{id}"), DEFAULT_DIMENSIONS);
                ((*id).to_owned(), vector(&[(&slot, sign)]))
            })
            .collect();
        TrajectoryStep {
            player: PlayerId::new("a"),
            head: head.to_owned(),
            chosen: chosen.to_owned(),
            legal,
            probabilities: chances
                .iter()
                .map(|(id, chance)| ((*id).to_owned(), *chance))
                .collect(),
            progress: Progress {
                round_number: round,
                ..Progress::default()
            },
        }
    }

    fn episode(steps: usize, gains: &[i64]) -> Episode {
        Episode {
            steps: gains
                .iter()
                .take(steps)
                .map(|gain| Progress {
                    planets_gained: *gain,
                    systems: 1,
                    round_number: 1,
                    ..Progress::default()
                })
                .collect(),
            final_progress: Progress {
                planets_gained: gains.last().copied().unwrap_or(0),
                systems: 1,
                round_number: 1,
                ..Progress::default()
            },
            cleared: false,
            shortfall: 3.0,
            traded_goods: 0.0,
        }
    }

    #[test]
    fn a_decision_that_did_better_than_average_is_reinforced() {
        // The property the whole update exists for. Two decisions, the first followed by a gain
        // and the second by nothing: the first option's weight must rise.
        let profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let trajectory = vec![
            step("activation", "good", &[("good", 0.5), ("bad", 0.5)], 1),
            step("activation", "bad", &[("good", 0.5), ("bad", 0.5)], 1),
        ];
        let played = episode(2, &[0, 2]);

        let rows = statistics(
            &trajectory,
            &played,
            &profile,
            &Reward::for_stage(Stage::One),
        );
        let mut updated = profile.clone();
        apply(&mut updated, &rows, Step::default());

        let (slot, sign) = bucket("option:good", DEFAULT_DIMENSIONS);
        let moved = updated.head("activation").unwrap().weights[&slot];
        assert!(
            moved * sign > 0.0,
            "the option that preceded the gain was not reinforced: {moved}"
        );
    }

    #[test]
    fn a_batch_where_every_return_is_identical_moves_nothing() {
        // Measured over 96 seat-games, 82 of them look exactly like this from blank weights. The
        // update must be zero rather than rounding error amplified by a near-zero variance.
        let profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let trajectory = vec![
            step("activation", "one", &[("one", 0.5), ("two", 0.5)], 1),
            step("activation", "two", &[("one", 0.5), ("two", 0.5)], 1),
        ];
        let flat = episode(2, &[0, 0]);

        let rows = statistics(&trajectory, &flat, &profile, &Reward::for_stage(Stage::One));
        let mut updated = profile.clone();
        let told = apply(
            &mut updated,
            &rows,
            Step {
                entropy: 0.0,
                ..Step::default()
            },
        );

        assert!(told["activation"].return_std.abs() < 1e-12);
        assert!(
            told["activation"].update_norm.abs() < 1e-12,
            "weights moved on a batch with nothing to learn from"
        );
    }

    #[test]
    fn one_head_is_updated_without_disturbing_another() {
        // Why the heads exist. A shared weight vector would have movement's update land on the
        // weights that decide votes, and neither would converge.
        let profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let trajectory = vec![
            step("movement", "good", &[("good", 0.5), ("bad", 0.5)], 1),
            step("movement", "bad", &[("good", 0.5), ("bad", 0.5)], 1),
        ];
        let played = episode(2, &[0, 2]);

        let rows = statistics(
            &trajectory,
            &played,
            &profile,
            &Reward::for_stage(Stage::One),
        );
        let mut updated = profile.clone();
        apply(&mut updated, &rows, Step::default());

        assert_ne!(
            updated.head("movement").unwrap().weights,
            profile.head("movement").unwrap().weights,
            "the head that saw the decisions moved"
        );
        assert_eq!(
            updated.head("agenda").unwrap().weights,
            profile.head("agenda").unwrap().weights,
            "a head that saw none did not"
        );
    }

    #[test]
    fn a_below_average_decision_is_pushed_down_even_when_its_return_was_positive() {
        // What centering is for, and the first version of this test did not check it. Both
        // decisions here are followed by gains, so *both* returns are positive: without centering
        // both options are reinforced, including the worse one, simply because the game went well.
        // The question a policy gradient answers is "better than what this seat usually does", not
        // "was the outcome positive".
        //
        // The two decisions offer disjoint options on purpose. Sharing them lets each step
        // contribute to the other's buckets through the expectation, which masked the difference
        // between centering and not centering entirely.
        let profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let trajectory = vec![
            step(
                "activation",
                "early",
                &[("early", 0.5), ("early_other", 0.5)],
                1,
            ),
            step(
                "activation",
                "late",
                &[("late", 0.5), ("late_other", 0.5)],
                1,
            ),
        ];
        // Potentials 2, 4, 6 → rewards 2, 2 → returns 4, 2, whose mean is 3. Both positive, and
        // one either side of the mean.
        let played = Episode {
            steps: vec![
                Progress {
                    planets_gained: 0,
                    systems: 1,
                    round_number: 1,
                    ..Progress::default()
                },
                Progress {
                    planets_gained: 1,
                    systems: 1,
                    round_number: 1,
                    ..Progress::default()
                },
            ],
            final_progress: Progress {
                planets_gained: 2,
                systems: 1,
                round_number: 1,
                ..Progress::default()
            },
            cleared: false,
            shortfall: 1.0,
            traded_goods: 0.0,
        };

        let reward = Reward::for_stage(Stage::One);
        let credited = returns(&played, &reward);
        assert!(
            credited.iter().all(|value| *value > 0.0),
            "both returns must be positive or the test proves nothing: {credited:?}"
        );

        let rows = statistics(&trajectory, &played, &profile, &reward);
        let mut updated = profile.clone();
        apply(
            &mut updated,
            &rows,
            Step {
                entropy: 0.0,
                ..Step::default()
            },
        );

        let weights = &updated.head("activation").unwrap().weights;
        let (above, above_sign) = bucket("option:early", DEFAULT_DIMENSIONS);
        let (below, below_sign) = bucket("option:late", DEFAULT_DIMENSIONS);
        assert!(
            weights[&above] * above_sign > 0.0,
            "the above-average decision was not reinforced"
        );
        assert!(
            weights[&below] * below_sign < 0.0,
            "the below-average decision was reinforced too, so nothing was centered"
        );
    }

    #[test]
    fn a_decision_the_policy_was_already_certain_of_teaches_nothing() {
        // The expectation the chosen features are measured against. With one option at probability
        // one, the chosen features *are* the expected features, so the difference is zero and there
        // is nothing to learn — the policy did not choose, it had no choice. Dropping the
        // expectation would credit that decision with its whole feature vector and teach the policy
        // to keep doing what it could not avoid.
        let profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let certain = vec![step("activation", "only", &[("only", 1.0)], 1)];
        let played = episode(1, &[3]);

        let rows = statistics(&certain, &played, &profile, &Reward::for_stage(Stage::One));
        for value in rows["activation"].feature_difference_sum.values() {
            assert!(
                value.abs() < 1e-9,
                "a forced decision produced a gradient of {value}"
            );
        }
        for value in rows["activation"].return_feature_difference_sum.values() {
            assert!(value.abs() < 1e-9);
        }
    }

    #[test]
    fn an_unlikely_choice_teaches_more_than_a_likely_one() {
        // The other half of the same rule: the further a decision is from what the policy expected
        // to do, the more it says about the weights.
        let profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let reward = Reward::for_stage(Stage::One);
        let played = episode(1, &[3]);

        let expected_choice = statistics(
            &[step("activation", "a", &[("a", 0.9), ("b", 0.1)], 1)],
            &played,
            &profile,
            &reward,
        );
        let surprise = statistics(
            &[step("activation", "b", &[("a", 0.9), ("b", 0.1)], 1)],
            &played,
            &profile,
            &reward,
        );

        let magnitude = |rows: &BTreeMap<String, Statistics>| {
            rows["activation"]
                .feature_difference_sum
                .values()
                .map(|value| value.abs())
                .sum::<f64>()
        };
        assert!(
            magnitude(&surprise) > magnitude(&expected_choice),
            "the surprising choice taught no more than the expected one"
        );
    }

    #[test]
    fn a_head_with_no_statistics_is_left_alone() {
        // No data is not evidence that a head's weights are wrong.
        let mut profile = blank_profile("sol", 16);
        let before = profile.clone();
        let told = apply(&mut profile, &BTreeMap::new(), Step::default());
        assert!(told.is_empty());
        assert_eq!(profile, before);
    }

    #[test]
    fn the_gradient_norm_is_clipped() {
        // One rare enormous return would otherwise move the weights far enough that the policy
        // which produced the data no longer resembles the one being updated.
        let profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let trajectory = vec![
            step("activation", "good", &[("good", 0.5), ("bad", 0.5)], 1),
            step("activation", "bad", &[("good", 0.5), ("bad", 0.5)], 1),
        ];
        let huge = episode(2, &[0, 1_000]);
        let rows = statistics(&trajectory, &huge, &profile, &Reward::for_stage(Stage::One));

        let mut tight = profile.clone();
        let told_tight = apply(
            &mut tight,
            &rows,
            Step {
                gradient_clip: 0.001,
                ..Step::default()
            },
        );
        let mut loose = profile.clone();
        let told_loose = apply(
            &mut loose,
            &rows,
            Step {
                gradient_clip: 100.0,
                ..Step::default()
            },
        );

        assert!(
            told_tight["activation"].update_norm < told_loose["activation"].update_norm,
            "clipping did not bound the step"
        );
    }

    #[test]
    fn statistics_from_two_batches_add_up_to_one_batch() {
        // The property that lets rollouts be computed on separate threads and reduced afterwards.
        let profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let reward = Reward::for_stage(Stage::One);
        let trajectory = vec![step(
            "activation",
            "good",
            &[("good", 0.5), ("bad", 0.5)],
            1,
        )];
        let played = episode(1, &[1]);

        let one = statistics(&trajectory, &played, &profile, &reward);
        let mut merged = one.clone();
        for (head, row) in &one {
            merged.get_mut(head).unwrap().merge(row);
        }

        assert_eq!(merged["activation"].actions, 2);
        assert!(
            (merged["activation"].return_sum - 2.0 * one["activation"].return_sum).abs() < 1e-12
        );
    }

    #[test]
    fn a_confident_distribution_has_less_entropy_than_a_flat_one() {
        let profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let reward = Reward::for_stage(Stage::One);
        let played = episode(1, &[1]);

        let flat = statistics(
            &[step("activation", "a", &[("a", 0.5), ("b", 0.5)], 1)],
            &played,
            &profile,
            &reward,
        );
        let sure = statistics(
            &[step("activation", "a", &[("a", 0.99), ("b", 0.01)], 1)],
            &played,
            &profile,
            &reward,
        );
        assert!(sure["activation"].entropy_sum < flat["activation"].entropy_sum);
    }

    #[test]
    fn the_update_never_writes_a_weight_that_is_not_a_number() {
        // One NaN weight makes every score NaN, every probability NaN, and the policy silently
        // falls back to whatever sorted first — for the rest of the training run.
        let profile = blank_profile("sol", DEFAULT_DIMENSIONS);
        let trajectory = vec![
            step("activation", "good", &[("good", 1.0), ("bad", 0.0)], 1),
            step("activation", "bad", &[("good", 0.0), ("bad", 1.0)], 1),
        ];
        let played = episode(2, &[0, 2]);
        let rows = statistics(
            &trajectory,
            &played,
            &profile,
            &Reward::for_stage(Stage::One),
        );

        let mut updated = profile.clone();
        apply(&mut updated, &rows, Step::default());
        for head in updated.learned.heads.values() {
            for (slot, weight) in &head.weights {
                assert!(weight.is_finite(), "{slot} became {weight}");
            }
        }
        assert_eq!(updated.validate(Some("sol")), Ok(()));
    }
}
