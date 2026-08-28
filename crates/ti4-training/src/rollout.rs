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

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_engine::choice::{Decider, Observed, SeededRandom, Table};
use ti4_engine::game::Game;
use ti4_engine::opening::{DEFAULT_REQUIREMENT, Requirement};
use ti4_engine::setup::start_game_seeded;
use ti4_model::Hex;
use ti4_model::content_types::{DEFAULT, SourceSet};
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::inference::{LearnedBot, TrajectoryStep};
use ti4_policy::learned::Profile;
use ti4_policy::progress::{Baseline, Progress};

use crate::gradient::{Statistics, statistics as collect_statistics};
use crate::reward::{Episode, Reward};

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
        Self::rounds(4)
    }

    /// A horizon of `rounds` rounds.
    ///
    /// Four rounds is the Stage-2 default and it compresses the thing the reward is trying to
    /// read: most games end tied, so the victory-point spread between policies is small and the
    /// gradient has little to separate. A longer horizon costs proportionally more compute and
    /// gives the outcomes room to differ.
    ///
    /// The step bound scales with the rounds rather than staying fixed. It exists to stop a game
    /// that has stopped advancing from spinning forever, and a bound tuned for four rounds would
    /// cut a legitimate eight-round game off partway and report it as a completed one.
    #[must_use]
    pub const fn rounds(rounds: u32) -> Self {
        Self {
            rounds,
            steps: 125_000_usize.saturating_mul(rounds as usize),
        }
    }
}

/// One seat's play, with the trajectory attached.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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

/// A training batch after trajectories have been reduced on rollout workers.
///
/// Only sufficient statistics cross the worker boundary. Evaluation continues to use
/// [`Rollout`], while training avoids retaining and then serially revisiting every legal option
/// and feature vector.
#[derive(Debug, Clone, PartialEq)]
pub struct ReducedBatch<K: Ord> {
    /// Games that failed before producing usable statistics.
    pub errors: usize,
    /// Statistics keyed by the policy identity being updated, then decision head.
    pub statistics: BTreeMap<K, BTreeMap<String, Statistics>>,
}

impl<K: Ord> Default for ReducedBatch<K> {
    fn default() -> Self {
        Self {
            errors: 0,
            statistics: BTreeMap::new(),
        }
    }
}

impl<K: Ord> ReducedBatch<K> {
    /// Decisions represented by this batch's sufficient statistics.
    #[must_use]
    pub fn decisions(&self) -> usize {
        self.statistics
            .values()
            .flat_map(BTreeMap::values)
            .map(|row| row.actions)
            .sum()
    }
}

fn merge_partials<K>(partials: &[ReducedBatch<K>]) -> ReducedBatch<K>
where
    K: Ord + Clone + Send + Sync,
{
    let errors = partials.iter().map(|partial| partial.errors).sum();
    let pairs: BTreeSet<(K, String)> = partials
        .iter()
        .flat_map(|partial| {
            partial
                .statistics
                .iter()
                .flat_map(|(key, rows)| rows.keys().map(|head| (key.clone(), head.clone())))
        })
        .collect();
    let merged: Vec<(K, String, Statistics)> = pairs
        .par_iter()
        .map(|(key, head)| {
            let mut row = Statistics::default();
            for partial in partials {
                if let Some(found) = partial.statistics.get(key).and_then(|rows| rows.get(head)) {
                    row.merge(found);
                }
            }
            (key.clone(), head.clone(), row)
        })
        .collect();
    let mut statistics: BTreeMap<K, BTreeMap<String, Statistics>> = BTreeMap::new();
    for (key, head, row) in merged {
        statistics.entry(key).or_default().insert(head, row);
    }
    ReducedBatch { errors, statistics }
}

/// Whether the faction-to-seat assignment scrambles its cyclic order per seed.
///
/// Off by default, which reproduces every checkpoint and parity fixture in the repository.
static SCRAMBLE_SEATS: AtomicBool = AtomicBool::new(false);

/// Draw each seed's cyclic seating order at random instead of always using the caller's order.
///
/// **What was wrong with the default.** The assignment `factions[(seat + rotation) % n]` is a
/// cyclic rotation, so the offset between any two factions never changes and only the cut moves.
/// That balances what it looks like it balances -- every faction takes every seat once, is speaker
/// once, and occupies every map slot once -- and silently fails to balance two things:
///
/// * **draft precedence.** For factions at cyclic distance `d`, the first drafts before the second
///   in `(n-d)/n` of rotations. Measured on six factions: 83.3% at d=1, 50% only at d=3, 16.7% at
///   d=5. Six of thirty ordered pairs are fair and the rest are not.
/// * **who borders whom.** Ring-neighbours are the adjacent indices, so with the shipped faction
///   list Sol borders Letnev and L1Z1X in *every game ever played*.
///
/// Both matter: two factions that want the same strategy card resolve it by a fixed precedence,
/// and the loser takes a fallback that may be worthless.
///
/// **What this does instead.** One permutation per seed, then the same rotation within the seed.
/// The rotation is kept deliberately -- it is what makes each faction play each seat exactly once
/// per seed, which is the design's variance reduction and is worth preserving. Only the order
/// being rotated becomes a function of the seed, so across a training stream every cyclic order
/// appears, precedence averages to even, and neighbours vary.
pub fn set_seat_scramble(enabled: bool) {
    SCRAMBLE_SEATS.store(enabled, Ordering::Relaxed);
}

/// Whether seat scrambling is on. Reported by trainers so a run's log states which it used.
#[must_use]
pub fn seat_scramble() -> bool {
    SCRAMBLE_SEATS.load(Ordering::Relaxed)
}

/// The faction seated at `seat` for this `seed` and `rotation`.
fn seated_faction(factions: &[FactionId], seed: u64, rotation: usize, seat: usize) -> FactionId {
    let count = factions.len();
    if count == 0 {
        return FactionId::new("");
    }
    if !seat_scramble() {
        return factions[(seat + rotation) % count].clone();
    }
    // Fisher-Yates over a seed-derived stream. The constant keeps this stream distinct from the
    // deck and sampling streams the same seed already drives, so seating does not move in lockstep
    // with them.
    let mut order: Vec<FactionId> = factions.to_vec();
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    for index in (1..count).rev() {
        // SplitMix64, so the permutation needs no rng dependency and stays reproducible.
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        #[expect(clippy::cast_possible_truncation, reason = "modulo a small index")]
        let pick = (z % (index as u64 + 1)) as usize;
        order.swap(index, pick);
    }
    order[(seat + rotation) % count].clone()
}

/// Board family used by a parity rollout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpeningMap {
    /// The Rust seeded partial-spiral board used by the ordinary trainer.
    RustVaried,
    /// The captured Python Save-54 tile geometry, with faction homes rotated by physical seat.
    Save54Captured,
    /// A Python-compatible constrained arrangement selected from a validated map pool.
    PythonPool {
        pool: Arc<ti4_sim::MapPool>,
        tile_seed_offset: u64,
    },
}

const SAVE54_NEUTRAL: [(&str, i32, i32); 25] = [
    ("18", 0, 0),
    ("72", -1, 0),
    ("49", -1, 1),
    ("48", 0, -1),
    ("31", 0, 1),
    ("74", 1, -1),
    ("46", 1, 0),
    ("71", -2, 0),
    ("62", -2, 1),
    ("70", -2, 2),
    ("44", -1, -1),
    ("77", -1, 2),
    ("36", 0, -2),
    ("69", 0, 2),
    ("63", 1, -2),
    ("24", 1, 1),
    ("35", 2, -2),
    ("41", 2, -1),
    ("28", 2, 0),
    ("25", -3, 2),
    ("79", -2, 3),
    ("40", -1, -2),
    ("26", 1, -3),
    ("64", 2, 1),
    ("39", 3, -1),
];

const SAVE54_HOMES: [Hex; 3] = [Hex::new(0, -3), Hex::new(-3, 3), Hex::new(3, 0)];

fn save54_board(
    content: &ContentStore,
    players: &[PlayerId],
    factions: &BTreeMap<PlayerId, FactionId>,
    sources: SourceSet,
) -> Result<ti4_content::galaxy::Galaxy, String> {
    if players.len() != SAVE54_HOMES.len() {
        return Err(format!("Save 54 needs 3 seats, got {}", players.len()));
    }
    let mut owned: Vec<(String, Hex)> = SAVE54_NEUTRAL
        .iter()
        .map(|(id, q, r)| ((*id).to_owned(), Hex::new(*q, *r)))
        .collect();
    for ((player, hex), _) in players.iter().zip(SAVE54_HOMES).zip(0..) {
        let faction = factions
            .get(player)
            .ok_or_else(|| format!("no faction assigned to {player}"))?;
        let home = ti4_content::factions::get(content, faction.as_str())
            .and_then(|record| record.home_system())
            .ok_or_else(|| format!("faction {faction} has no home system"))?;
        owned.push((home.to_owned(), hex));
    }
    let borrowed: Vec<(&str, Hex)> = owned.iter().map(|(id, hex)| (id.as_str(), *hex)).collect();
    ti4_content::galaxy::Galaxy::placed(content, &borrowed, sources)
        .map_err(|error| format!("Save 54 board: {error}"))
}

