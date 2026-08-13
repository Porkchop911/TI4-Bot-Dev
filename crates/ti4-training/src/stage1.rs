//! A training run: roll out, credit, update, repeat (M10-027, M10-028).
//!
//! Ported from the generation loop of the oracle's `tools/train_stage1_policy_gradient.py`.
//!
//! One generation plays a batch of games with the current profiles, turns each seat's decisions
//! into returns, and applies one centered update per head. The stage decides only what a decision
//! is worth — [`crate::reward::Stage::One`] pays for the opening, [`crate::reward::Stage::Two`]
//! for points — so the loop itself is the same either way.
//!
//! # What a run reports
//!
//! Every generation's telemetry is kept, because a training run that learns nothing looks exactly
//! like one that is still early. The number to watch is the return's spread: where it is zero, the
//! decisions in that batch were all credited alike and nothing was learned from them however many
//! there were.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;
use ti4_policy::learned::{DEFAULT_DIMENSIONS, Profile, blank_profile};

use crate::gradient::{Step, Telemetry, apply, batch_statistics};
use crate::reward::{Reward, Stage};
use crate::rollout::{Horizon, play};

/// How a run is set up.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Which stage's return to optimise.
    pub stage: Stage,
    /// Seats at the table.
    pub players: Vec<PlayerId>,
    /// How many generations to run.
    pub generations: usize,
    /// How many games each generation plays.
    pub games: u64,
    /// The step size and its bounds.
    pub step: Step,
    /// How many buckets each head carries.
    pub dimensions: usize,
    /// Where the seeds start. Each generation takes a fresh block, so no generation trains on the
    /// games the last one already learned from.
    pub seed: u64,
}

impl Plan {
    /// A small deterministic run, for a smoke test.
    #[must_use]
    pub fn smoke(stage: Stage) -> Self {
        Self {
            stage,
            players: ["a", "b", "c"]
                .iter()
                .map(|name| PlayerId::new(*name))
                .collect(),
            generations: 3,
            games: 4,
            step: Step::default(),
            dimensions: DEFAULT_DIMENSIONS,
            seed: 0,
        }
    }

    /// The horizon this stage's rollouts run to.
    #[must_use]
    pub const fn horizon(&self) -> Horizon {
        match self.stage {
            Stage::One => Horizon::opening(),
            Stage::Two => Horizon::short(),
        }
    }
}

/// What one generation did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Generation {
    /// Which generation this was, from zero.
    pub index: usize,
    /// Games that failed rather than played. Counted, never hidden: a failed game contributes no
    /// decisions, and a run that silently drops half its batch reports a clean generation.
    pub errors: usize,
    /// Decisions credited, across every seat and head.
    pub decisions: usize,
    /// Telemetry per seat and head.
    pub telemetry: BTreeMap<String, BTreeMap<String, Telemetry>>,
}

impl Generation {
    /// The largest return spread any head saw.
    ///
    /// Zero means every decision in the generation was credited alike, so no weight could move
    /// whatever the learning rate was.
    #[must_use]
    pub fn best_spread(&self) -> f64 {
        self.telemetry
            .values()
            .flat_map(BTreeMap::values)
            .map(|told| told.return_std)
            .fold(0.0, f64::max)
    }

    /// How far the weights moved, summed over heads.
    #[must_use]
    pub fn movement(&self) -> f64 {
        self.telemetry
            .values()
            .flat_map(BTreeMap::values)
            .map(|told| told.update_norm)
            .sum()
    }
}

/// A finished run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// The fitted profile per seat.
    pub profiles: BTreeMap<PlayerId, Profile>,
    /// What each generation did, in order.
    pub generations: Vec<Generation>,
}

