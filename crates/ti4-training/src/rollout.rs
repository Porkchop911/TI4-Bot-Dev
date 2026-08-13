//! Playing games with learned policies, and reducing them to episodes (M10-012).
//!
//! Ported from the oracle's `_rollout` in `tools/train_stage1_policy_gradient.py`.
//!
//! One rollout seats a learned policy per faction, plays a bounded game, and hands back one
//! [`Episode`] per seat: the decisions that seat took, the progress at each of them, and how the
//! opening ended. [`crate::reward::returns`] turns an episode into the number each decision is
//! credited with.
//!
//! # Bounded on purpose
//!
//! A blank policy plays uniformly at random and a random game can spend a long time achieving
//! nothing, so a rollout is capped in rounds and in steps. Stage 1 cares only about round one and
//! runs one round; Stage 2 runs a short horizon. Both are far below the length of a decided game,
//! which is the point: the training signal is what a policy produced early, not who eventually won
//! a game nobody has yet learned to win.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_engine::choice::{Observed, SeededRandom, Table};
use ti4_engine::game::Game;
use ti4_engine::opening::{DEFAULT_REQUIREMENT, Requirement};
use ti4_engine::setup::start_game_seeded;
use ti4_model::content_types::{POK, SourceSet};
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::inference::{LearnedBot, TrajectoryStep};
use ti4_policy::learned::Profile;
use ti4_policy::progress::{Baseline, Progress};

use crate::reward::Episode;

/// How far a rollout is allowed to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Horizon {
    /// Rounds to play.
    pub rounds: u32,
    /// Steps to take at most, across the whole game.
    ///
    /// A separate bound from `rounds` on purpose: a game that stopped advancing rounds would
    /// otherwise spin for ever inside one and the round limit would never be reached.
    pub steps: usize,
}

impl Horizon {
    /// One round, which is all Stage 1 measures.
    #[must_use]
    pub const fn opening() -> Self {
        Self {
            rounds: 1,
            steps: 200_000,
        }
    }

    /// A short horizon for Stage 2.
    #[must_use]
    pub const fn short() -> Self {
        Self {
            rounds: 4,
            steps: 500_000,
        }
    }
}

/// One seat's play, with the trajectory attached.
#[derive(Debug, Clone)]
pub struct SeatRollout {
    /// Which seat.
    pub player: PlayerId,
    /// The faction it played.
    pub faction: FactionId,
    /// Every decision the policy took, in order.
    pub trajectory: Vec<TrajectoryStep>,
    /// The episode the reward reads.
    pub episode: Episode,
}

/// What one played game produced.
#[derive(Debug, Clone)]
pub struct Rollout {
    /// The seed this game was played from.
    pub seed: u64,
    /// One entry per seat, in seating order.
    pub seats: Vec<SeatRollout>,
    /// The failure, if the game could not be played. Counted, never hidden: a rollout that
    /// errored and reported an empty trajectory is indistinguishable from a policy that made no
    /// decisions, and a trainer would learn from the difference between them.
    pub error: Option<String>,
}

/// Set a game up: seats, factions, a board, and starting fleets on it.
///
/// Split from [`play`] because it is a different job, and because a setup failure has to be
/// reported as one rather than as a game in which nobody decided anything.
fn seated(
    content: &ContentStore,
    players: &[PlayerId],
    sources: SourceSet,
    seed: u64,
) -> Result<
    (
        ti4_model::state::GameState,
        ti4_content::galaxy::Galaxy,
        BTreeMap<PlayerId, FactionId>,
    ),
    String,
> {
    let mut state = match start_game_seeded(content, players, sources, None, seed) {
        Ok(state) => state,
        Err(error) => return Err(format!("setup: {error}")),
    };

    let factions = ti4_engine::seating::seat_in_scope(players);
    for (player, faction) in &factions {
        if let Some(seat) = state.player_mut(player) {
            seat.faction = faction.clone();
        }
    }

    let filler: Vec<String> = ti4_engine::seating::neutral_systems(content, 30, sources)
        .into_iter()
        .map(|system| system.to_string())
        .collect();
    let borrowed: Vec<&str> = filler.iter().map(String::as_str).collect();
    let galaxy = match ti4_engine::seating::build_board(content, &factions, &borrowed, sources) {
        Ok(galaxy) => galaxy,
        Err(error) => return Err(format!("board: {error}")),
    };
    for (player, faction) in &factions {
        if let Err(error) =
            ti4_engine::seating::deploy(&mut state, content, player, faction, sources)
        {
            return Err(format!("deploy: {error}"));
        }
    }

    Ok((state, galaxy, factions))
}

