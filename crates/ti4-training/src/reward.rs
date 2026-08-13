//! What a played game is worth to the learner (M10-011).
//!
//! Ported from the oracle's `tools/train_stage1_policy_gradient.py`: `potential`, `Reward`,
//! `_step_rewards` and `_returns`.
//!
//! # The two stages
//!
//! **Stage 1 optimises the opening.** Round-4 victory points have a standard deviation of about
//! 1.4 per player-game and are mostly interaction, so from zero weights they are very nearly pure
//! noise — a search run against them selects on luck long before it selects on play. The three
//! opening facts ([`ti4_engine::opening`]) are dense, available after one round instead of four,
//! and almost noise-free.
//!
//! **Stage 2 optimises points.** Victory points are the objective and everything else only shapes
//! the path to them. The shaping is not optional: a four-round game yields about 1.49 victory
//! points and 1.3 scoring decisions per faction-game, which is far too sparse to learn from on its
//! own, so a seat is also paid for *reaching* a position it could score from.
//!
//! # Why the coefficients are what they are
//!
//! Each one encodes a way this went wrong before:
//!
//! - Every component is **capped at its requirement**, so production or territory beyond the gate
//!   cannot farm an auxiliary reward.
//! - `objective_weight` must stay **below** `vp_weight`, or reaching a scoring position would pay
//!   better than scoring and a policy would learn to stand next to points without taking them.
//! - `r1_shaping` is a tenth. At Stage-1 magnitudes the opening potential would swamp a
//!   1.49-point game and Stage 2 would quietly be Stage 1 again.
//! - The opening potential applies **only to transitions with both ends inside round one**. Across
//!   the whole game it telescopes into "still holds three gained planets at the horizon", which is
//!   a different and much easier question than gaining them — and the status phase that closes the
//!   round would leak round-two state into the round-one gradient.
//! - Rewards are **potential differences**, so losing ground produces a negative step rather than
//!   merely a smaller positive one.

use serde::{Deserialize, Serialize};
use ti4_engine::opening::{DEFAULT_REQUIREMENT, Requirement};

/// One captured decision's view of what the game had produced so far.
///
/// Rules facts only — no authored opinion about which objective is worth chasing. `scoreable_*`
/// are counts from the engine's own scoreable predicates: the objectives this seat could score at
/// this instant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    /// Planets taken since setup.
    pub planets_gained: i64,
    /// Distinct systems holding a controlled planet.
    pub systems: i64,
    /// Units gained since setup.
    pub units_gained: i64,
    /// Points scored.
    pub victory_points: i64,
    /// Revealed public objectives this seat could score right now.
    pub scoreable_public: i64,
    /// Secret objectives this seat could score right now.
    pub scoreable_secret: i64,
    /// Which round this snapshot was taken in.
    pub round_number: u32,
}

/// Potential over exactly the three Stage-1 gate components.
///
/// Capping each component at its requirement is what stops production or territory beyond the gate
/// from farming an auxiliary reward. A state loss produces a negative delta, which is the point of
/// expressing the reward as a difference of potentials rather than as an award.
#[must_use]
pub fn potential(
    progress: &Progress,
    requirement: Requirement,
    expansion_weight: f64,
    unit_weight: f64,
    conjunctive_weight: f64,
) -> f64 {
    let capped = |value: i64, bar: usize| -> f64 {
        let bar = i64::try_from(bar).unwrap_or(i64::MAX);
        #[expect(
            clippy::cast_precision_loss,
            reason = "planet, system and unit counts are single digits"
        )]
        let held = value.clamp(0, bar) as f64;
        held
    };
    let planets = capped(progress.planets_gained, requirement.planets_gained);
    let systems = capped(progress.systems, requirement.systems);
    let units = capped(progress.units_gained, requirement.units_gained);

    #[expect(clippy::cast_precision_loss, reason = "bars are single digits")]
    let planet_bar = requirement.planets_gained.max(1) as f64;
    #[expect(clippy::cast_precision_loss, reason = "bars are single digits")]
    let system_bar = requirement.systems.max(1) as f64;
    // Balanced progress, so a seat cannot bank the whole potential on one component.
    let balanced = (planets / planet_bar).min(systems / system_bar);

    expansion_weight * (planets + systems) + unit_weight * units + conjunctive_weight * balanced
}