/// Set a game up: seats, factions, a board, and starting fleets on it.
///
/// Split from [`play`] because it is a different job, and because a setup failure has to be
/// reported as one rather than as a game in which nobody decided anything.
fn seated(
    content: &ContentStore,
    players: &[PlayerId],
    factions: &BTreeMap<PlayerId, FactionId>,
    sources: SourceSet,
    seed: u64,
    map: &OpeningMap,
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

    for (player, faction) in factions {
        if let Some(seat) = state.player_mut(player) {
            seat.faction = faction.clone();
        }
    }
    // G1 (T6b): the setup deal ran before seating, so every note id read "generic". Re-deal
    // once the seats know who they are; no note has moved yet, so this is a clean refresh.
    ti4_engine::promissory::deal(&mut state, content, sources);

    // Drawn by seed, so a batch plays many boards rather than one. A policy trained on a single
    // map learns that map, and no batch report would say so.
    let galaxy = match map {
        OpeningMap::RustVaried => {
            let filler: Vec<String> = ti4_engine::seating::map_filler(content, 30, sources, seed)
                .into_iter()
                .map(|system| system.to_string())
                .collect();
            let borrowed: Vec<&str> = filler.iter().map(String::as_str).collect();
            ti4_engine::seating::build_board(content, factions, &borrowed, sources)
                .map_err(|error| format!("board: {error}"))?
        }
        OpeningMap::Save54Captured => save54_board(content, players, factions, sources)?,
        OpeningMap::PythonPool {
            pool,
            tile_seed_offset,
        } => {
            let homes: Result<Vec<String>, String> = players
                .iter()
                .map(|player| {
                    let faction = factions
                        .get(player)
                        .ok_or_else(|| format!("no faction assigned to {player}"))?;
                    ti4_content::factions::get(content, faction.as_str())
                        .and_then(|record| record.home_system())
                        .map(str::to_owned)
                        .ok_or_else(|| format!("faction {faction} has no home system"))
                })
                .collect();
            let homes = homes?;
            let borrowed: Vec<&str> = homes.iter().map(String::as_str).collect();
            pool.galaxy(
                content,
                sources,
                seed.wrapping_add(*tile_seed_offset),
                &borrowed,
            )
            .map_err(|error| format!("Python map pool: {error}"))?
        }
    };
    for (player, faction) in factions {
        if let Err(error) =
            ti4_engine::seating::deploy(&mut state, content, player, faction, sources)
        {
            return Err(format!("deploy: {error}"));
        }
    }

    Ok((state, galaxy, factions.clone()))
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
    let factions = ti4_engine::seating::seat_in_scope(players);
    play_assigned(
        content,
        players,
        &factions,
        profiles,
        sources,
        seed,
        horizon,
        requirement,
    )
}

fn play_shared(
    content: &ContentStore,
    players: &[PlayerId],
    profiles: &BTreeMap<PlayerId, Arc<Profile>>,
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
    requirement: Requirement,
) -> Rollout {
    let factions = ti4_engine::seating::seat_in_scope(players);
    play_assigned_on_map_shared(
        content,
        players,
        &factions,
        profiles,
        sources,
        seed,
        horizon,
        requirement,
        &OpeningMap::RustVaried,
        true,
    )
}

/// Play with an explicit physical-seat to faction assignment.
///
/// This is the primitive rotations require: policy identity follows the faction while the
/// assignment moves around fixed physical seats. The legacy [`play`] wrapper retains the stable
/// in-scope assignment for existing callers.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "a rollout's complete deterministic input"
)]
pub fn play_assigned(
    content: &ContentStore,
    players: &[PlayerId],
    factions: &BTreeMap<PlayerId, FactionId>,
    profiles: &BTreeMap<PlayerId, Profile>,
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
    requirement: Requirement,
) -> Rollout {
    play_assigned_on_map(
        content,
        players,
        factions,
        profiles,
        sources,
        seed,
        horizon,
        requirement,
        &OpeningMap::RustVaried,
    )
}

/// Play an explicitly assigned rollout on a selected parity board family.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "a rollout's complete deterministic input"
)]
pub fn play_assigned_on_map(
    content: &ContentStore,
    players: &[PlayerId],
    factions: &BTreeMap<PlayerId, FactionId>,
    profiles: &BTreeMap<PlayerId, Profile>,
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
    requirement: Requirement,
    map: &OpeningMap,
) -> Rollout {
    let shared = profiles
        .iter()
        .map(|(player, profile)| (player.clone(), Arc::new(profile.clone())))
        .collect();
    play_assigned_on_map_shared(
        content,
        players,
        factions,
        &shared,
        sources,
        seed,
        horizon,
        requirement,
        map,
        true,
    )
}

/// Play one game with caller-provided deciders, one per player.
///
/// Deliberately additive: the learned path keeps constructing its own recording bots; this entry
/// point exists so a diagnostic (for example a single-game decision trace) can seat its own
/// [`Decider`] wrappers without changing how training games are played. The returned rollout's
/// per-seat trajectories are empty unless the deciders themselves record them.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "a rollout's complete deterministic input"
)]
pub fn play_with_deciders(
    content: &ContentStore,
    players: &[PlayerId],
    factions: &BTreeMap<PlayerId, FactionId>,
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
    requirement: Requirement,
    map: &OpeningMap,
    deciders: BTreeMap<PlayerId, Box<dyn Decider>>,
) -> Rollout {
    let (state, galaxy, factions) = match seated(content, players, factions, sources, seed, map) {
        Ok(seated) => seated,
        Err(error) => return failed(seed, error),
    };
    finish_game(
        content,
        state,
        galaxy,
        &factions,
        players,
        sources,
        seed,
        horizon,
        requirement,
        deciders,
        None,
    )
}

/// Construct an unadvanced rollout game with deciders created from exact setup baselines.
///
/// This is the interactive counterpart of [`play_with_decider_factory`]. The factory runs after
/// deployment, so learned bots receive the same baseline facts as training bots, but the caller
/// retains the game and decides when each engine step occurs.
///
/// # Errors
/// Returns setup, map, deployment, factory, missing-seat, or unknown-seat failures before a game
/// is returned. No engine step is attempted on failure.
pub fn setup_game_with_decider_factory<'a, F>(
    content: &'a ContentStore,
    players: &[PlayerId],
    factions: &BTreeMap<PlayerId, FactionId>,
    sources: SourceSet,
    seed: u64,
    map: &OpeningMap,
    factory: F,
) -> Result<Game<'a>, String>
where
    F: FnOnce(
        &BTreeMap<PlayerId, Baseline>,
    ) -> Result<BTreeMap<PlayerId, Box<dyn Decider>>, String>,
{
    let (state, galaxy, _) = seated(content, players, factions, sources, seed, map)?;
    let baselines = opening_baselines(&state, content, sources, Some(&galaxy), players);
    let deciders = factory(&baselines)?;
    let mut table = Table::with_default(Box::new(SeededRandom::new(seed)));
    for player in players {
        let decider = deciders
            .get(player)
            .ok_or_else(|| format!("no decider seated for {player}"));
        if decider.is_err() {
            return Err(format!("no decider seated for {player}"));
        }
    }
    if let Some(extra) = deciders.keys().find(|player| !players.contains(player)) {
        return Err(format!("decider supplied for unknown seat {extra}"));
    }
    for (player, decider) in deciders {
        table.seat(player, decider);
    }
    Ok(Game::with_table(state, content, table)
        .with_sources(sources)
        .with_galaxy(galaxy))
}

/// Play one game with deciders constructed after deployment from the exact setup baselines.
///
/// A policy that records shaped per-decision returns must measure every snapshot against the same
/// post-deployment baseline used for the rollout's final progress. Constructing deciders before
/// [`seated`] cannot provide that value; guessing it from the first later decision can already be
/// too late. This factory boundary exposes only the baselines, not mutable or omniscient state.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "a rollout's complete deterministic input"
)]
pub fn play_with_decider_factory<F>(
    content: &ContentStore,
    players: &[PlayerId],
    factions: &BTreeMap<PlayerId, FactionId>,
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
    requirement: Requirement,
    map: &OpeningMap,
    factory: F,
) -> Rollout
where
    F: FnOnce(
        &BTreeMap<PlayerId, Baseline>,
    ) -> Result<BTreeMap<PlayerId, Box<dyn Decider>>, String>,
{
    let (state, galaxy, factions) = match seated(content, players, factions, sources, seed, map) {
        Ok(seated) => seated,
        Err(error) => return failed(seed, error),
    };
    let baselines = opening_baselines(&state, content, sources, Some(&galaxy), players);
    let deciders = match factory(&baselines) {
        Ok(deciders) => deciders,
        Err(error) => return failed(seed, format!("constructing deciders: {error}")),
    };
    finish_game(
        content,
        state,
        galaxy,
        &factions,
        players,
        sources,
        seed,
        horizon,
        requirement,
        deciders,
        None,
    )
}