/// Seat one learned profile per player and play a bounded game.
///
/// `profiles` is keyed by player. A seat with no profile plays uniformly at random, which is what
/// a blank profile does anyway — named as an absence rather than silently substituted, so a
/// missing profile is visible in a report.
#[must_use]
pub fn play(
    content: &ContentStore,
    players: &[PlayerId],
    profiles: &BTreeMap<PlayerId, Profile>,
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
    requirement: Requirement,
) -> Rollout {
    let (state, galaxy, factions) = match seated(content, players, sources, seed) {
        Ok(seated) => seated,
        Err(error) => return failed(seed, error),
    };

    // The baseline is taken *after* deployment and before the first decision. Taken any earlier it
    // is empty and every starting planet reads as a conquest; any later and the seat is credited
    // with less than it did.
    let baselines: BTreeMap<PlayerId, Baseline> = {
        let seen = Observed::new(&state, content, sources, Some(&galaxy));
        players
            .iter()
            .map(|player| (player.clone(), Baseline::taken(&seen, player)))
            .collect()
    };

    let mut table = Table::with_default(Box::new(SeededRandom::new(seed)));
    let mut handles: BTreeMap<PlayerId, std::rc::Rc<std::cell::RefCell<Vec<TrajectoryStep>>>> =
        BTreeMap::new();
    for (index, player) in players.iter().enumerate() {
        let profile = profiles.get(player).cloned().unwrap_or_else(|| {
            ti4_policy::learned::blank_profile(
                &factions
                    .get(player)
                    .map_or_else(String::new, ToString::to_string),
                ti4_policy::learned::DEFAULT_DIMENSIONS,
            )
        });
        let stream = seed
            .wrapping_mul(1_000_003)
            .wrapping_add(u64::try_from(index).unwrap_or(0));
        let bot = LearnedBot::new(profile, stream)
            .recording()
            .from_setup(baselines.get(player).copied().unwrap_or_default());
        handles.insert(player.clone(), bot.trajectory());
        table.seat(player.clone(), Box::new(bot));
    }

    let mut game = Game::with_table(state, content, table).with_galaxy(galaxy);
    let error = game
        .run(horizon.rounds, horizon.steps)
        .err()
        .map(|error| error.to_string());

    // Measured once at the end, against the same baselines, so the final snapshot and the
    // per-decision ones are the same measurement taken at different times.
    let finals: BTreeMap<PlayerId, Progress> = {
        let seen = Observed::new(&game.state, content, sources, game.galaxy());
        players
            .iter()
            .map(|player| {
                (
                    player.clone(),
                    ti4_policy::progress::measure(
                        &seen,
                        player,
                        baselines.get(player).copied().unwrap_or_default(),
                    ),
                )
            })
            .collect()
    };
    let openings = ti4_engine::opening::measure(
        &game.state,
        &baselines
            .iter()
            .map(|(player, baseline)| (player.clone(), (baseline.planets, baseline.units)))
            .collect(),
        &factions
            .values()
            .map(|faction| (faction.to_string(), requirement))
            .collect(),
    );

    let seats = players
        .iter()
        .map(|player| {
            let trajectory = handles
                .get(player)
                .map(|handle| handle.borrow().clone())
                .unwrap_or_default();
            let opening = openings.get(player);
            let steps: Vec<Progress> = trajectory.iter().map(|step| step.progress).collect();
            SeatRollout {
                player: player.clone(),
                faction: factions
                    .get(player)
                    .cloned()
                    .unwrap_or_else(|| FactionId::new("")),
                episode: Episode {
                    steps,
                    final_progress: finals.get(player).copied().unwrap_or_default(),
                    cleared: opening.is_some_and(ti4_engine::opening::Opening::cleared),
                    shortfall: opening.map_or(0.0, |opening| opening.weighted_shortfall(1.0, 1.0)),
                    traded_goods: 0.0,
                },
                trajectory,
            }
        })
        .collect();

    Rollout { seed, seats, error }
}

fn failed(seed: u64, error: String) -> Rollout {
    Rollout {
        seed,
        seats: Vec::new(),
        error: Some(error),
    }
}

/// The default opening bar.
#[must_use]
pub const fn default_requirement() -> Requirement {
    DEFAULT_REQUIREMENT
}