/// Every coefficient of the training return, carried as one object.
///
/// A struct rather than eleven arguments threaded through the trainer. In the oracle, the last
/// time a coefficient was added by hand one call site kept the old arity, every game raised, the
/// trainer caught it per game and wrote 432 zero-scored rows per generation as legitimate data,
/// and two arms ran seven generations of noise while exiting successfully. A missing field here is
/// one construction error in one place instead.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Reward {
    /// Which stage's return to compute.
    pub stage: Stage,
    /// The opening bar the potential is capped against.
    #[serde(skip, default = "default_requirement")]
    pub requirement: Requirement,
    /// Stage 1: crossing the round-one bar, against the shortfall for approaching it.
    pub clear_bonus: f64,
    /// How much a gained planet or system is worth in the opening potential.
    pub expansion_weight: f64,
    /// How much a gained unit is worth.
    pub unit_weight: f64,
    /// Extra for progressing on planets and systems together rather than one alone.
    pub conjunctive_weight: f64,
    /// Stage 2: victory points are the objective; the rest only shapes the path to them.
    pub vp_weight: f64,
    /// Paid for satisfying a revealed public objective and taken back when it is scored, so a
    /// point is worth `vp_weight` however it is reached and a satisfied-then-lost position nets
    /// zero. Must stay below `vp_weight`, or reaching a scoring position would pay better than
    /// scoring.
    pub objective_weight: f64,
    /// The same for a secret objective.
    pub secret_weight: f64,
    /// Round one is priced, not demanded. A hard floor measured brittle in both directions: too
    /// strict and a seat froze with every candidate rejected, too forgiving and it ratcheted down.
    pub r1_bonus: f64,
    /// The Stage-1 potential, scaled down and applied only to transitions inside round one.
    pub r1_shaping: f64,
    /// Trade goods obtained through a transaction. Off by default: goods are not points.
    pub trade_bonus: f64,
}

const fn default_requirement() -> Requirement {
    DEFAULT_REQUIREMENT
}

/// Which curriculum stage a return is computed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    /// Optimise the opening round.
    One,
    /// Optimise points, with the opening riding along.
    Two,
}

impl Default for Reward {
    fn default() -> Self {
        Self {
            stage: Stage::One,
            requirement: DEFAULT_REQUIREMENT,
            clear_bonus: 22.0,
            expansion_weight: 2.0,
            unit_weight: 1.0,
            conjunctive_weight: 0.0,
            vp_weight: 1.0,
            objective_weight: 0.35,
            secret_weight: 0.25,
            r1_bonus: 3.0,
            r1_shaping: 0.1,
            trade_bonus: 0.0,
        }
    }
}

impl Reward {
    /// The default coefficients for a stage.
    #[must_use]
    pub fn for_stage(stage: Stage) -> Self {
        Self {
            stage,
            ..Self::default()
        }
    }

    /// The Stage-1 potential of one snapshot.
    #[must_use]
    pub fn stage1_potential(&self, progress: &Progress) -> f64 {
        potential(
            progress,
            self.requirement,
            self.expansion_weight,
            self.unit_weight,
            self.conjunctive_weight,
        )
    }

    /// Victory points, plus the objectives this seat could convert into them.
    #[must_use]
    pub fn horizon_potential(&self, progress: &Progress) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "scores are single digits")]
        let points = progress.victory_points as f64;
        #[expect(clippy::cast_precision_loss, reason = "counts are single digits")]
        let public = progress.scoreable_public as f64;
        #[expect(clippy::cast_precision_loss, reason = "counts are single digits")]
        let secret = progress.scoreable_secret as f64;
        self.vp_weight * points + self.objective_weight * public + self.secret_weight * secret
    }

    /// Whether this reward is self-consistent.
    ///
    /// The one that matters is `objective_weight < vp_weight`. Above it, a policy is paid more for
    /// standing next to a point than for taking it, and it will learn exactly that — which looks
    /// like a policy that has learned to play well right up until you check the scoreboard.
    ///
    /// # Errors
    /// [`RewardError`] naming the coefficient that is wrong.
    pub const fn validate(&self) -> Result<(), RewardError> {
        if self.objective_weight >= self.vp_weight {
            return Err(RewardError::ObjectivePaysBetterThanScoring);
        }
        if self.secret_weight >= self.vp_weight {
            return Err(RewardError::SecretPaysBetterThanScoring);
        }
        if self.r1_shaping > 1.0 {
            return Err(RewardError::OpeningSwampsPoints);
        }
        Ok(())
    }
}