/// Play one game recording both the trajectory and the option-free critic vector per decision.
///
/// M10-031's capture path. Separate from [`play_assigned_on_map_shared`] rather than another
/// boolean on it, because it returns something extra and only one caller wants it.
///
/// The returned map is keyed by player and aligned index-for-index with that seat's trajectory:
/// `LearnedBot` pushes to both buffers under the same branch, so decision `i` in one is decision
/// `i` in the other.
#[must_use]
/// What one seat's capture produced beyond its trajectory.
///
/// Both buffers are pushed under the same `recording` branch as the trajectory, so index `i` in
/// each is decision `i` in the others.
#[derive(Debug, Clone, Default)]
pub struct SeatCapture {
    /// The option-free critic vector at each decision.
    pub critic: Vec<ti4_policy::features::FeatureVector>,
    /// The projected per-option vectors the MLP consumes, with their option ids, at each decision.
    ///
    /// **Not** `TrajectoryStep::legal`, which holds the raw schema-4 features the linear policy
    /// scores with. The projection drops the unbounded `state-option:`/`prompt-option:` crosses and
    /// adds the bare `seat-state:` facts, so a corpus built from `legal` trains an MLP on inputs it
    /// never sees at inference.
    pub projected: ti4_policy::inference::ProjectedOptions,
}

pub fn play_capturing(
    content: &ContentStore,
    players: &[PlayerId],
    factions: &BTreeMap<PlayerId, FactionId>,
    profiles: &BTreeMap<PlayerId, Arc<Profile>>,
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
    requirement: Requirement,
    map: &OpeningMap,
    critic: ti4_policy::critic::CriticFeatures,
) -> (Rollout, BTreeMap<PlayerId, SeatCapture>) {
    let (state, galaxy, factions) = match seated(content, players, factions, sources, seed, map) {
        Ok(seated) => seated,
        Err(error) => return (failed(seed, error), BTreeMap::new()),
    };
    let baselines = opening_baselines(&state, content, sources, Some(&galaxy), players);

    let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
    let mut handles: BTreeMap<PlayerId, std::rc::Rc<std::cell::RefCell<Vec<TrajectoryStep>>>> =
        BTreeMap::new();
    let mut critic_handles: BTreeMap<
        PlayerId,
        std::rc::Rc<std::cell::RefCell<Vec<ti4_policy::features::FeatureVector>>>,
    > = BTreeMap::new();
    let mut projected_handles: BTreeMap<
        PlayerId,
        std::rc::Rc<std::cell::RefCell<ti4_policy::inference::ProjectedOptions>>,
    > = BTreeMap::new();
    for (index, player) in players.iter().enumerate() {
        let profile = profiles.get(player).cloned().unwrap_or_else(|| {
            Arc::new(ti4_policy::learned::blank_explicit_profile(
                &factions
                    .get(player)
                    .map_or_else(String::new, ToString::to_string),
            ))
        });
        let stream = seed
            .wrapping_mul(1_000_003)
            .wrapping_add(u64::try_from(index).unwrap_or(0));
        let bot = LearnedBot::from_shared(profile, stream)
            .recording()
            .recording_critic(critic)
            .recording_projected()
            .from_setup(baselines.get(player).copied().unwrap_or_default());
        handles.insert(player.clone(), bot.trajectory());
        critic_handles.insert(player.clone(), bot.critic_vectors());
        projected_handles.insert(player.clone(), bot.projected_vectors());
        deciders.insert(player.clone(), Box::new(bot));
    }

    let rollout = finish_game(
        content,
        state,
        galaxy,
        &factions,
        players,
        sources,
        seed,
        horizon,
        requirement,
        deciders,
        Some(&handles),
    );
    let captured = critic_handles
        .into_iter()
        .map(|(player, handle)| {
            let projected = projected_handles
                .get(&player)
                .map(|handle| handle.borrow().clone())
                .unwrap_or_default();
            (
                player,
                SeatCapture {
                    critic: handle.borrow().clone(),
                    projected,
                },
            )
        })
        .collect();
    (rollout, captured)
}

fn play_assigned_on_map_shared(
    content: &ContentStore,
    players: &[PlayerId],
    factions: &BTreeMap<PlayerId, FactionId>,
    profiles: &BTreeMap<PlayerId, Arc<Profile>>,
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
    requirement: Requirement,
    map: &OpeningMap,
    record_trajectories: bool,
) -> Rollout {
    let (state, galaxy, factions) = match seated(content, players, factions, sources, seed, map) {
        Ok(seated) => seated,
        Err(error) => return failed(seed, error),
    };

    // The baseline is taken *after* deployment and before the first decision.
    let baselines = opening_baselines(&state, content, sources, Some(&galaxy), players);

    let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
    let mut handles: BTreeMap<PlayerId, std::rc::Rc<std::cell::RefCell<Vec<TrajectoryStep>>>> =
        BTreeMap::new();
    for (index, player) in players.iter().enumerate() {
        let profile = profiles.get(player).cloned().unwrap_or_else(|| {
            Arc::new(ti4_policy::learned::blank_explicit_profile(
                &factions
                    .get(player)
                    .map_or_else(String::new, ToString::to_string),
            ))
        });
        let stream = seed
            .wrapping_mul(1_000_003)
            .wrapping_add(u64::try_from(index).unwrap_or(0));
        // Recording is opt-in: evaluation panels only need final progress and opening
        // measurements, and retaining every decision's feature vectors for a whole panel
        // costs gigabytes of allocation that are then freed serially on the main thread.
        let mut bot = LearnedBot::from_shared(profile, stream);
        if record_trajectories {
            bot = bot.recording();
        }
        bot = bot.from_setup(baselines.get(player).copied().unwrap_or_default());
        if record_trajectories {
            handles.insert(player.clone(), bot.trajectory());
        }
        deciders.insert(player.clone(), Box::new(bot));
    }

    finish_game(
        content,
        state,
        galaxy,
        &factions,
        players,
        sources,
        seed,
        horizon,
        requirement,
        deciders,
        if record_trajectories {
            Some(&handles)
        } else {
            None
        },
    )
}

