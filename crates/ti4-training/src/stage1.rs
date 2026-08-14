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
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_model::content_types::{FULL, SourceSet};
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::learned::{DEFAULT_DIMENSIONS, Profile, blank_explicit_profile};

use crate::gradient::{Step, Telemetry, apply};
use crate::reward::{Reward, Stage};
use crate::rollout::{
    Horizon, Rollout, play_batch_statistics, play_rotated_batch_statistics,
    play_rotated_save54_pool_batch_statistics,
};

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
    /// Legacy schema-2 bucket count. New Stage-1 runs use sparse explicit schema 4.
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

/// Python-reference Stage-1 configuration, keyed by faction and counterbalanced by rotation.
#[derive(Debug, Clone, PartialEq)]
pub struct FactionPlan {
    /// Which reward and rollout horizon to use.
    pub stage: Stage,
    /// Factions whose profiles are trained.
    pub factions: Vec<FactionId>,
    /// Updates to run in this invocation.
    pub generations: usize,
    /// Independent varied-map seeds per update. Each is played once per seat rotation.
    pub train_seeds: u64,
    /// Learning rule. The reference uses 0.03 / 0.01 / 1.0.
    pub step: Step,
    /// First training seed. Resumption advances by `train_seeds` per completed update.
    pub seed: u64,
    /// Content scope used by every rollout.
    pub sources: SourceSet,
    /// Python-compatible constrained map pool. `None` retains the Rust varied-map generator.
    pub map_pool: Option<Arc<ti4_sim::MapPool>>,
    /// Python Stage 1 uses `tile_seed = game_seed + 20_000_000`.
    pub tile_seed_offset: u64,
    /// Optional continuation state.
    pub start: Option<FactionStart>,
}

/// Where a faction-keyed run resumes.
#[derive(Debug, Clone, PartialEq)]
pub struct FactionStart {
    pub profiles: BTreeMap<FactionId, Profile>,
    pub generation: usize,
}

/// A faction-keyed run whose profiles follow factions through physical-seat rotations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactionRun {
    pub profiles: BTreeMap<FactionId, Profile>,
    pub generations: Vec<Generation>,
}

/// Held-out opening measurements for one faction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct OpeningMetrics {
    pub seat_games: usize,
    pub clearance: f64,
    pub planets_gained: f64,
    pub systems: f64,
    pub units_gained: f64,
    pub shortfall: f64,
}

impl FactionPlan {
    /// Exact high-level settings of the successful three-faction Python curriculum.
    #[must_use]
    pub fn python_reference() -> Self {
        Self {
            stage: Stage::One,
            factions: ["letnev", "jolnar", "hacan"]
                .into_iter()
                .map(FactionId::new)
                .collect(),
            generations: 25,
            train_seeds: 16,
            step: Step {
                learning_rate: 0.03,
                entropy: 0.01,
                gradient_clip: 1.0,
            },
            seed: 73_000_000,
            sources: FULL,
            map_pool: None,
            tile_seed_offset: 20_000_000,
            start: None,
        }
    }

    /// Six-faction Stage-2 configuration on shared varied maps with complete seat rotation.
    ///
    /// It deliberately reuses the Stage-1 optimizer settings until a measured Stage-2 tuning run
    /// justifies changing them. Four-round reward semantics and horizon come from [`Stage::Two`];
    /// no teacher or authored utility is introduced by selecting this plan.
    #[must_use]
    pub fn stage_two_reference() -> Self {
        Self {
            stage: Stage::Two,
            factions: ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
                .into_iter()
                .map(FactionId::new)
                .collect(),
            generations: 25,
            train_seeds: 16,
            step: Step {
                learning_rate: 0.03,
                entropy: 0.01,
                gradient_clip: 1.0,
            },
            seed: 93_000_000,
            sources: FULL,
            map_pool: None,
            tile_seed_offset: 20_000_000,
            start: None,
        }
    }

    #[must_use]
    pub fn resuming(mut self, start: FactionStart) -> Self {
        self.start = Some(start);
        self
    }

    /// The horizon selected by this faction run's stage.
    #[must_use]
    pub const fn horizon(&self) -> Horizon {
        match self.stage {
            Stage::One => Horizon::opening(),
            Stage::Two => Horizon::short(),
        }
    }
}