/// The content scope rollouts are played under.
#[must_use]
pub const fn default_sources() -> SourceSet {
    POK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seats(names: &[&str]) -> Vec<PlayerId> {
        names.iter().map(|name| PlayerId::new(*name)).collect()
    }

    fn rollout(seed: u64, horizon: Horizon) -> Rollout {
        play(
            ContentStore::embedded(),
            &seats(&["a", "b", "c"]),
            &BTreeMap::new(),
            POK,
            seed,
            horizon,
            DEFAULT_REQUIREMENT,
        )
    }

    #[test]
    fn a_rollout_produces_a_trajectory_per_seat() {
        let played = rollout(1, Horizon::opening());
        assert_eq!(played.error, None, "a seeded rollout runs clean");
        assert_eq!(played.seats.len(), 3);
        for seat in &played.seats {
            assert!(
                !seat.trajectory.is_empty(),
                "{} took no decisions at all",
                seat.player
            );
            assert_eq!(seat.episode.steps.len(), seat.trajectory.len());
        }
    }

    #[test]
    fn a_seat_is_credited_only_with_its_own_decisions() {
        // A trajectory carrying another seat's decisions would train every policy on everybody's
        // play, and every metric would still look like training. Checked by name rather than by
        // the trajectories merely differing: two seats coincidentally playing alike is unlikely
        // rather than impossible, and unlikely is what keeps catching this project out.
        let played = rollout(2, Horizon::opening());
        assert_eq!(played.seats.len(), 3);
        for seat in &played.seats {
            assert!(!seat.trajectory.is_empty());
            for step in &seat.trajectory {
                assert_eq!(
                    step.player, seat.player,
                    "{}'s trajectory carries a decision taken by {}",
                    seat.player, step.player
                );
                assert!(
                    step.probabilities.contains_key(&step.chosen),
                    "a step whose chosen option was not among the ones it scored"
                );
            }
        }
    }

    #[test]
    fn the_baseline_is_taken_after_deployment() {
        // Taken before it, every starting planet reads as a conquest and every seat clears the
        // opening bar at setup. That is the difference between a gate and a formality.
        let played = rollout(3, Horizon::opening());
        for seat in &played.seats {
            let first = seat.episode.steps.first().expect("decisions were taken");
            assert_eq!(
                first.planets_gained, 0,
                "{} was credited with its starting planets",
                seat.player
            );
        }
    }

    #[test]
    fn progress_is_stamped_at_the_decision_not_at_the_end() {
        // The reward is a difference between consecutive snapshots. Stamped late, the credit lands
        // on the wrong decision, and every step would carry the same numbers.
        let played = rollout(4, Horizon::short());
        let moving = played.seats.iter().any(|seat| {
            let mut steps = seat.episode.steps.iter();
            let Some(first) = steps.next() else {
                return false;
            };
            steps.any(|step| step != first)
        });
        assert!(moving, "every snapshot in every seat was identical");
    }

    #[test]
    fn the_same_seed_plays_the_same_rollout() {
        // Everything a trainer concludes rests on this. Without it a regression and a reseed look
        // the same.
        let once = rollout(7, Horizon::opening());
        let twice = rollout(7, Horizon::opening());
        for (a, b) in once.seats.iter().zip(&twice.seats) {
            assert_eq!(a.trajectory.len(), b.trajectory.len());
            assert_eq!(a.episode.steps, b.episode.steps);
            assert_eq!(a.episode.cleared, b.episode.cleared);
        }
    }

    #[test]
    fn different_seeds_play_different_rollouts() {
        let one = rollout(11, Horizon::opening());
        let other = rollout(12, Horizon::opening());
        let same = one
            .seats
            .iter()
            .zip(&other.seats)
            .all(|(a, b)| a.episode.steps == b.episode.steps);
        assert!(!same, "two seeds produced one rollout");
    }

    #[test]
    fn an_opening_rollout_stops_after_one_round() {
        let played = rollout(5, Horizon::opening());
        for seat in &played.seats {
            for step in &seat.episode.steps {
                assert_eq!(
                    step.round_number, 1,
                    "a decision outside round one reached a Stage-1 episode"
                );
            }
        }
    }

    #[test]
    fn a_longer_horizon_reaches_later_rounds() {
        // If it did not, Stage 2 would be Stage 1 with a different reward.
        let played = rollout(6, Horizon::short());
        let latest = played
            .seats
            .iter()
            .flat_map(|seat| seat.episode.steps.iter())
            .map(|step| step.round_number)
            .max()
            .unwrap_or(0);
        assert!(latest > 1, "the short horizon never left round one");
    }

    #[test]
    fn an_episode_from_a_rollout_produces_returns() {
        // The join this module exists for: a played game has to reduce to numbers the trainer can
        // credit decisions with.
        use crate::reward::{Reward, Stage, returns};

        let played = rollout(8, Horizon::opening());
        let seat = played.seats.first().expect("a seat played");
        let credited = returns(&seat.episode, &Reward::for_stage(Stage::One));

        assert_eq!(credited.len(), seat.trajectory.len());
        assert!(
            credited.iter().all(|value| value.is_finite()),
            "a non-finite return would poison every weight it touched"
        );
    }
}