/// What each seat held at setup: taken after deployment and before the first decision. Taken any
/// earlier it is empty and every starting planet reads as a conquest; any later and the seat is
/// credited with less than it did.
fn opening_baselines(
    state: &ti4_model::state::GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    players: &[PlayerId],
) -> BTreeMap<PlayerId, Baseline> {
    let seen = Observed::new(state, content, sources, galaxy);
    players
        .iter()
        .map(|player| (player.clone(), Baseline::taken(&seen, player)))
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "a rollout's complete deterministic input"
)]
fn finish_game(
    content: &ContentStore,
    state: ti4_model::state::GameState,
    galaxy: ti4_content::galaxy::Galaxy,
    factions: &BTreeMap<PlayerId, FactionId>,
    players: &[PlayerId],
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
    requirement: Requirement,
    mut deciders: BTreeMap<PlayerId, Box<dyn Decider>>,
    handles: Option<&BTreeMap<PlayerId, std::rc::Rc<std::cell::RefCell<Vec<TrajectoryStep>>>>>,
) -> Rollout {
    let baselines = opening_baselines(&state, content, sources, Some(&galaxy), players);

    let mut table = Table::with_default(Box::new(SeededRandom::new(seed)));
    for player in players {
        match deciders.remove(player) {
            Some(decider) => table.seat(player.clone(), decider),
            None => return failed(seed, format!("no decider seated for {player}")),
        }
    }

    let mut game = Game::with_table(state, content, table)
        .with_sources(sources)
        .with_galaxy(galaxy);

    // Round one is run on its own so the opening can be measured where the opening ends.
    //
    // `opening::measure` compares a state against the setup snapshot and has no notion of a round,
    // so measuring it after the horizon answered "did this seat ever reach the bar", not "did its
    // opening clear". Under a four-round horizon those differ by about sixteen points, and the
    // second is what `Episode::cleared` is read as everywhere: Stage 1 pays `clear_bonus` for it,
    // Stage 2 credits `r1_bonus` for it at the last round-one decision on the stated grounds that
    // "a round-three decision cannot change whether round one cleared" — which was not true of the
    // quantity being computed. See `plans/M10-034_CLEARANCE_MEASUREMENT_FINDING.md`.
    //
    // `Game::run` targets `state.round + rounds`, so this is the same game played in two calls, not
    // two games. `horizon.steps` guards each call: it is a runaway limit rather than a budget, and
    // a real four-round game uses a small fraction of it.
    let opening_error = game.run(1, horizon.steps).err().map(|e| e.to_string());
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
        content,
        sources,
    );

    // The remaining rounds. A game that already finished or errored in round one runs no further,
    // and keeps the opening that was measured before it stopped.
    let error = opening_error.or_else(|| {
        if horizon.rounds <= 1 || game.state.finished {
            return None;
        }
        game.run(horizon.rounds - 1, horizon.steps)
            .err()
            .map(|error| error.to_string())
    });

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

    let seats = players
        .iter()
        .map(|player| {
            let trajectory = handles
                .and_then(|handles| handles.get(player))
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

/// Play one game and keep what a mechanics audit needs: the events emitted and the final state.
///
/// The ordinary rollout path keeps neither, which is right for training -- a batch of 96 games has
/// no use for either -- but it left the only check that a subsystem is reachable a source-text one
/// asserting the driver still contains a call to it. That check passed for the agenda phase the
/// whole time no simulated game ever ran one, because nothing lifted the custodians token. Wiring
/// is not reachability, and this is how to tell them apart.
///
/// # Panics
/// Panics if seating cannot be built for `seed`, which would mean a broken map pool.
#[must_use]
pub fn audit_game(
    content: &'static ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
    map: &OpeningMap,
) -> (Vec<String>, ti4_model::state::GameState) {
    let players: Vec<PlayerId> = (0..factions.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let wanted: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(seat, player)| (player.clone(), seated_faction(factions, seed, 0, seat)))
        .collect();
    let (state, galaxy, assignments) =
        seated(content, &players, &wanted, sources, seed, map).expect("seating");

    let mut table = Table::with_default(Box::new(SeededRandom::new(seed)));
    for (index, player) in players.iter().enumerate() {
        let faction = assignments
            .get(player)
            .cloned()
            .unwrap_or_else(|| FactionId::new(""));
        let profile = profiles
            .get(&faction)
            .cloned()
            .unwrap_or_else(|| ti4_policy::learned::blank_explicit_profile(faction.as_str()));
        let stream = seed
            .wrapping_mul(1_000_003)
            .wrapping_add(u64::try_from(index).unwrap_or(0));
        table.seat(
            player.clone(),
            Box::new(LearnedBot::from_shared(Arc::new(profile), stream)),
        );
    }
    let mut game = Game::with_table(state, content, table)
        .with_sources(sources)
        .with_galaxy(galaxy);
    let _ = game.run(horizon.rounds, horizon.steps);
    (game.events.clone(), game.state.clone())
}

/// What one audited game yields.
///
/// Two states, because they answer different questions. The first is taken when the strategy phase
/// ends, so the strategy cards are still in hand; the last is the end of the horizon, where the
/// openings are measured and where a seat's forces have finished moving.
pub type Audited = (
    Vec<String>,
    ti4_model::state::GameState,
    BTreeMap<PlayerId, FactionId>,
    BTreeMap<PlayerId, ti4_engine::opening::Opening>,
    ti4_model::state::GameState,
);

/// [`audit_game`] for a policy that is not a [`Profile`].
///
/// The MLP seats a `Box<dyn Decider>` built against a bundle rather than a shared profile, so it
/// cannot use the profile-keyed path above. This exists for the same reason that one does: the
/// training rollout keeps neither the event log nor the final state, which is correct for a batch
/// of 96 games and useless for asking what the policy actually *did* — which strategy card each
/// faction takes, how often it follows a secondary.
///
/// Returns the event log, the final state, and the seat-to-faction assignment, because the first
/// two are meaningless for a per-faction report without the third.
///
/// # Errors
/// Returns an error when seating fails or the factory refuses.
pub fn audit_game_with_deciders<F>(
    content: &'static ContentStore,
    factions: &[FactionId],
    sources: SourceSet,
    seed: u64,
    rotation: usize,
    horizon: Horizon,
    map: &OpeningMap,
    factory: F,
) -> Result<Audited, String>
where
    F: FnOnce(
        &BTreeMap<PlayerId, FactionId>,
        &BTreeMap<PlayerId, Baseline>,
    ) -> Result<BTreeMap<PlayerId, Box<dyn Decider>>, String>,
{
    let players: Vec<PlayerId> = (0..factions.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let wanted: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(seat, player)| {
            (
                player.clone(),
                seated_faction(factions, seed, rotation, seat),
            )
        })
        .collect();
    let (state, galaxy, assignments) = seated(content, &players, &wanted, sources, seed, map)
        .map_err(|error| format!("seating {seed}/{rotation}: {error}"))?;

    // The setup baselines go to the factory as well as the assignment. Every opening-progress
    // feature is a delta against them, so a bot seated without one reports absolute holdings where
    // it was trained on gains -- an evaluation that silently measures a different model than the
    // one under test.
    let baselines = opening_baselines(&state, content, sources, Some(&galaxy), &players);
    let mut deciders = factory(&assignments, &baselines)?;
    let mut table = Table::with_default(Box::new(SeededRandom::new(seed)));
    for player in &players {
        match deciders.remove(player) {
            Some(decider) => table.seat(player.clone(), decider),
            None => return Err(format!("no decider seated for {player}")),
        }
    }

    // Taken before the game runs: every opening delta is measured against it, and a caller that
    // forgot it would get absolute holdings reported as gains.
    let baseline = ti4_engine::opening::snapshot(&state);

    let mut game = Game::with_table(state, content, table)
        .with_sources(sources)
        .with_galaxy(galaxy);

    // Stepped rather than `run`, to snapshot the state the moment the strategy phase ends.
    //
    // Strategy cards are returned to the common pool in the status phase, so a seat's
    // `strategy_cards` is empty by the end of round one and an end-of-game snapshot answers "which
    // card does this faction hold now", which is always "none". The pick itself is only visible
    // between the strategy phase and the status phase that closes the round.
    let mut after_strategy: Option<ti4_model::state::GameState> = None;
    let target = game.state.round.saturating_add(horizon.rounds);
    let mut steps = 0usize;
    while game.state.round < target && !game.state.finished && steps < horizon.steps {
        let was_strategy = game.state.phase == ti4_model::state::Phase::Strategy;
        let result = game.step();
        if result.error.is_some() {
            break;
        }
        if was_strategy
            && game.state.phase != ti4_model::state::Phase::Strategy
            && after_strategy.is_none()
        {
            after_strategy = Some(game.state.clone());
        }
        steps += 1;
    }

    // A game that never left the strategy phase has no pick to report; the final state is the
    // honest fallback rather than a fabricated one.
    let picks = after_strategy.unwrap_or_else(|| game.state.clone());
    let openings = ti4_engine::opening::measure(
        &game.state,
        &baseline,
        &assignments
            .values()
            .map(|faction| {
                (
                    faction.to_string(),
                    ti4_engine::opening::DEFAULT_REQUIREMENT,
                )
            })
            .collect(),
        content,
        sources,
    );
    Ok((
        game.events.clone(),
        picks,
        assignments,
        openings,
        game.state.clone(),
    ))
}

/// Play every faction in every physical seat on every seed.
///
/// Profiles are keyed by faction, not by seat. Each seed therefore yields `factions.len()` games
/// sharing one varied map draw, exactly the counterbalance used by the Python trainer.
fn play_rotated_on_map_batch(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
    map: &OpeningMap,
    record_trajectories: bool,
) -> Vec<Rollout> {
    if seeds.is_empty() || factions.is_empty() {
        return Vec::new();
    }
    let players: Vec<PlayerId> = (0..factions.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let shared_profiles: BTreeMap<FactionId, Arc<Profile>> = profiles
        .iter()
        .map(|(faction, profile)| (faction.clone(), Arc::new(profile.clone())))
        .collect();
    let jobs: Vec<(u64, usize)> = seeds
        .iter()
        .flat_map(|seed| (0..factions.len()).map(move |rotation| (*seed, rotation)))
        .collect();

    jobs.par_iter()
        .map(|(seed, rotation)| {
            let assignments: BTreeMap<PlayerId, FactionId> = players
                .iter()
                .enumerate()
                .map(|(seat, player)| {
                    (
                        player.clone(),
                        seated_faction(factions, *seed, *rotation, seat),
                    )
                })
                .collect();
            let seated_profiles = assignments
                .iter()
                .filter_map(|(player, faction)| {
                    shared_profiles
                        .get(faction)
                        .cloned()
                        .map(|profile| (player.clone(), profile))
                })
                .collect();
            play_assigned_on_map_shared(
                content,
                &players,
                &assignments,
                &seated_profiles,
                sources,
                *seed,
                horizon,
                requirement,
                map,
                record_trajectories,
            )
        })
        .collect()
}

#[must_use]
pub fn play_rotated_batch(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
) -> Vec<Rollout> {
    play_rotated_on_map_batch(
        content,
        factions,
        profiles,
        sources,
        seeds,
        horizon,
        requirement,
        &OpeningMap::RustVaried,
        true,
    )
}

/// Play every faction in every Save-54 physical seat for each seed.
#[must_use]
pub fn play_rotated_save54_batch(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
) -> Vec<Rollout> {
    play_rotated_on_map_batch(
        content,
        factions,
        profiles,
        sources,
        seeds,
        horizon,
        requirement,
        &OpeningMap::Save54Captured,
        true,
    )
}

/// Play every faction in every physical Save-54 seat on Python-compatible pooled maps.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "a differential panel's complete deterministic input"
)]
pub fn play_rotated_save54_pool_batch(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
    pool: Arc<ti4_sim::MapPool>,
    tile_seed_offset: u64,
) -> Vec<Rollout> {
    play_rotated_save54_pool_batch_with_workers(
        content,
        factions,
        profiles,
        sources,
        seeds,
        horizon,
        requirement,
        pool,
        tile_seed_offset,
        0,
        true,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "the worker-count seam makes deterministic parallelism directly testable"
)]
fn play_rotated_save54_pool_batch_with_workers(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
    pool: Arc<ti4_sim::MapPool>,
    tile_seed_offset: u64,
    workers: usize,
    record_trajectories: bool,
) -> Vec<Rollout> {
    if seeds.is_empty() || factions.is_empty() {
        return Vec::new();
    }
    let players: Vec<PlayerId> = (0..factions.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let map = OpeningMap::PythonPool {
        pool,
        tile_seed_offset,
    };
    let shared_profiles: BTreeMap<FactionId, Arc<Profile>> = profiles
        .iter()
        .map(|(faction, profile)| (faction.clone(), Arc::new(profile.clone())))
        .collect();
    let jobs: Vec<(usize, u64, usize)> = seeds
        .iter()
        .flat_map(|seed| (0..factions.len()).map(move |rotation| (*seed, rotation)))
        .enumerate()
        .map(|(index, (seed, rotation))| (index, seed, rotation))
        .collect();
    let execute = || {
        jobs.par_iter()
            .map(|(index, seed, rotation)| {
                let assignments: BTreeMap<PlayerId, FactionId> = players
                    .iter()
                    .enumerate()
                    .map(|(seat, player)| {
                        (
                            player.clone(),
                            seated_faction(factions, *seed, *rotation, seat),
                        )
                    })
                    .collect();
                let seated_profiles = assignments
                    .iter()
                    .filter_map(|(player, faction)| {
                        shared_profiles
                            .get(faction)
                            .cloned()
                            .map(|profile| (player.clone(), profile))
                    })
                    .collect();
                (
                    *index,
                    play_assigned_on_map_shared(
                        content,
                        &players,
                        &assignments,
                        &seated_profiles,
                        sources,
                        *seed,
                        horizon,
                        requirement,
                        &map,
                        record_trajectories,
                    ),
                )
            })
            .collect::<Vec<_>>()
    };
    let mut indexed = if workers == 0 {
        execute()
    } else {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .expect("test worker pool is valid")
            .install(execute)
    };
    indexed.sort_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, rollout)| rollout).collect()
}