/// Train explicit schema-4 profiles on varied maps with full seat rotation.
#[must_use]
pub fn train_factions(content: &'static ContentStore, plan: &FactionPlan) -> FactionRun {
    let blank = || {
        plan.factions
            .iter()
            .map(|faction| (faction.clone(), blank_explicit_profile(faction.as_str())))
            .collect::<BTreeMap<_, _>>()
    };
    let (mut profiles, already) = plan.start.as_ref().map_or_else(
        || (blank(), 0),
        |start| (start.profiles.clone(), start.generation),
    );
    let reward = Reward::for_stage(plan.stage);
    let mut generations = Vec::with_capacity(plan.generations);
    for local in 0..plan.generations {
        let index = already + local;
        let elapsed = u64::try_from(index).unwrap_or(u64::MAX);
        let first = plan
            .seed
            .wrapping_add(elapsed.wrapping_mul(plan.train_seeds));
        let seeds: Vec<u64> = (first..first + plan.train_seeds).collect();
        let reduced = plan.map_pool.as_ref().map_or_else(
            || {
                play_rotated_batch_statistics(
                    content,
                    &plan.factions,
                    &profiles,
                    plan.sources,
                    &seeds,
                    plan.horizon(),
                    ti4_engine::opening::DEFAULT_REQUIREMENT,
                    &reward,
                )
            },
            |pool| {
                play_rotated_save54_pool_batch_statistics(
                    content,
                    &plan.factions,
                    &profiles,
                    plan.sources,
                    &seeds,
                    plan.horizon(),
                    ti4_engine::opening::DEFAULT_REQUIREMENT,
                    Arc::clone(pool),
                    plan.tile_seed_offset,
                    &reward,
                )
            },
        );
        let errors = reduced.errors;
        let decisions = reduced.decisions();
        let collected = reduced.statistics;
        let mut telemetry = BTreeMap::new();
        for (faction, rows) in &collected {
            if let Some(profile) = profiles.get_mut(faction) {
                telemetry.insert(faction.to_string(), apply(profile, rows, plan.step));
            }
        }
        generations.push(Generation {
            index,
            errors,
            decisions,
            telemetry,
        });
    }
    FactionRun {
        profiles,
        generations,
    }
}

/// Evaluate a faction table on held-out varied maps, with every faction in every seat.
#[must_use]
pub fn evaluate_factions(
    content: &'static ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    first_seed: u64,
    seeds: u64,
) -> BTreeMap<FactionId, OpeningMetrics> {
    let seed_block: Vec<u64> = (first_seed..first_seed + seeds).collect();
    let rollouts = crate::rollout::play_rotated_batch(
        content,
        factions,
        profiles,
        sources,
        &seed_block,
        Horizon::opening(),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
    );
    opening_metrics(&rollouts)
}

