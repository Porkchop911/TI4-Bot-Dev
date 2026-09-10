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
pub use ti4_policy::progress::Progress;

/// Potential over exactly the four Stage-1 gate components.
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
    let capacity_ships = capped(progress.capacity_ships, requirement.capacity_ships);
    let infantry = capped(progress.infantry, requirement.infantry);

    #[expect(clippy::cast_precision_loss, reason = "bars are single digits")]
    let planet_bar = requirement.planets_gained.max(1) as f64;
    #[expect(clippy::cast_precision_loss, reason = "bars are single digits")]
    let system_bar = requirement.systems.max(1) as f64;
    // Balanced progress, so a seat cannot bank the whole potential on one component.
    let balanced = (planets / planet_bar).min(systems / system_bar);

    expansion_weight * (planets + systems)
        + unit_weight * (capacity_ships + infantry)
        + conjunctive_weight * balanced
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
    /// Terminal bonus paid when the seat finishes with at least three victory points.
    ///
    /// Off by default (reference behavior). An experiment turns it on to sharpen credit toward
    /// high-scoring games: added to the final reward slot, so every decision's return — a suffix
    /// sum — carries exactly this much more in games that cross the bar, no matter which decisions
    /// produced them. The bar is three points because that is the sustained-play target it exists
    /// to push toward; see the Stage-2 plateau work.
    pub high_vp_bonus: f64,
    /// Uniform penalty per game whose opening did not clear, credited at the final slot so every
    /// decision's return carries the full-game cost of an uncleared opening. The round-one bonus
    /// is only visible to round-one decisions; this one prices the clearance risk everywhere.
    /// Zero keeps the reference reward exactly (Stage-2 gate experiments).
    pub clearance_weight: f64,
    /// Moderate reward for fleet strength, as a potential difference over the seat's fleet value
    /// in resources ([`Progress::fleet_value_permille`]: fighters 0.75 each, upgraded ships 1.3x
    /// their base unit's cost). Paid when the fleet grows and taken back when it is lost, like
    /// every other term here. Off by default; keep it well below `clearance_weight`, so a whole
    /// game of fleet-building never pays more than an uncleared opening costs.
    pub fleet_weight: f64,
    /// Small reward per technology owned beyond the setup baseline ([`Progress::technologies_gained`]).
    /// Off by default; keep it below `vp_weight`, so researching is a path to points rather than
    /// an end in itself.
    pub tech_weight: f64,
    /// Terminal penalty for strategy-card monoculture. When this seat played at least three cards
    /// and its most-played card exceeds 80% of them, the final slot pays `weight * excess`, where
    /// excess ramps linearly from zero at exactly 80% to one at 100% -- a four-of-four monoculture
    /// pays exactly `weight`, three-of-four (75%) pays nothing. Credited at the final slot like
    /// the clearance floor, so every decision's return carries it. Off by default.
    pub strategy_diversity_weight: f64,
    /// How much a decision is credited for what happens later (gamma).
    ///
    /// One (the default) is the undiscounted suffix sum this trainer has always used: every
    /// decision carries the entire rest of the game, ~190 decisions for a seat, so a round-one
    /// decision is credited with round four's outcome at full weight. Below one, credit decays
    /// with distance, which trades a little bias for a large drop in variance -- the standard
    /// reason discounting exists, and the one thing the current rule does not do.
    pub discount: f64,
    /// Centre returns against the mean for their **round**, not one mean for the whole head.
    ///
    /// Off by default. A suffix-sum return is systematically larger early in a game than late,
    /// simply because more of the game remains, so one scalar baseline per head leaves that
    /// difference in the advantage and calls it signal. Bucketing the baseline by round removes
    /// it. Costs nothing in data: the buckets partition the same decisions.
    pub round_baseline: bool,
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
            discount: 1.0,
            round_baseline: false,
            high_vp_bonus: 0.0,
            clearance_weight: 0.0,
            fleet_weight: 0.0,
            tech_weight: 0.0,
            strategy_diversity_weight: 0.0,
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

    /// Victory points, plus the objectives this seat could convert into them, plus the shaped
    /// path terms (fleet value and technologies beyond setup) when their weights are on.
    #[must_use]
    pub fn horizon_potential(&self, progress: &Progress) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "scores are single digits")]
        let points = progress.victory_points as f64;
        #[expect(clippy::cast_precision_loss, reason = "counts are single digits")]
        let public = progress.scoreable_public as f64;
        #[expect(clippy::cast_precision_loss, reason = "counts are single digits")]
        let secret = progress.scoreable_secret as f64;
        #[expect(
            clippy::cast_precision_loss,
            reason = "permille fleet values stay far below f64's exact-integer range"
        )]
        let fleet = progress.fleet_value_permille as f64 / 1000.0;
        #[expect(clippy::cast_precision_loss, reason = "tech counts are single digits")]
        let techs = progress.technologies_gained as f64;
        self.vp_weight * points
            + self.objective_weight * public
            + self.secret_weight * secret
            + self.fleet_weight * fleet
            + self.tech_weight * techs
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
    /// Strategy cards this seat played, by corpus card id. Feeds the monoculture penalty; empty
    /// keeps every legacy episode exactly as it was scored before the field existed.
    #[serde(default)]
    pub strategy_card_plays: std::collections::BTreeMap<String, i64>,
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
            // The high-VP terminal bonus, credited at the final slot so every decision's return
            // (a suffix sum) carries it. Only games that finish at or above the bar pay it.
            if reward.high_vp_bonus > 0.0
                && let Some(last) = rewards.last_mut()
            {
                *last += f64::from(u8::from(episode.final_progress.victory_points >= 3))
                    * reward.high_vp_bonus;
            }
            // The clearance floor, credited at the final slot so every decision's return carries
            // the full-game cost of an uncleared opening (the round-one bonus is only visible to
            // round-one decisions). Keeps learned play inside the gate's per-faction clearance
            // band instead of trading opening safety for mid-game VP.
            if reward.clearance_weight > 0.0
                && let Some(last) = rewards.last_mut()
            {
                *last -= f64::from(u8::from(!episode.cleared)) * reward.clearance_weight;
            }
            // The strategy-card monoculture penalty, credited at the final slot like the clearance
            // floor. A share of exactly 80% pays nothing; it ramps to `weight` at 100%. Fewer than
            // three plays is too small a sample to call monoculture.
            if reward.strategy_diversity_weight > 0.0 {
                let total: i64 = episode.strategy_card_plays.values().sum();
                if total >= 3 {
                    let most = *episode.strategy_card_plays.values().max().unwrap_or(&0);
                    #[allow(clippy::cast_precision_loss, reason = "card counts are single digits")]
                    let share = (most as f64) / (total as f64);
                    if share > 0.8 {
                        let excess = ((share - 0.8) / 0.2).clamp(0.0, 1.0);
                        if let Some(last) = rewards.last_mut() {
                            *last -= reward.strategy_diversity_weight * excess;
                        }
                    }
                }
            }
        }
    }

    // Discounted suffix sum. At gamma = 1 this is exactly the undiscounted form, bit for bit:
    // `future = future * 1.0 + value` and multiplying by one is exact in IEEE 754.
    let gamma = reward.discount;
    let mut future = 0.0;
    let mut result = vec![0.0; rewards.len()];
    for (index, value) in rewards.iter().enumerate().rev() {
        future = future * gamma + value;
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

    fn opening_composition(capacity_ships: i64, infantry: i64) -> Progress {
        Progress {
            capacity_ships,
            infantry,
            round_number: 1,
            ..Progress::default()
        }
    }

    #[test]
    fn capacity_and_infantry_each_provide_dense_progress_up_to_the_real_bar() {
        let reward = Reward::default();
        let empty = reward.stage1_potential(&opening_composition(0, 0));
        let one_hull = reward.stage1_potential(&opening_composition(1, 0));
        let composed = reward.stage1_potential(&opening_composition(2, 3));
        let excess = reward.stage1_potential(&opening_composition(20, 30));

        assert!(
            one_hull > empty,
            "a capacity ship must advance the potential"
        );
        assert!(
            composed > one_hull,
            "ground forces must advance the potential"
        );
        assert_eq!(
            composed, excess,
            "both composition components cap at the gate"
        );
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
            strategy_card_plays: std::collections::BTreeMap::new(),
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
    fn the_high_vp_bonus_pays_only_when_the_seat_finishes_at_or_above_three() {
        // The Stage-2 plateau experiment: games that cross the bar are worth exactly this much
        // more, and every decision's return carries it (suffix sums). Below the bar nothing moves.
        let mut reward = Reward::for_stage(Stage::Two);
        reward.high_vp_bonus = 0.5;
        let reference = Reward::for_stage(Stage::Two); // bonus stays off

        let crossed = Episode {
            steps: vec![opening(0, 1, 0, 1), opening(2, 2, 1, 1)],
            final_progress: Progress {
                victory_points: 3,
                ..opening(3, 3, 1, 1)
            },
            cleared: true,
            shortfall: 0.0,
            traded_goods: 0.0,
            strategy_card_plays: std::collections::BTreeMap::new(),
        };
        let below = Episode {
            final_progress: Progress {
                victory_points: 2,
                ..opening(3, 3, 1, 1)
            },
            ..crossed.clone()
        };

        for (name, episode) in [("crossed", &crossed), ("below", &below)] {
            let with = returns(episode, &reward);
            let without = returns(episode, &reference);
            assert_eq!(with.len(), without.len());
            for (index, (got, base)) in with.iter().zip(without.iter()).enumerate() {
                let expected_delta = if name == "crossed" { 0.5 } else { 0.0 };
                assert!(
                    (got - base - expected_delta).abs() < 1e-9,
                    "{name} step {index}: {got} against {base}"
                );
            }
        }
    }

    #[test]
    fn the_clearance_penalty_pays_only_when_the_opening_misses_the_bar() {
        // The Stage-2 gate experiment: an uncleared opening costs exactly this much in every
        // decision's return (suffix sums), whatever VP the game later reaches; cleared games are
        // untouched. This is what keeps learned play inside the gate's clearance band.
        let mut reward = Reward::for_stage(Stage::Two);
        reward.clearance_weight = 1.0;
        let reference = Reward::for_stage(Stage::Two); // penalty stays off

        let cleared = Episode {
            steps: vec![opening(0, 1, 0, 1), opening(2, 2, 1, 1)],
            final_progress: Progress {
                victory_points: 3,
                ..opening(3, 3, 1, 1)
            },
            cleared: true,
            shortfall: 0.0,
            traded_goods: 0.0,
            strategy_card_plays: std::collections::BTreeMap::new(),
        };
        let missed = Episode {
            final_progress: Progress {
                victory_points: 4,
                ..opening(3, 3, 1, 1)
            },
            cleared: false,
            shortfall: 2.0,
            ..cleared.clone()
        };

        for (name, episode) in [("cleared", &cleared), ("missed", &missed)] {
            let with = returns(episode, &reward);
            let without = returns(episode, &reference);
            assert_eq!(with.len(), without.len());
            for (index, (got, base)) in with.iter().zip(without.iter()).enumerate() {
                // The missed game's high VP must not offset the penalty: it is a flat cost.
                let expected_delta = if name == "missed" { -1.0 } else { 0.0 };
                assert!(
                    (got - base - expected_delta).abs() < 1e-9,
                    "{name} step {index}: {got} against {base}"
                );
            }
        }
    }

    #[test]
    fn fleet_and_tech_shaping_pay_for_gains_and_claw_back_losses() {
        // Both terms are potential differences: building the fleet and researching pay, losing
        // them takes it back. Off by default, so the reference reward stays exact.
        let mut reward = Reward::for_stage(Stage::Two);
        reward.fleet_weight = 0.05;
        reward.tech_weight = 0.1;

        let weak = Progress {
            fleet_value_permille: 2000,
            technologies_gained: 0,
            round_number: 2,
            ..Progress::default()
        };
        let strong = Progress {
            fleet_value_permille: 12000,
            technologies_gained: 3,
            round_number: 2,
            ..Progress::default()
        };

        // +10 resources of fleet and three new technologies: 0.05 * 10 + 0.1 * 3 = 0.8.
        let up = step_rewards(&[weak, strong], &reward)[0];
        assert!((up - 0.8).abs() < 1e-9, "{}", up);

        // The same ground lost is a negative step of the same size.
        let down = step_rewards(
            &[
                Progress {
                    fleet_value_permille: 12000,
                    technologies_gained: 3,
                    round_number: 2,
                    ..Progress::default()
                },
                Progress {
                    fleet_value_permille: 2000,
                    technologies_gained: 0,
                    round_number: 2,
                    ..Progress::default()
                },
            ],
            &reward,
        )[0];
        assert!((down + 0.8).abs() < 1e-9, "{}", down);

        // Off by default: the reference reward is untouched (the golden oracle test above pins
        // this bit-for-bit).
        let reference = Reward::for_stage(Stage::Two);
        assert!(reference.fleet_weight.abs() < f64::EPSILON);
        assert!(reference.tech_weight.abs() < f64::EPSILON);
    }

    fn episode_with_plays(plays: &[(&str, i64)]) -> Episode {
        let mut strategy_card_plays = std::collections::BTreeMap::new();
        for (card, count) in plays {
            strategy_card_plays.insert((*card).to_owned(), *count);
        }
        Episode {
            steps: vec![at(1), at(2)],
            final_progress: at(3),
            cleared: true,
            shortfall: 0.0,
            traded_goods: 0.0,
            strategy_card_plays,
        }
    }

    #[test]
    fn strategy_card_monoculture_pays_only_above_eighty_percent_of_at_least_three_plays() {
        let mut reward = Reward::for_stage(Stage::Two);
        reward.strategy_diversity_weight = 1.0;
        let reference = Reward::for_stage(Stage::Two); // penalty stays off

        // Four-of-four: a full monoculture pays exactly the weight, carried by every return.
        let mono = episode_with_plays(&[("warfare", 4)]);
        for (got, base) in returns(&mono, &reward)
            .iter()
            .zip(returns(&mono, &reference))
        {
            assert!((got - base + 1.0).abs() < 1e-9, "{got} against {base}");
        }

        // Three-of-four is 75%: below the bar, nothing moves.
        let mixed = episode_with_plays(&[("warfare", 3), ("diplomacy", 1)]);
        assert_eq!(returns(&mixed, &reward), returns(&mixed, &reference));

        // Four-of-five is exactly 80%: the bar is strict.
        let at_bar = episode_with_plays(&[("warfare", 4), ("diplomacy", 1)]);
        assert_eq!(returns(&at_bar, &reward), returns(&at_bar, &reference));

        // Five-of-six ramps: (5/6 - 0.8) / 0.2 = one sixth of the weight.
        let ramped = episode_with_plays(&[("warfare", 5), ("diplomacy", 1)]);
        for (got, base) in returns(&ramped, &reward)
            .iter()
            .zip(returns(&ramped, &reference))
        {
            assert!(
                (got - base + 1.0 / 6.0).abs() < 1e-9,
                "{got} against {base}"
            );
        }

        // Two plays is too small a sample to call monoculture.
        let tiny = episode_with_plays(&[("warfare", 2)]);
        assert_eq!(returns(&tiny, &reward), returns(&tiny, &reference));
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
            strategy_card_plays: std::collections::BTreeMap::new(),
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
            strategy_card_plays: std::collections::BTreeMap::new(),
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
    fn legacy_returns_match_the_oracle_on_the_shared_subspace() {
        // Generated by calling the oracle's `_returns`, not by reading it. The historical fixture
        // predates the composition gate, so migrate its one-unit proxy onto one capacity ship.
        // This retains exact parity for every unchanged return rule; the dedicated composition
        // test above owns the intentional divergence in Stage-1 shaping.
        let mut corpus: Vec<GoldenEpisode> =
            serde_json::from_str(include_str!("../tests/golden_returns.json"))
                .expect("the golden corpus parses");
        assert!(corpus.len() >= 6, "both stages, several shapes");

        for case in &mut corpus {
            for progress in case
                .steps
                .iter_mut()
                .chain(std::iter::once(&mut case.final_progress))
            {
                progress.capacity_ships = progress.units_gained.clamp(0, 1);
            }
        }

        for (index, case) in corpus.iter().enumerate() {
            let reward = Reward::for_stage(if case.stage == 1 {
                Stage::One
            } else {
                Stage::Two
            });
            // Legacy episodes carry no card-play data; with the new weights off by default this
            // stays bit-for-bit the oracle's return, which is exactly what the assertion checks.
            let episode = Episode {
                steps: case.steps.clone(),
                final_progress: case.final_progress,
                cleared: case.cleared,
                shortfall: case.shortfall,
                traded_goods: case.traded_goods,
                strategy_card_plays: std::collections::BTreeMap::new(),
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
            strategy_card_plays: std::collections::BTreeMap::new(),
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
            strategy_card_plays: std::collections::BTreeMap::new(),
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