/// Evaluation-only variant of [`play_rotated_batch`]: plays the identical games but does not
/// record per-decision trajectories, so a panel that only needs final metrics neither retains
/// nor serially frees gigabytes of feature vectors. Final progress and opening measurements are
/// identical to the recording variant for a given seed.
#[must_use]
pub fn play_rotated_batch_evaluation(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
) -> Vec<Rollout> {
    play_rotated_on_map_batch(
        content,
        factions,
        profiles,
        sources,
        seeds,
        horizon,
        requirement,
        &OpeningMap::RustVaried,
        false,
    )
}

/// Evaluation-only variant of [`play_rotated_save54_pool_batch`]. See
/// [`play_rotated_batch_evaluation`] for what is and is not retained.
#[must_use]
pub fn play_rotated_save54_pool_batch_evaluation(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
    pool: Arc<ti4_sim::MapPool>,
    tile_seed_offset: u64,
) -> Vec<Rollout> {
    play_rotated_save54_pool_batch_with_workers(
        content,
        factions,
        profiles,
        sources,
        seeds,
        horizon,
        requirement,
        pool,
        tile_seed_offset,
        0,
        false,
    )
}

fn failed(seed: u64, error: String) -> Rollout {
    Rollout {
        seed,
        seats: Vec::new(),
        error: Some(error),
    }
}

fn reduce_rollout<K: Ord + Clone>(
    rollout: &Rollout,
    profiles: &BTreeMap<K, Arc<Profile>>,
    reward: &Reward,
    key_of: impl Fn(&SeatRollout) -> K,
) -> ReducedBatch<K> {
    if rollout.error.is_some() {
        return ReducedBatch {
            errors: 1,
            statistics: BTreeMap::new(),
        };
    }
    let mut reduced = ReducedBatch::default();
    for seat in &rollout.seats {
        let key = key_of(seat);
        let Some(profile) = profiles.get(&key) else {
            continue;
        };
        let rows = collect_statistics(&seat.trajectory, &seat.episode, profile, reward);
        let target = reduced.statistics.entry(key).or_default();
        for (head, row) in rows {
            target.entry(head).or_default().merge(&row);
        }
    }
    reduced
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reduced rotated batch needs the same deterministic inputs as its rollout panel"
)]
fn play_rotated_map_batch_statistics(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
    map: &OpeningMap,
    reward: &Reward,
) -> ReducedBatch<FactionId> {
    if seeds.is_empty() || factions.is_empty() {
        return ReducedBatch::default();
    }
    let players: Vec<PlayerId> = (0..factions.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let shared_profiles: BTreeMap<FactionId, Arc<Profile>> = profiles
        .iter()
        .map(|(faction, profile)| (faction.clone(), Arc::new(profile.clone())))
        .collect();
    let jobs: Vec<(u64, usize)> = seeds
        .iter()
        .flat_map(|seed| (0..factions.len()).map(move |rotation| (*seed, rotation)))
        .collect();

    let partials: Vec<ReducedBatch<FactionId>> = jobs
        .par_iter()
        .map(|(seed, rotation)| {
            let assignments: BTreeMap<PlayerId, FactionId> = players
                .iter()
                .enumerate()
                .map(|(seat, player)| {
                    (
                        player.clone(),
                        seated_faction(factions, *seed, *rotation, seat),
                    )
                })
                .collect();
            let seated_profiles = assignments
                .iter()
                .filter_map(|(player, faction)| {
                    shared_profiles
                        .get(faction)
                        .cloned()
                        .map(|profile| (player.clone(), profile))
                })
                .collect();
            let rollout = play_assigned_on_map_shared(
                content,
                &players,
                &assignments,
                &seated_profiles,
                sources,
                *seed,
                horizon,
                requirement,
                map,
                true,
            );
            reduce_rollout(&rollout, &shared_profiles, reward, |seat| {
                seat.faction.clone()
            })
        })
        .collect();

    merge_partials(&partials)
}

/// Play several seed groups (one per update) in one shared parallel wave, returning one reduced
/// batch per group in group order.
///
/// The wave lets straggler games from one update overlap with faster games from neighbouring
/// updates instead of idling the pool at each update boundary. Every job is independent and
/// seeded by its own (group seed, rotation), so per-game results are identical to playing the
/// groups sequentially; only the wall-clock schedule differs.
fn play_rotated_map_group_statistics(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seed_groups: &[Vec<u64>],
    horizon: Horizon,
    requirement: Requirement,
    map: &OpeningMap,
    reward: &Reward,
) -> Vec<ReducedBatch<FactionId>> {
    if factions.is_empty() || seed_groups.iter().all(std::vec::Vec::is_empty) {
        return (0..seed_groups.len())
            .map(|_| ReducedBatch::default())
            .collect();
    }
    let players: Vec<PlayerId> = (0..factions.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let shared_profiles: BTreeMap<FactionId, Arc<Profile>> = profiles
        .iter()
        .map(|(faction, profile)| (faction.clone(), Arc::new(profile.clone())))
        .collect();
    let jobs: Vec<(usize, u64, usize)> = seed_groups
        .iter()
        .enumerate()
        .flat_map(|(group, seeds)| {
            seeds.iter().flat_map(move |seed| {
                (0..factions.len()).map(move |rotation| (group, *seed, rotation))
            })
        })
        .collect();

    let partials: Vec<(usize, ReducedBatch<FactionId>)> = jobs
        .par_iter()
        .map(|(group, seed, rotation)| {
            let assignments: BTreeMap<PlayerId, FactionId> = players
                .iter()
                .enumerate()
                .map(|(seat, player)| {
                    (
                        player.clone(),
                        seated_faction(factions, *seed, *rotation, seat),
                    )
                })
                .collect();
            let seated_profiles = assignments
                .iter()
                .filter_map(|(player, faction)| {
                    shared_profiles
                        .get(faction)
                        .cloned()
                        .map(|profile| (player.clone(), profile))
                })
                .collect();
            let rollout = play_assigned_on_map_shared(
                content,
                &players,
                &assignments,
                &seated_profiles,
                sources,
                *seed,
                horizon,
                requirement,
                map,
                true,
            );
            let reduced = reduce_rollout(&rollout, &shared_profiles, reward, |seat| {
                seat.faction.clone()
            });
            (*group, reduced)
        })
        .collect();

    // `collect` preserves job order, so each group's partials arrive in seed/rotation order.
    let mut per_group: Vec<Vec<ReducedBatch<FactionId>>> =
        (0..seed_groups.len()).map(|_| Vec::new()).collect();
    for (group, partial) in partials {
        per_group[group].push(partial);
    }
    per_group
        .into_iter()
        .map(|partials| merge_partials(&partials))
        .collect()
}

/// Play and reduce several updates' faction-rotated varied-map batches in one shared wave.
#[must_use]
pub fn play_rotated_batch_group_statistics(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seed_groups: &[Vec<u64>],
    horizon: Horizon,
    requirement: Requirement,
    reward: &Reward,
) -> Vec<ReducedBatch<FactionId>> {
    play_rotated_map_group_statistics(
        content,
        factions,
        profiles,
        sources,
        seed_groups,
        horizon,
        requirement,
        &OpeningMap::RustVaried,
        reward,
    )
}

/// Play and reduce several updates' faction-rotated Python map-pool batches in one shared wave.
#[must_use]
pub fn play_rotated_save54_pool_batch_group_statistics(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seed_groups: &[Vec<u64>],
    horizon: Horizon,
    requirement: Requirement,
    pool: Arc<ti4_sim::MapPool>,
    tile_seed_offset: u64,
    reward: &Reward,
) -> Vec<ReducedBatch<FactionId>> {
    play_rotated_map_group_statistics(
        content,
        factions,
        profiles,
        sources,
        seed_groups,
        horizon,
        requirement,
        &OpeningMap::PythonPool {
            pool,
            tile_seed_offset,
        },
        reward,
    )
}

/// Play and reduce a faction-rotated varied-map training batch on rollout workers.
#[must_use]
pub fn play_rotated_batch_statistics(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
    reward: &Reward,
) -> ReducedBatch<FactionId> {
    play_rotated_map_batch_statistics(
        content,
        factions,
        profiles,
        sources,
        seeds,
        horizon,
        requirement,
        &OpeningMap::RustVaried,
        reward,
    )
}

/// Play and reduce a faction-rotated Python map-pool training batch on rollout workers.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "a parity batch's complete deterministic input"
)]
pub fn play_rotated_save54_pool_batch_statistics(
    content: &ContentStore,
    factions: &[FactionId],
    profiles: &BTreeMap<FactionId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
    pool: Arc<ti4_sim::MapPool>,
    tile_seed_offset: u64,
    reward: &Reward,
) -> ReducedBatch<FactionId> {
    play_rotated_map_batch_statistics(
        content,
        factions,
        profiles,
        sources,
        seeds,
        horizon,
        requirement,
        &OpeningMap::PythonPool {
            pool,
            tile_seed_offset,
        },
        reward,
    )
}

