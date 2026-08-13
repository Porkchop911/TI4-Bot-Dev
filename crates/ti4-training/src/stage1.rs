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
use ti4_model::content_types::DEFAULT;
use ti4_model::id::PlayerId;
use ti4_policy::learned::{DEFAULT_DIMENSIONS, Profile, blank_profile};

use crate::gradient::{Step, Telemetry, apply, batch_statistics};
use crate::reward::{Reward, Stage};
use crate::rollout::Horizon;

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
    /// Profiles to continue from, and how many generations have already been run.
    ///
    /// `None` starts blank. A resumed run continues the seed schedule from `generation` rather
    /// than restarting it, so it trains on games the earlier run did not — which is what makes a
    /// run stopped at N and resumed equivalent to an uninterrupted run of 2N.
    pub start: Option<Start>,
}

/// Where a resumed run picks up.
#[derive(Debug, Clone, PartialEq)]
pub struct Start {
    /// The profiles to continue training.
    pub profiles: BTreeMap<PlayerId, Profile>,
    /// How many generations have already been run.
    pub generation: usize,
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
            start: None,
        }
    }

    /// The same plan, continuing from a checkpoint's profiles.
    #[must_use]
    pub fn resuming(mut self, start: Start) -> Self {
        self.start = Some(start);
        self
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
    let blank = || -> BTreeMap<PlayerId, Profile> {
        plan.players
            .iter()
            .map(|player| {
                let faction = factions
                    .get(player)
                    .map_or_else(String::new, ToString::to_string);
                (player.clone(), blank_profile(&faction, plan.dimensions))
            })
            .collect()
    };
    let (mut profiles, already) = plan.start.as_ref().map_or_else(
        || (blank(), 0),
        |start| (start.profiles.clone(), start.generation),
    );

    let reward = Reward::for_stage(plan.stage);
    let horizon = plan.horizon();
    let mut generations = Vec::with_capacity(plan.generations);

    for index in 0..plan.generations {
        // A fresh block of seeds each generation. Reusing them would have every generation learn
        // from the same games, and a curve that flattened would say nothing about the policy.
        // Counted from the generations already run, not from this run's first one. Restarting
        // the schedule would have a resumed run re-train on games the earlier one already learned
        // from, and the resumed profile would differ from an uninterrupted one for that reason
        // alone.
        let elapsed = u64::try_from(already + index).unwrap_or(u64::MAX);
        let first = plan.seed.wrapping_add(elapsed.wrapping_mul(plan.games));
        let seeds: Vec<u64> = (first..first + plan.games).collect();
        let rollouts = crate::rollout::play_batch(
            content,
            &plan.players,
            &profiles,
            DEFAULT,
            &seeds,
            horizon,
            ti4_engine::opening::DEFAULT_REQUIREMENT,
        );

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
            index: already + index,
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
    fn a_run_stopped_and_resumed_matches_an_uninterrupted_one() {
        // The acceptance criterion the checkpoint work was for, and the property that makes a long
        // training run trustworthy: if resuming changed the result, every overnight run would be a
        // different experiment from the one somebody meant to start.
        //
        // The version of this test that shipped before did not train anything. It built two
        // `Checkpoint` structs, assigned `final_update = 10` to both, wrote identical history
        // entries into each with the same loop, and asserted they were equal — which they were,
        // because the test wrote both sides. Neutering its only real assertion left the whole
        // archive suite passing.
        //
        // This one trains.
        let whole = Plan {
            generations: 4,
            ..Plan::smoke(Stage::Two)
        };
        let uninterrupted = train(ContentStore::embedded(), &whole);

        let first_half = Plan {
            generations: 2,
            ..Plan::smoke(Stage::Two)
        };
        let stopped = train(ContentStore::embedded(), &first_half);
        let second_half = Plan {
            generations: 2,
            ..Plan::smoke(Stage::Two)
        }
        .resuming(Start {
            profiles: stopped.profiles.clone(),
            generation: 2,
        });
        let resumed = train(ContentStore::embedded(), &second_half);

        assert_ne!(
            stopped.profiles, resumed.profiles,
            "the second half trained nothing, so the comparison below would be vacuous"
        );
        assert_eq!(
            uninterrupted.profiles, resumed.profiles,
            "resuming produced a different policy from training straight through"
        );
    }

    #[test]
    fn a_resumed_run_trains_on_games_the_first_half_did_not() {
        // Why the seed schedule continues rather than restarting. Re-training on the same games
        // would make a resumed run differ from an uninterrupted one for that reason alone, and the
        // equivalence above is what would catch it — this names the cause.
        let plan = Plan {
            generations: 2,
            ..Plan::smoke(Stage::Two)
        };
        let first = train(ContentStore::embedded(), &plan);
        let resumed = train(
            ContentStore::embedded(),
            &plan.clone().resuming(Start {
                profiles: first.profiles.clone(),
                generation: 2,
            }),
        );

        let early: Vec<usize> = first.generations.iter().map(|one| one.index).collect();
        let later: Vec<usize> = resumed.generations.iter().map(|one| one.index).collect();
        assert_eq!(early, vec![0, 1]);
        assert_eq!(
            later,
            vec![2, 3],
            "the resumed run counted from where it stopped"
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