/// A reward whose coefficients would teach the wrong thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RewardError {
    /// Reaching a scoring position pays at least as well as scoring.
    #[error(
        "objective_weight must stay below vp_weight, or standing next to a point pays as well as taking it"
    )]
    ObjectivePaysBetterThanScoring,
    /// The same for secrets.
    #[error("secret_weight must stay below vp_weight")]
    SecretPaysBetterThanScoring,
    /// The opening shaping would dominate the points it is meant to shape.
    #[error("r1_shaping above 1.0 makes Stage 2 into Stage 1")]
    OpeningSwampsPoints,
}

/// One played game, reduced to what the return needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    /// The progress snapshot at each captured decision, in order.
    pub steps: Vec<Progress>,
    /// The progress after the last decision.
    pub final_progress: Progress,
    /// Whether the seat cleared its opening bar.
    pub cleared: bool,
    /// How far off the bar it was. Zero when cleared.
    pub shortfall: f64,
    /// Trade goods obtained through transactions.
    pub traded_goods: f64,
}

/// The reward following each captured decision, as potential differences.
///
/// One shorter than the snapshot list, because a reward follows a decision and the final snapshot
/// follows the last one.
#[must_use]
pub fn step_rewards(snapshots: &[Progress], reward: &Reward) -> Vec<f64> {
    snapshots
        .windows(2)
        .map(|pair| {
            let (before, after) = (&pair[0], &pair[1]);
            match reward.stage {
                Stage::One => reward.stage1_potential(after) - reward.stage1_potential(before),
                Stage::Two => {
                    let mut value =
                        reward.horizon_potential(after) - reward.horizon_potential(before);
                    // Both ends inside round one, so the status phase that closes the round
                    // cannot leak round-two state into the round-one gradient. Applied across the
                    // whole game this telescopes into "still holds three gained planets at the
                    // horizon", which is a different and much easier question than gaining them.
                    if before.round_number == 1 && after.round_number == 1 {
                        value += reward.r1_shaping
                            * (reward.stage1_potential(after) - reward.stage1_potential(before));
                    }
                    value
                }
            }
        })
        .collect()
}