/// Play a batch of rollouts in parallel, returning results in seed order.
///
/// Each seed gets its own game; profiles are shared read-only. Results are
/// collected by seed, not by completion, so the order is deterministic
/// regardless of thread scheduling.
///
/// # Parallelism
///
/// Seeds are divided into chunks proportional to `available_parallelism()`.
/// Each chunk is processed by one thread. The caller should ensure that
/// `profiles` is `Send + Sync` (which it is when cloned as `Arc<Profile>`
/// or borrowed as `&Profile` behind a shared pointer).
///
/// # Determinism
///
/// The same seeds produce the same rollouts regardless of worker count,
/// because each seed's game is independent and results are sorted by seed.
pub fn play_batch(
    content: &ContentStore,
    players: &[PlayerId],
    profiles: &BTreeMap<PlayerId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
) -> Vec<Rollout> {
    if seeds.is_empty() {
        return Vec::new();
    }

    let shared: BTreeMap<PlayerId, Arc<Profile>> = profiles
        .iter()
        .map(|(player, profile)| (player.clone(), Arc::new(profile.clone())))
        .collect();
    let mut results: Vec<Rollout> = seeds
        .par_iter()
        .map(|seed| {
            play_shared(
                content,
                players,
                &shared,
                sources,
                *seed,
                horizon,
                requirement,
            )
        })
        .collect();

    // Sort by seed so the order is deterministic regardless of thread scheduling.
    results.sort_by_key(|r| r.seed);
    results
}

/// Play ordinary fixed-seat games and reduce their trajectories on the rollout workers.
///
/// Used by both Stage 1 and Stage 2. The returned merge order follows `seeds`, so worker
/// scheduling cannot change floating-point accumulation or the resulting policy update.
#[must_use]
pub fn play_batch_statistics(
    content: &ContentStore,
    players: &[PlayerId],
    profiles: &BTreeMap<PlayerId, Profile>,
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
    reward: &Reward,
) -> ReducedBatch<PlayerId> {
    if seeds.is_empty() {
        return ReducedBatch::default();
    }
    let shared: BTreeMap<PlayerId, Arc<Profile>> = profiles
        .iter()
        .map(|(player, profile)| (player.clone(), Arc::new(profile.clone())))
        .collect();
    let partials: Vec<ReducedBatch<PlayerId>> = seeds
        .par_iter()
        .map(|seed| {
            let rollout = play_shared(
                content,
                players,
                &shared,
                sources,
                *seed,
                horizon,
                requirement,
            );
            reduce_rollout(&rollout, &shared, reward, |seat| seat.player.clone())
        })
        .collect();
    merge_partials(&partials)
}

/// The default opening bar.
#[must_use]
pub const fn default_requirement() -> Requirement {
    DEFAULT_REQUIREMENT
}

/// The content scope rollouts are played under.
#[must_use]
pub const fn default_sources() -> SourceSet {
    DEFAULT
}

// --- authored-bot reference panel -------------------------------------------------------------

/// Play the Stage-2 evaluation panel with the **authored** bot in every seat.
///
/// A reference point rather than a competitor. A learned policy that has plateaued tells you
/// nothing on its own about whether it has converged to the best available play or merely to the
/// best its gradient could find: the number to compare against is what a hand-written bot scores
/// on the identical panel. Same seeds, same rotations, same map pool, same horizon — only the
/// decider differs.
///
/// Deliberately additive rather than a flag on the learned path, so nothing about the running
/// trainer's behaviour can change as a side effect of measuring a baseline.
#[must_use]
pub fn play_rotated_pool_batch_authored(
    content: &'static ContentStore,
    factions: &[FactionId],
    sources: SourceSet,
    seeds: &[u64],
    horizon: Horizon,
    requirement: Requirement,
    pool: Arc<ti4_sim::MapPool>,
    tile_seed_offset: u64,
) -> Vec<Rollout> {
    if seeds.is_empty() || factions.is_empty() {
        return Vec::new();
    }
    let players: Vec<PlayerId> = (0..factions.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let map = OpeningMap::PythonPool {
        pool,
        tile_seed_offset,
    };
    let jobs: Vec<(usize, u64, usize)> = seeds
        .iter()
        .flat_map(|seed| (0..factions.len()).map(move |rotation| (*seed, rotation)))
        .enumerate()
        .map(|(index, (seed, rotation))| (index, seed, rotation))
        .collect();

    let mut results: Vec<(usize, Rollout)> = jobs
        .par_iter()
        .map(|(index, seed, rotation)| {
            let assignments: BTreeMap<PlayerId, FactionId> = players
                .iter()
                .enumerate()
                .map(|(seat, player)| {
                    (
                        player.clone(),
                        seated_faction(factions, *seed, *rotation, seat),
                    )
                })
                .collect();
            (
                *index,
                play_assigned_on_map_authored(
                    content,
                    &players,
                    &assignments,
                    sources,
                    *seed,
                    horizon,
                    requirement,
                    &map,
                ),
            )
        })
        .collect();
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, rollout)| rollout).collect()
}