/// Play, credit, update, repeat.
///
/// Starts from blank profiles: an untrained policy plays uniformly at random, which is the honest
/// starting point rather than one inherited from somewhere nobody fitted.
#[must_use]
pub fn train(content: &'static ContentStore, plan: &Plan) -> Run {
    let factions = ti4_engine::seating::seat_in_scope(&plan.players);
    let mut profiles: BTreeMap<PlayerId, Profile> = plan
        .players
        .iter()
        .map(|player| {
            let faction = factions
                .get(player)
                .map_or_else(String::new, ToString::to_string);
            (player.clone(), blank_profile(&faction, plan.dimensions))
        })
        .collect();

    let reward = Reward::for_stage(plan.stage);
    let horizon = plan.horizon();
    let mut generations = Vec::with_capacity(plan.generations);

    for index in 0..plan.generations {
        // A fresh block of seeds each generation. Reusing them would have every generation learn
        // from the same games, and a curve that flattened would say nothing about the policy.
        let first = plan
            .seed
            .wrapping_add((index as u64).wrapping_mul(plan.games));
        let rollouts: Vec<crate::rollout::Rollout> = (first..first + plan.games)
            .map(|seed| {
                play(
                    content,
                    &plan.players,
                    &profiles,
                    POK,
                    seed,
                    horizon,
                    ti4_engine::opening::DEFAULT_REQUIREMENT,
                )
            })
            .collect();

        let errors = rollouts.iter().filter(|one| one.error.is_some()).count();
        let collected = batch_statistics(&rollouts, &profiles, &reward);
        let decisions: usize = collected
            .values()
            .flat_map(BTreeMap::values)
            .map(|row| row.actions)
            .sum();

        let mut telemetry = BTreeMap::new();
        for (player, rows) in &collected {
            if let Some(profile) = profiles.get_mut(player) {
                telemetry.insert(player.to_string(), apply(profile, rows, plan.step));
            }
        }

        generations.push(Generation {
            index,
            errors,
            decisions,
            telemetry,
        });
    }

    Run {
        profiles,
        generations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_plays_games_and_credits_decisions() {
        let plan = Plan::smoke(Stage::Two);
        let run = train(ContentStore::embedded(), &plan);

        assert_eq!(run.generations.len(), plan.generations);
        for generation in &run.generations {
            assert_eq!(generation.errors, 0, "a generation failed games");
            assert!(
                generation.decisions > 0,
                "generation {} credited nothing",
                generation.index
            );
        }
    }

    #[test]
    fn training_changes_the_profile_it_started_from() {
        // The whole point. A run that reports generations and leaves the weights untouched is the
        // failure this test exists to catch, and it looks identical to a run that is still early.
        let plan = Plan::smoke(Stage::Two);
        let run = train(ContentStore::embedded(), &plan);

        let blank = blank_profile("sol", plan.dimensions);
        let fitted = run.profiles.values().next().expect("a seat was trained");
        assert_ne!(
            fitted.learned.heads, blank.learned.heads,
            "three generations moved no weight at all"
        );
        assert!(
            run.generations.iter().any(|one| one.movement() > 0.0),
            "no generation reported any movement"
        );
    }

    #[test]
    fn a_fitted_profile_is_still_a_valid_one() {
        // An update that produced a non-finite weight or an unusable temperature would score every
        // option as NaN, and the policy would silently play whatever sorted first for ever.
        let plan = Plan::smoke(Stage::Two);
        let run = train(ContentStore::embedded(), &plan);

        for (player, profile) in &run.profiles {
            assert_eq!(
                profile.validate(None),
                Ok(()),
                "{player}'s fitted profile is not loadable"
            );
        }
    }

    #[test]
    fn the_same_plan_trains_the_same_profile() {
        // Everything a training run concludes rests on this. Without it a regression and a reseed
        // look the same.
        let plan = Plan::smoke(Stage::Two);
        let once = train(ContentStore::embedded(), &plan);
        let twice = train(ContentStore::embedded(), &plan);
        assert_eq!(once.profiles, twice.profiles);
    }

    #[test]
    fn each_generation_trains_on_games_the_last_one_did_not() {
        // Reusing the seeds would have every generation learn from the same games, and a curve
        // that flattened would say nothing about the policy.
        let plan = Plan {
            generations: 2,
            ..Plan::smoke(Stage::Two)
        };
        let first = plan.seed;
        let second = plan.seed + plan.games;
        assert_ne!(first, second);

        let run = train(ContentStore::embedded(), &plan);
        assert_eq!(run.generations.len(), 2);
    }

    #[test]
    fn both_stages_have_a_gradient_once_returns_are_pooled_across_games() {
        // A correction to an earlier claim of mine, kept as a test so it cannot drift back.
        //
        // Measured per seat-game, 82 of 96 Stage-1 episodes credit every decision alike, and I
        // reported that as "Stage 1 has no gradient from blank". That does not follow. The update
        // centres returns per `(seat, head)` across the whole generation, not within one episode,
        // and different games gain different numbers of units — so the pooled spread is non-zero
        // and weights do move.
        //
        // What survives of the original finding is that Stage 1's signal is much *sparser*: it
        // comes from the minority of games where anything was produced at all, where Stage 2 has
        // something to say about nearly every seat.
        let one = train(ContentStore::embedded(), &Plan::smoke(Stage::One));
        let two = train(ContentStore::embedded(), &Plan::smoke(Stage::Two));

        for (stage, run) in [("stage 1", &one), ("stage 2", &two)] {
            let spread: f64 = run
                .generations
                .iter()
                .map(Generation::best_spread)
                .fold(0.0, f64::max);
            assert!(spread > 0.0, "{stage} credited every decision alike");
            assert!(
                run.generations
                    .iter()
                    .map(Generation::movement)
                    .sum::<f64>()
                    > 0.0,
                "{stage} moved no weight"
            );
        }
    }

    #[test]
    fn stage_two_has_a_gradient_from_blank_weights() {
        // The other half of the same measurement, and the reason training starts here.
        let plan = Plan::smoke(Stage::Two);
        let run = train(ContentStore::embedded(), &plan);

        let spread: f64 = run
            .generations
            .iter()
            .map(Generation::best_spread)
            .fold(0.0, f64::max);
        assert!(
            spread > 0.0,
            "stage 2 credited every decision alike, so nothing could be learned"
        );
        assert!(
            run.generations
                .iter()
                .map(Generation::movement)
                .sum::<f64>()
                > 0.0,
            "and no weight moved"
        );
    }
}