/// The return at each decision: the sum of every reward from it to the end of the game.
#[must_use]
pub fn returns(episode: &Episode, reward: &Reward) -> Vec<f64> {
    if episode.steps.is_empty() {
        return Vec::new();
    }
    let mut snapshots = episode.steps.clone();
    snapshots.push(episode.final_progress);
    let mut rewards = step_rewards(&snapshots, reward);
    if rewards.is_empty() {
        return Vec::new();
    }

    match reward.stage {
        Stage::One => {
            if let Some(last) = rewards.last_mut() {
                *last += reward.clear_bonus * f64::from(u8::from(episode.cleared));
            }
        }
        Stage::Two => {
            // Credited at the last decision taken in round one, so every round-one decision
            // carries it and no later one does. A round-three decision cannot change whether round
            // one cleared, and paying it there would only add variance.
            let final_round_one = episode
                .steps
                .iter()
                .enumerate()
                .filter(|(_, step)| step.round_number == 1)
                .map(|(index, _)| index)
                .next_back();
            if let Some(index) = final_round_one
                && let Some(slot) = rewards.get_mut(index)
            {
                *slot += reward.r1_bonus
                    * (f64::from(u8::from(episode.cleared)) - 0.1 * episode.shortfall);
            }
            if let Some(last) = rewards.last_mut() {
                *last += reward.trade_bonus * episode.traded_goods;
            }
        }
    }

    let mut future = 0.0;
    let mut result = vec![0.0; rewards.len()];
    for (index, value) in rewards.iter().enumerate().rev() {
        future += value;
        result[index] = future;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(round: u32) -> Progress {
        Progress {
            round_number: round,
            ..Progress::default()
        }
    }

    fn opening(planets: i64, systems: i64, units: i64, round: u32) -> Progress {
        Progress {
            planets_gained: planets,
            systems,
            units_gained: units,
            round_number: round,
            ..Progress::default()
        }
    }

    #[test]
    fn the_potential_is_capped_at_the_bar_so_nothing_beyond_it_can_be_farmed() {
        // Without the cap a policy learns to keep producing after the gate is met, because each
        // extra unit still pays. The gate is a bar, not a scoreboard.
        let reward = Reward::default();
        let met = reward.stage1_potential(&opening(3, 3, 1, 1));
        let far_beyond = reward.stage1_potential(&opening(30, 30, 30, 1));
        assert!(
            (met - far_beyond).abs() < f64::EPSILON,
            "{met} against {far_beyond}"
        );
    }

    #[test]
    fn losing_ground_is_a_negative_step_not_a_smaller_positive_one() {
        // The reason rewards are potential differences. A seat that loses two planets should feel
        // it, and an award-shaped reward can only ever pay zero.
        let reward = Reward::default();
        let snapshots = [opening(3, 3, 1, 1), opening(1, 1, 1, 1)];
        let steps = step_rewards(&snapshots, &reward);
        assert_eq!(steps.len(), 1);
        assert!(steps[0] < 0.0, "{}", steps[0]);
    }

    #[test]
    fn progress_toward_the_bar_pays_before_the_bar_is_reached() {
        // A from-zero policy needs something to climb. Pass/fail alone is flat everywhere below
        // the bar.
        let reward = Reward::default();
        let climbing = step_rewards(&[opening(0, 1, 0, 1), opening(1, 2, 0, 1)], &reward);
        assert!(climbing[0] > 0.0, "{}", climbing[0]);
    }

    #[test]
    fn clearing_the_bar_is_paid_once_at_the_end_of_a_stage_one_episode() {
        let reward = Reward::for_stage(Stage::One);
        let cleared = Episode {
            steps: vec![opening(0, 1, 0, 1), opening(2, 2, 1, 1)],
            final_progress: opening(3, 3, 1, 1),
            cleared: true,
            shortfall: 0.0,
            traded_goods: 0.0,
        };
        let missed = Episode {
            cleared: false,
            ..cleared.clone()
        };

        let with = returns(&cleared, &reward);
        let without = returns(&missed, &reward);
        // The bonus lands on the last step and telescopes back through every earlier return.
        assert!((with[0] - without[0] - reward.clear_bonus).abs() < 1e-9);
        assert!((with[1] - without[1] - reward.clear_bonus).abs() < 1e-9);
    }

    #[test]
    fn a_return_is_the_sum_of_every_reward_still_to_come() {
        let reward = Reward::for_stage(Stage::One);
        let episode = Episode {
            steps: vec![opening(0, 1, 0, 1), opening(1, 2, 0, 1)],
            final_progress: opening(2, 3, 0, 1),
            cleared: false,
            shortfall: 1.0,
            traded_goods: 0.0,
        };
        let steps = {
            let mut snapshots = episode.steps.clone();
            snapshots.push(episode.final_progress);
            step_rewards(&snapshots, &reward)
        };
        let got = returns(&episode, &reward);

        assert!(
            (got[1] - steps[1]).abs() < 1e-9,
            "the last return is its own reward"
        );
        assert!(
            (got[0] - (steps[0] + steps[1])).abs() < 1e-9,
            "and an earlier one carries everything after it"
        );
    }

    #[test]
    fn scoring_a_point_pays_better_than_standing_next_to_one() {
        // The coefficient relation the whole of Stage 2 rests on. Reversed, a policy is paid more
        // for reaching a scoring position than for scoring, and it will learn exactly that — which
        // looks like good play right up until somebody checks the scoreboard.
        let reward = Reward::for_stage(Stage::Two);
        let satisfied = Progress {
            scoreable_public: 1,
            round_number: 2,
            ..Progress::default()
        };
        let scored = Progress {
            victory_points: 1,
            round_number: 2,
            ..Progress::default()
        };

        let reaching = reward.horizon_potential(&satisfied);
        let taking = reward.horizon_potential(&scored);
        assert!(taking > reaching, "{taking} against {reaching}");
    }

    #[test]
    fn taking_a_point_nets_the_full_weight_however_it_was_reached() {
        // The objective payment is taken back when the objective is scored, so satisfy-then-score
        // pays exactly `vp_weight` in total rather than that plus the shaping.
        let reward = Reward::for_stage(Stage::Two);
        let none = Progress {
            round_number: 2,
            ..Progress::default()
        };
        let satisfied = Progress {
            scoreable_public: 1,
            round_number: 2,
            ..Progress::default()
        };
        let scored = Progress {
            victory_points: 1,
            scoreable_public: 0,
            round_number: 2,
            ..Progress::default()
        };

        let path = step_rewards(&[none, satisfied, scored], &reward);
        let total: f64 = path.iter().sum();
        assert!(
            (total - reward.vp_weight).abs() < 1e-9,
            "satisfy-then-score paid {total}, not {}",
            reward.vp_weight
        );
    }

    #[test]
    fn a_satisfied_position_that_is_lost_again_nets_nothing() {
        let reward = Reward::for_stage(Stage::Two);
        let none = Progress {
            round_number: 2,
            ..Progress::default()
        };
        let satisfied = Progress {
            scoreable_public: 1,
            round_number: 2,
            ..Progress::default()
        };

        let path = step_rewards(&[none, satisfied, none], &reward);
        assert!(path.iter().sum::<f64>().abs() < 1e-9);
    }

    #[test]
    fn the_opening_shapes_round_one_and_stops_there() {
        // Applied across the whole game the opening potential telescopes into "still holds three
        // gained planets at the horizon", which is a different and much easier question than
        // gaining them. Both ends must be inside round one, so the status phase that closes the
        // round cannot leak round-two state into the round-one gradient.
        let reward = Reward::for_stage(Stage::Two);
        let inside = step_rewards(&[opening(0, 1, 0, 1), opening(2, 2, 1, 1)], &reward);
        let crossing = step_rewards(&[opening(0, 1, 0, 1), opening(2, 2, 1, 2)], &reward);
        let later = step_rewards(&[opening(0, 1, 0, 3), opening(2, 2, 1, 3)], &reward);

        assert!(inside[0] > 0.0, "round one is shaped: {}", inside[0]);
        assert!(
            crossing[0].abs() < f64::EPSILON,
            "a transition leaving round one is not: {}",
            crossing[0]
        );
        assert!(
            later[0].abs() < f64::EPSILON,
            "and neither is one wholly outside it: {}",
            later[0]
        );
    }

    #[test]
    fn the_opening_shaping_cannot_swamp_the_points_it_shapes() {
        // At Stage-1 magnitudes the opening potential would dominate a 1.49-point game and Stage 2
        // would quietly be Stage 1 again.
        let reward = Reward::for_stage(Stage::Two);
        let opening_step = step_rewards(&[opening(0, 0, 0, 1), opening(3, 3, 1, 1)], &reward)[0];
        let one_point = reward.vp_weight;
        assert!(
            opening_step < 2.0 * one_point,
            "a whole cleared opening is worth {opening_step} against {one_point} for a point"
        );
    }

    #[test]
    fn the_round_one_bonus_lands_on_the_last_round_one_decision() {
        // Every round-one decision must carry it and no later one. A round-three decision cannot
        // change whether round one cleared, and paying it there would only add variance.
        let reward = Reward::for_stage(Stage::Two);
        let episode = Episode {
            steps: vec![at(1), at(1), at(2), at(3)],
            final_progress: at(3),
            cleared: true,
            shortfall: 0.0,
            traded_goods: 0.0,
        };
        let missed = Episode {
            cleared: false,
            shortfall: 0.0,
            ..episode.clone()
        };

        let with = returns(&episode, &reward);
        let without = returns(&missed, &reward);
        assert!(
            (with[0] - without[0] - reward.r1_bonus).abs() < 1e-9,
            "the first round-one decision carries it"
        );
        assert!(
            (with[1] - without[1] - reward.r1_bonus).abs() < 1e-9,
            "and so does the last"
        );
        assert!(
            (with[2] - without[2]).abs() < 1e-9,
            "a round-two decision does not"
        );
        assert!((with[3] - without[3]).abs() < 1e-9);
    }

    #[test]
    fn a_reward_that_would_teach_the_wrong_thing_is_refused() {
        assert_eq!(Reward::for_stage(Stage::One).validate(), Ok(()));
        assert_eq!(Reward::for_stage(Stage::Two).validate(), Ok(()));

        let inverted = Reward {
            objective_weight: 1.5,
            ..Reward::for_stage(Stage::Two)
        };
        assert_eq!(
            inverted.validate(),
            Err(RewardError::ObjectivePaysBetterThanScoring)
        );

        let swamping = Reward {
            r1_shaping: 5.0,
            ..Reward::for_stage(Stage::Two)
        };
        assert_eq!(swamping.validate(), Err(RewardError::OpeningSwampsPoints));
    }

    #[derive(serde::Deserialize)]
    struct GoldenEpisode {
        stage: u8,
        steps: Vec<Progress>,
        #[serde(rename = "final")]
        final_progress: Progress,
        cleared: bool,
        shortfall: f64,
        traded_goods: f64,
        returns: Vec<f64>,
    }

    #[test]
    fn the_returns_match_the_oracle_trainer_to_the_number() {
        // Generated by calling the oracle's `_returns`, not by reading it. Every coefficient and
        // every placement rule interacts — the clear bonus telescopes back through the episode,
        // the round-one bonus lands on one specific decision, the shaping applies to some
        // transitions and not others — so agreeing on each rule separately is not the same as
        // agreeing on the number a trainer would actually use.
        let corpus: Vec<GoldenEpisode> =
            serde_json::from_str(include_str!("../tests/golden_returns.json"))
                .expect("the golden corpus parses");
        assert!(corpus.len() >= 6, "both stages, several shapes");

        for (index, case) in corpus.iter().enumerate() {
            let reward = Reward::for_stage(if case.stage == 1 {
                Stage::One
            } else {
                Stage::Two
            });
            let episode = Episode {
                steps: case.steps.clone(),
                final_progress: case.final_progress,
                cleared: case.cleared,
                shortfall: case.shortfall,
                traded_goods: case.traded_goods,
            };

            let ours = returns(&episode, &reward);
            assert_eq!(ours.len(), case.returns.len(), "episode {index}");
            for (step, (got, want)) in ours.iter().zip(&case.returns).enumerate() {
                assert!(
                    (got - want).abs() < 1e-9,
                    "episode {index} step {step}: {got} against the oracle's {want}"
                );
            }
        }
    }

    #[test]
    fn an_episode_with_no_decisions_has_no_returns() {
        let empty = Episode {
            steps: Vec::new(),
            final_progress: at(1),
            cleared: false,
            shortfall: 7.0,
            traded_goods: 0.0,
        };
        assert!(returns(&empty, &Reward::default()).is_empty());
    }

    #[test]
    fn the_two_stages_disagree_about_the_same_game() {
        // If they did not, there would be no curriculum — just one stage run twice.
        let episode = Episode {
            steps: vec![opening(0, 1, 0, 1), opening(3, 3, 1, 1)],
            final_progress: Progress {
                planets_gained: 3,
                systems: 3,
                units_gained: 1,
                victory_points: 2,
                round_number: 2,
                ..Progress::default()
            },
            cleared: true,
            shortfall: 0.0,
            traded_goods: 0.0,
        };

        let one = returns(&episode, &Reward::for_stage(Stage::One));
        let two = returns(&episode, &Reward::for_stage(Stage::Two));
        assert!(
            (one[0] - two[0]).abs() > 1.0,
            "stage 1 returned {} and stage 2 {}",
            one[0],
            two[0]
        );
    }
}