/// One game of the reference panel, seated with the authored bot.
fn play_assigned_on_map_authored(
    content: &ContentStore,
    players: &[PlayerId],
    factions: &BTreeMap<PlayerId, FactionId>,
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
    requirement: Requirement,
    map: &OpeningMap,
) -> Rollout {
    let (state, galaxy, factions) = match seated(content, players, factions, sources, seed, map) {
        Ok(seated) => seated,
        Err(error) => return failed(seed, error),
    };
    let baselines: BTreeMap<PlayerId, Baseline> = {
        let seen = Observed::new(&state, content, sources, Some(&galaxy));
        players
            .iter()
            .map(|player| (player.clone(), Baseline::taken(&seen, player)))
            .collect()
    };

    let mut table = Table::with_default(Box::new(SeededRandom::new(seed)));
    for (index, player) in players.iter().enumerate() {
        let stream = seed
            .wrapping_mul(1_000_003)
            .wrapping_add(u64::try_from(index).unwrap_or(0));
        table.seat(
            player.clone(),
            Box::new(ti4_policy::bot::ScoredBot::new(stream)),
        );
    }

    let mut game = Game::with_table(state, content, table)
        .with_sources(sources)
        .with_galaxy(galaxy);
    let error = game
        .run(horizon.rounds, horizon.steps)
        .err()
        .map(|error| error.to_string());

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
        content,
        sources,
    );

    let seats = players
        .iter()
        .map(|player| {
            let opening = openings.get(player);
            SeatRollout {
                player: player.clone(),
                faction: factions
                    .get(player)
                    .cloned()
                    .unwrap_or_else(|| FactionId::new("")),
                trajectory: Vec::new(),
                episode: Episode {
                    steps: Vec::new(),
                    final_progress: finals.get(player).copied().unwrap_or_default(),
                    cleared: opening.is_some_and(ti4_engine::opening::Opening::cleared),
                    shortfall: opening.map_or(0.0, |opening| opening.weighted_shortfall(1.0, 1.0)),
                    traded_goods: 0.0,
                },
            }
        })
        .collect();

    Rollout { seed, seats, error }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;

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

    fn save54_pool() -> Arc<ti4_sim::MapPool> {
        let mut coords: Vec<[i32; 2]> = SAVE54_NEUTRAL.iter().map(|(_, q, r)| [*q, *r]).collect();
        coords.extend(SAVE54_HOMES.map(|hex| [hex.q, hex.r]));
        let mut arrangement: Vec<String> = SAVE54_NEUTRAL
            .iter()
            .map(|(system, _, _)| (*system).to_owned())
            .collect();
        arrangement.extend(["10", "12", "16"].map(str::to_owned));
        let payload = serde_json::json!({
            "schema": "ti4-map-pool-v1",
            "effort": 2000,
            "coords": coords,
            "slots": SAVE54_HOMES.map(|hex| [hex.q, hex.r]),
            "arrangements": [arrangement],
        });
        Arc::new(
            ti4_sim::MapPool::from_reader(payload.to_string().as_bytes())
                .expect("test Save-54 pool is valid"),
        )
    }

    #[test]
    fn pooled_save54_batch_plays_every_rotation_without_changing_the_pool() {
        let factions: Vec<FactionId> = ["letnev", "jolnar", "hacan"]
            .into_iter()
            .map(FactionId::new)
            .collect();
        let profiles = factions
            .iter()
            .map(|faction| {
                (
                    faction.clone(),
                    ti4_policy::learned::blank_explicit_profile(faction.as_str()),
                )
            })
            .collect();
        let pool = save54_pool();
        let played = play_rotated_save54_pool_batch(
            ContentStore::embedded(),
            &factions,
            &profiles,
            POK,
            &[7],
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
            Arc::clone(&pool),
            20_000_000,
        );
        assert_eq!(played.len(), 3);
        assert!(played.iter().all(|rollout| rollout.error.is_none()));
        assert_eq!(pool.len(), 1, "playing rotations does not mutate the pool");
    }

    #[test]
    fn pooled_save54_parallelism_preserves_exact_rollout_order_and_values() {
        let factions: Vec<FactionId> = ["letnev", "jolnar", "hacan"]
            .into_iter()
            .map(FactionId::new)
            .collect();
        let profiles = factions
            .iter()
            .map(|faction| {
                (
                    faction.clone(),
                    ti4_policy::learned::blank_explicit_profile(faction.as_str()),
                )
            })
            .collect();
        let run = |workers| {
            play_rotated_save54_pool_batch_with_workers(
                ContentStore::embedded(),
                &factions,
                &profiles,
                POK,
                &[7, 8],
                Horizon::opening(),
                DEFAULT_REQUIREMENT,
                save54_pool(),
                20_000_000,
                workers,
                true,
            )
        };

        assert_eq!(run(1), run(32));
    }

    #[test]
    fn worker_reduction_matches_parent_reduction_for_rotated_stage_one() {
        let factions: Vec<FactionId> = ["letnev", "jolnar", "hacan"]
            .into_iter()
            .map(FactionId::new)
            .collect();
        let profiles: BTreeMap<FactionId, Profile> = factions
            .iter()
            .map(|faction| {
                (
                    faction.clone(),
                    ti4_policy::learned::blank_explicit_profile(faction.as_str()),
                )
            })
            .collect();
        let reward = Reward::for_stage(crate::reward::Stage::One);
        let rollouts = play_rotated_save54_pool_batch(
            ContentStore::embedded(),
            &factions,
            &profiles,
            POK,
            &[7, 8],
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
            save54_pool(),
            20_000_000,
        );
        let expected = crate::gradient::faction_batch_statistics(&rollouts, &profiles, &reward);
        let reduced = play_rotated_save54_pool_batch_statistics(
            ContentStore::embedded(),
            &factions,
            &profiles,
            POK,
            &[7, 8],
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
            save54_pool(),
            20_000_000,
            &reward,
        );

        assert_eq!(reduced.errors, 0);
        assert_eq!(reduced.statistics, expected);
    }

    #[test]
    fn worker_reduction_matches_parent_reduction_for_stage_two() {
        let players = seats(&["a", "b", "c"]);
        let factions = ti4_engine::seating::seat_in_scope(&players);
        let profiles: BTreeMap<PlayerId, Profile> = players
            .iter()
            .map(|player| {
                let faction = factions
                    .get(player)
                    .map_or_else(String::new, ToString::to_string);
                (
                    player.clone(),
                    ti4_policy::learned::blank_explicit_profile(&faction),
                )
            })
            .collect();
        let reward = Reward::for_stage(crate::reward::Stage::Two);
        let rollouts = play_batch(
            ContentStore::embedded(),
            &players,
            &profiles,
            POK,
            &[13],
            Horizon::short(),
            DEFAULT_REQUIREMENT,
        );
        let expected = crate::gradient::batch_statistics(&rollouts, &profiles, &reward);
        let reduced = play_batch_statistics(
            ContentStore::embedded(),
            &players,
            &profiles,
            POK,
            &[13],
            Horizon::short(),
            DEFAULT_REQUIREMENT,
            &reward,
        );

        assert_eq!(reduced.errors, 0);
        assert_eq!(reduced.statistics, expected);
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

    #[test]
    fn play_batch_produces_one_rollout_per_seed() {
        let seeds = vec![100, 101, 102, 103];
        let rollouts = play_batch(
            ContentStore::embedded(),
            &seats(&["a", "b", "c"]),
            &BTreeMap::new(),
            POK,
            &seeds,
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
        );
        assert_eq!(rollouts.len(), 4);
        for (i, rollout) in rollouts.iter().enumerate() {
            assert_eq!(rollout.seed, seeds[i]);
            assert_eq!(rollout.error, None, "seed {} should not error", seeds[i]);
            assert_eq!(rollout.seats.len(), 3);
        }
    }

    #[test]
    fn play_batch_returns_results_in_seed_order() {
        let seeds = vec![110, 105, 100, 108];
        let mut sorted = seeds.clone();
        sorted.sort_unstable();
        let rollouts = play_batch(
            ContentStore::embedded(),
            &seats(&["a", "b", "c"]),
            &BTreeMap::new(),
            POK,
            &seeds,
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
        );
        // Results must be sorted by seed value regardless of input order.
        for (i, seed) in sorted.iter().enumerate() {
            assert_eq!(rollouts[i].seed, *seed, "seed {seed} at position {i}");
        }
    }

    #[test]
    fn play_batch_is_deterministic_across_runs() {
        let seeds = vec![120, 121, 122];
        let once = play_batch(
            ContentStore::embedded(),
            &seats(&["a", "b", "c"]),
            &BTreeMap::new(),
            POK,
            &seeds,
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
        );
        let twice = play_batch(
            ContentStore::embedded(),
            &seats(&["a", "b", "c"]),
            &BTreeMap::new(),
            POK,
            &seeds,
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
        );
        for (a, b) in once.iter().zip(&twice) {
            assert_eq!(a.seed, b.seed);
            assert_eq!(a.seats.len(), b.seats.len());
            for (sa, sb) in a.seats.iter().zip(&b.seats) {
                assert_eq!(sa.episode.steps, sb.episode.steps);
            }
        }
    }

    #[test]
    fn play_batch_empty_input_returns_empty() {
        let rollouts = play_batch(
            ContentStore::embedded(),
            &seats(&["a", "b", "c"]),
            &BTreeMap::new(),
            POK,
            &[],
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
        );
        assert!(rollouts.is_empty());
    }

    #[test]
    fn a_rotated_batch_places_every_faction_in_every_physical_seat() {
        let factions: Vec<FactionId> = ["letnev", "jolnar", "hacan"]
            .into_iter()
            .map(FactionId::new)
            .collect();
        let profiles: BTreeMap<FactionId, Profile> = factions
            .iter()
            .map(|faction| {
                (
                    faction.clone(),
                    ti4_policy::learned::blank_explicit_profile(faction.as_str()),
                )
            })
            .collect();
        let rollouts = play_rotated_batch(
            ContentStore::embedded(),
            &factions,
            &profiles,
            POK,
            &[41],
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
        );
        assert_eq!(rollouts.len(), 3);
        assert!(rollouts.iter().all(|rollout| rollout.error.is_none()));
        for faction in &factions {
            let seats: std::collections::BTreeSet<_> = rollouts
                .iter()
                .flat_map(|rollout| &rollout.seats)
                .filter(|seat| &seat.faction == faction)
                .map(|seat| seat.player.clone())
                .collect();
            assert_eq!(seats.len(), 3, "{faction} was not fully rotated");
        }
    }

    #[test]
    fn a_group_wave_matches_sequential_groups_with_frozen_profiles() {
        use crate::reward::Stage;
        let factions: Vec<FactionId> = ["letnev", "jolnar", "hacan"]
            .into_iter()
            .map(FactionId::new)
            .collect();
        let profiles: BTreeMap<FactionId, Profile> = factions
            .iter()
            .map(|faction| {
                (
                    faction.clone(),
                    ti4_policy::learned::blank_explicit_profile(faction.as_str()),
                )
            })
            .collect();
        let content = ContentStore::embedded();
        let reward = Reward::for_stage(Stage::Two);
        let group_a = vec![201u64, 202];
        let group_b = vec![203u64, 204];

        // One shared wave over both groups...
        let waved = play_rotated_batch_group_statistics(
            content,
            &factions,
            &profiles,
            POK,
            &[group_a.clone(), group_b.clone()],
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
            &reward,
        );
        // ...must equal playing each group on its own with the same frozen profiles.
        let sequential = [
            play_rotated_batch_statistics(
                content,
                &factions,
                &profiles,
                POK,
                &group_a,
                Horizon::opening(),
                DEFAULT_REQUIREMENT,
                &reward,
            ),
            play_rotated_batch_statistics(
                content,
                &factions,
                &profiles,
                POK,
                &group_b,
                Horizon::opening(),
                DEFAULT_REQUIREMENT,
                &reward,
            ),
        ];

        assert_eq!(waved.len(), 2);
        for (index, (wave, own)) in waved.iter().zip(sequential.iter()).enumerate() {
            assert_eq!(
                wave, own,
                "group {index} differs between wave and sequential play"
            );
        }
    }

    #[test]
    fn a_group_wave_with_empty_groups_returns_empty_batches() {
        use crate::reward::Stage;
        let factions: Vec<FactionId> = ["letnev", "jolnar"]
            .into_iter()
            .map(FactionId::new)
            .collect();
        let profiles: BTreeMap<FactionId, Profile> = factions
            .iter()
            .map(|faction| {
                (
                    faction.clone(),
                    ti4_policy::learned::blank_explicit_profile(faction.as_str()),
                )
            })
            .collect();
        let reward = Reward::for_stage(Stage::Two);
        let waved = play_rotated_batch_group_statistics(
            ContentStore::embedded(),
            &factions,
            &profiles,
            POK,
            &[vec![301u64], vec![]],
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
            &reward,
        );
        assert_eq!(waved.len(), 2);
        assert!(waved[1].statistics.is_empty());
    }

    #[test]
    fn the_opening_is_measured_at_the_end_of_round_one_however_long_the_game_runs() {
        // F-M10-034-C1. `opening::measure` compares a state against the setup snapshot and has no
        // notion of a round, so measuring it after the horizon answered "did this seat ever reach
        // the bar" rather than "did its opening clear". On the trained MLP those differed by about
        // sixteen points, and `Episode::cleared` is read as the opening everywhere -- Stage 1 pays
        // `clear_bonus` for it and Stage 2 credits `r1_bonus` for it at the last round-one
        // decision, on the stated grounds that a later decision cannot change it.
        //
        // Nothing in the suite pinned this, which is why the semantics could drift in the first
        // place: the change that fixed it broke no test.
        let factions: Vec<FactionId> = ["letnev", "jolnar", "sol"]
            .into_iter()
            .map(FactionId::new)
            .collect();
        let profiles: BTreeMap<FactionId, Profile> = factions
            .iter()
            .map(|faction| {
                (
                    faction.clone(),
                    ti4_policy::learned::blank_explicit_profile(faction.as_str()),
                )
            })
            .collect();
        let content = ContentStore::embedded();
        let seeds = vec![11u64, 12u64];

        let one_round = play_rotated_batch(
            content,
            &factions,
            &profiles,
            POK,
            &seeds,
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
        );
        let four_rounds = play_rotated_batch(
            content,
            &factions,
            &profiles,
            POK,
            &seeds,
            Horizon {
                rounds: 4,
                steps: 200_000,
            },
            DEFAULT_REQUIREMENT,
        );

        assert_eq!(one_round.len(), four_rounds.len());
        let mut compared = 0usize;
        for (short, long) in one_round.iter().zip(four_rounds.iter()) {
            assert_eq!(short.seed, long.seed);
            assert_eq!(short.seats.len(), long.seats.len());
            for (a, b) in short.seats.iter().zip(long.seats.iter()) {
                assert_eq!(a.player, b.player);
                assert_eq!(
                    a.episode.cleared, b.episode.cleared,
                    "seat {} on seed {}: the opening changed when the game ran longer",
                    a.player, short.seed
                );
                assert!(
                    (a.episode.shortfall - b.episode.shortfall).abs() < 1e-9,
                    "seat {} on seed {}: shortfall {} against {}",
                    a.player,
                    short.seed,
                    a.episode.shortfall,
                    b.episode.shortfall
                );
                compared += 1;
            }
        }
        assert!(compared >= 6, "only {compared} seats compared");

        // Non-vacuity: the longer games must actually have gone further, or the two runs would be
        // the same game and the comparison would prove nothing.
        assert!(
            four_rounds
                .iter()
                .zip(one_round.iter())
                .any(|(long, short)| {
                    long.seats.iter().zip(short.seats.iter()).any(|(b, a)| {
                        b.episode.final_progress.victory_points
                            > a.episode.final_progress.victory_points
                            || b.episode.final_progress.planets_gained
                                > a.episode.final_progress.planets_gained
                    })
                }),
            "four rounds produced no more progress than one, so the horizons did not differ"
        );
    }

    #[test]
    fn an_evaluation_rollout_matches_the_recording_rollout_on_finals() {
        let factions: Vec<FactionId> = ["letnev", "jolnar"]
            .into_iter()
            .map(FactionId::new)
            .collect();
        let profiles: BTreeMap<FactionId, Profile> = factions
            .iter()
            .map(|faction| {
                (
                    faction.clone(),
                    ti4_policy::learned::blank_explicit_profile(faction.as_str()),
                )
            })
            .collect();
        let content = ContentStore::embedded();
        let seeds = vec![7u64];

        let full = play_rotated_batch(
            content,
            &factions,
            &profiles,
            POK,
            &seeds,
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
        );
        let evaluation = play_rotated_batch_evaluation(
            content,
            &factions,
            &profiles,
            POK,
            &seeds,
            Horizon::opening(),
            DEFAULT_REQUIREMENT,
        );

        assert_eq!(full.len(), evaluation.len());
        for (index, (recording, unrecorded)) in full.iter().zip(evaluation.iter()).enumerate() {
            assert_eq!(recording.seed, unrecorded.seed);
            assert!(
                unrecorded
                    .seats
                    .iter()
                    .all(|seat| seat.trajectory.is_empty()),
                "evaluation rollout {index} retained a trajectory"
            );
            assert!(
                unrecorded
                    .seats
                    .iter()
                    .all(|seat| seat.episode.steps.is_empty()),
                "evaluation rollout {index} retained per-decision snapshots"
            );
            for (recording_seat, unrecorded_seat) in
                recording.seats.iter().zip(unrecorded.seats.iter())
            {
                assert_eq!(recording_seat.faction, unrecorded_seat.faction);
                assert_eq!(
                    recording_seat.episode.final_progress, unrecorded_seat.episode.final_progress,
                    "rollout {index}: final progress differs"
                );
                assert_eq!(
                    recording_seat.episode.cleared,
                    unrecorded_seat.episode.cleared
                );
                // Same deterministic computation on both sides: exact equality is the claim.
                let shortfall_delta =
                    (recording_seat.episode.shortfall - unrecorded_seat.episode.shortfall).abs();
                assert!(
                    shortfall_delta <= f64::EPSILON,
                    "rollout {index}: shortfall differs"
                );
            }
        }
    }
}