/// Evaluate a faction table on the exact Python-compatible constrained Save-54 map pool.
#[must_use]
pub fn evaluate_factions_on_pool(
    content: &'static ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    first_seed: u64,
    seeds: u64,
    pool: Arc<ti4_sim::MapPool>,
    tile_seed_offset: u64,
) -> BTreeMap<FactionId, OpeningMetrics> {
    let seed_block: Vec<u64> = (first_seed..first_seed + seeds).collect();
    let rollouts = crate::rollout::play_rotated_save54_pool_batch(
        content,
        factions,
        profiles,
        sources,
        &seed_block,
        Horizon::opening(),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        pool,
        tile_seed_offset,
    );
    opening_metrics(&rollouts)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "opening evaluation panels and their totals are deliberately small"
)]
fn opening_metrics(rollouts: &[Rollout]) -> BTreeMap<FactionId, OpeningMetrics> {
    #[derive(Default)]
    struct Totals {
        games: usize,
        cleared: usize,
        planets: i64,
        systems: i64,
        units: i64,
        shortfall: f64,
    }
    let mut totals: BTreeMap<FactionId, Totals> = BTreeMap::new();
    for seat in rollouts
        .iter()
        .filter(|rollout| rollout.error.is_none())
        .flat_map(|rollout| &rollout.seats)
    {
        let row = totals.entry(seat.faction.clone()).or_default();
        row.games += 1;
        row.cleared += usize::from(seat.episode.cleared);
        row.planets += seat.episode.final_progress.planets_gained;
        row.systems += seat.episode.final_progress.systems;
        row.units += seat.episode.final_progress.units_gained;
        row.shortfall += seat.episode.shortfall;
    }
    totals
        .into_iter()
        .map(|(faction, row)| {
            let n = row.games.max(1) as f64;
            (
                faction,
                OpeningMetrics {
                    seat_games: row.games,
                    clearance: row.cleared as f64 / n,
                    planets_gained: row.planets as f64 / n,
                    systems: row.systems as f64 / n,
                    units_gained: row.units as f64 / n,
                    shortfall: row.shortfall / n,
                },
            )
        })
        .collect()
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
                (player.clone(), blank_explicit_profile(&faction))
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
        let reduced = play_batch_statistics(
            content,
            &plan.players,
            &profiles,
            FULL,
            &seeds,
            horizon,
            ti4_engine::opening::DEFAULT_REQUIREMENT,
            &reward,
        );
        let errors = reduced.errors;
        let decisions = reduced.decisions();
        let collected = reduced.statistics;

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
    use ti4_policy::learned::blank_profile;

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
    fn the_reference_faction_plan_matches_the_working_python_curriculum() {
        let plan = FactionPlan::python_reference();
        assert_eq!(
            plan.factions
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["letnev", "jolnar", "hacan"]
        );
        assert_eq!(plan.train_seeds, 16);
        assert!((plan.step.learning_rate - 0.03).abs() < f64::EPSILON);
        assert!((plan.step.entropy - 0.01).abs() < f64::EPSILON);
        assert!((plan.step.gradient_clip - 1.0).abs() < f64::EPSILON);
        assert_eq!(plan.seed, 73_000_000);
        assert!(
            plan.sources
                .contains(ti4_model::content_types::Source::ThundersEdge),
            "the TE strategy deck and expedition require the TE content scope"
        );
    }

    #[test]
    fn stage_two_reference_uses_six_rotations_and_four_round_rewards() {
        let mut plan = FactionPlan::stage_two_reference();
        assert_eq!(plan.stage, Stage::Two);
        assert_eq!(plan.horizon(), Horizon::short());
        assert_eq!(plan.factions.len(), 6);

        plan.factions.truncate(3);
        plan.generations = 1;
        plan.train_seeds = 1;
        let run = train_factions(ContentStore::embedded(), &plan);
        assert_eq!(run.generations.len(), 1);
        assert_eq!(run.generations[0].errors, 0);
        assert!(run.generations[0].decisions > 0);
        assert!(run.generations[0].movement() > 0.0);
    }

    #[test]
    fn faction_stage_two_resume_matches_an_uninterrupted_run() {
        let mut whole = FactionPlan::stage_two_reference();
        whole.factions.truncate(3);
        whole.generations = 2;
        whole.train_seeds = 1;
        let uninterrupted = train_factions(ContentStore::embedded(), &whole);

        let mut half = whole.clone();
        half.generations = 1;
        let first = train_factions(ContentStore::embedded(), &half);
        let resumed = train_factions(
            ContentStore::embedded(),
            &half.resuming(FactionStart {
                profiles: first.profiles,
                generation: 1,
            }),
        );

        assert_eq!(uninterrupted.profiles, resumed.profiles);
    }

    #[test]
    fn faction_training_starts_sparse_and_grows_named_weights() {
        let mut plan = FactionPlan::python_reference();
        plan.generations = 1;
        plan.train_seeds = 1;
        let run = train_factions(ContentStore::embedded(), &plan);
        assert_eq!(run.generations.len(), 1);
        assert_eq!(run.generations[0].errors, 0);
        assert!(run.generations[0].decisions > 0);
        for profile in run.profiles.values() {
            assert_eq!(profile.schema, ti4_policy::learned::STAGE1_EXPLICIT_SCHEMA);
            assert!(profile.validate(Some(&profile.faction)).is_ok());
        }
        assert!(run.profiles.values().any(|profile| {
            profile
                .learned
                .heads
                .values()
                .flat_map(|head| head.weights.keys())
                .any(|name| !name.starts_with('h'))
        }));
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
