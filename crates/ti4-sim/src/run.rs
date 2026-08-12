//! Single-game and batch runners (M10-007, M10-008).
//!
//! Ported from the oracle's `engine/sim.py` `play` and `run`.
//!
//! A run is defined by its seed. The same seed and the same engine give the same game, which is
//! what makes a batch reproducible rather than merely large — and what lets a failure be handed
//! to somebody as a number rather than a description.

use std::collections::BTreeMap;
use std::time::Instant;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::game::Game;
use ti4_engine::objectives::VICTORY_TARGET;
use ti4_engine::setup::start_game;
use ti4_model::content_types::{POK, SourceSet};
use ti4_model::id::{FactionId, PlayerId};
use ti4_model::state::GameState;

use crate::result::{Batch, Ending, GameResult};

/// What a run is allowed to do before it is called off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Horizon {
    /// Rounds to play at most.
    pub rounds: u32,
    /// Steps to take at most, across the whole game.
    ///
    /// A separate bound from `rounds` on purpose: a game that stops advancing rounds would
    /// otherwise spin for ever inside one, and the round limit would never be reached.
    pub steps: usize,
}

impl Default for Horizon {
    fn default() -> Self {
        Self {
            rounds: 50,
            steps: 2_000_000,
        }
    }
}

/// How a game is set up before it is played.
#[derive(Debug, Clone)]
pub struct Table {
    /// Seats, in order.
    pub players: Vec<PlayerId>,
    /// The faction each seat plays.
    pub factions: BTreeMap<PlayerId, FactionId>,
    /// Content scope.
    pub sources: SourceSet,
}

impl Table {
    /// Seat `players` on the first factions the corpus offers, in a stable order.
    ///
    /// A placeholder for the map pool and faction panels of M10-002 through M10-006, which decide
    /// this properly. Named as one so nobody mistakes "the first six factions" for a balanced
    /// draw — a batch run on it measures the engine, not the matchup.
    #[must_use]
    pub fn seated(content: &ContentStore, players: &[PlayerId], sources: SourceSet) -> Self {
        let available: Vec<String> = ti4_content::factions::catalogue(content, sources)
            .keys()
            .map(|alias| (*alias).to_owned())
            .collect();
        let factions = players
            .iter()
            .zip(available)
            .map(|(player, alias)| (player.clone(), FactionId::new(alias)))
            .collect();
        Self {
            players: players.to_vec(),
            factions,
            sources,
        }
    }
}

/// Build a seated game: a board, factions, and starting fleets on it.
///
/// Without this a game has no galaxy, and a game with no galaxy is offered no tactical action —
/// so it drafts strategy cards, passes, and ends with nobody having scored. Forty such games ran
/// clean and reported a completion rate of 1.00, which is how a harness can measure nothing at
/// all and look healthy doing it.
fn seat(content: &ContentStore, table: &Table) -> Result<(GameState, Galaxy), String> {
    let mut state = start_game(content, &table.players, table.sources, None)
        .map_err(|error| format!("setup: {error}"))?;

    for (player, faction) in &table.factions {
        if let Some(seat) = state.player_mut(player) {
            seat.faction = faction.clone();
        }
    }

    // Enough neutral tiles to sit between the homes and Mecatol.
    let filler: Vec<String> = ti4_engine::seating::neutral_systems(content, 18, table.sources)
        .into_iter()
        .map(|system| system.to_string())
        .collect();
    let borrowed: Vec<&str> = filler.iter().map(String::as_str).collect();
    let galaxy =
        ti4_engine::seating::build_board(content, &table.factions, &borrowed, table.sources)
            .map_err(|error| format!("board: {error}"))?;

    for (player, faction) in &table.factions {
        ti4_engine::seating::deploy(&mut state, content, player, faction, table.sources)
            .map_err(|error| format!("deploy: {error}"))?;
    }
    Ok((state, galaxy))
}

/// Play one game and reduce it to a [`GameResult`].
///
/// Never panics on a broken game: a failure is recorded on the result and the batch counts it.
/// A runner that stopped on the first bad seed would make a hundred-game batch as informative as
/// its worst game.
#[must_use]
pub fn play(
    content: &ContentStore,
    players: &[PlayerId],
    sources: SourceSet,
    seed: u64,
    horizon: Horizon,
) -> GameResult {
    let started = Instant::now();
    let table = Table::seated(content, players, sources);
    let (state, galaxy) = match seat(content, &table) {
        Ok(seated) => seated,
        Err(error) => return failed(seed, players, started, error),
    };

    let mut game = Game::with_seeded_random(state, content, seed).with_galaxy(galaxy);
    let outcome = game
        .run(horizon.rounds, horizon.steps)
        .err()
        .map(|error| error.to_string());
    let seconds = started.elapsed().as_secs_f64();

    let victory_points: BTreeMap<String, i32> = game
        .state
        .players
        .iter()
        .map(|seat| (seat.id.to_string(), seat.victory_points))
        .collect();
    let mut events: BTreeMap<String, usize> = BTreeMap::new();
    for event in &game.events {
        // Counted by label, with any payload after the first colon dropped: a per-system
        // activation would otherwise make every game's event table unique and uncountable.
        let label = event.split(':').next().unwrap_or(event);
        *events.entry(label.to_owned()).or_default() += 1;
    }

    let error = outcome;
    let top = victory_points.values().copied().max().unwrap_or(0);
    let ended_because = if error.is_some() {
        Ending::Error
    } else if top >= VICTORY_TARGET {
        Ending::VictoryPoints
    } else if game.state.finished {
        Ending::ObjectivesExhausted
    } else {
        Ending::HorizonReached
    };

    GameResult {
        seed,
        finished: game.state.finished && error.is_none(),
        // The leader, not a winner. A game that ran out of objectives still has one.
        winner: leader(&victory_points),
        rounds: game.state.round,
        victory_points,
        events,
        decisions: game.table.log.records.len(),
        seconds,
        ended_because,
        error,
    }
}

/// Whoever is ahead, or `None` when nobody has scored or the lead is tied.
///
/// A tie is not a leader. Naming one of them would invent a result the game did not produce, and
/// seat order would decide it — which is exactly the bias a batch is run to detect.
fn leader(points: &BTreeMap<String, i32>) -> Option<String> {
    let best = points.values().copied().max()?;
    if best <= 0 {
        return None;
    }
    let mut leading = points.iter().filter(|(_, score)| **score == best);
    let (seat, _) = leading.next()?;
    if leading.next().is_some() {
        return None; // tied
    }
    Some(seat.clone())
}

fn failed(seed: u64, players: &[PlayerId], started: Instant, error: String) -> GameResult {
    GameResult {
        seed,
        finished: false,
        winner: None,
        rounds: 0,
        victory_points: players
            .iter()
            .map(|player| (player.to_string(), 0))
            .collect(),
        events: BTreeMap::new(),
        decisions: 0,
        seconds: started.elapsed().as_secs_f64(),
        ended_because: Ending::Error,
        error: Some(error),
    }
}

/// Play `count` games from consecutive seeds, in parallel, and collect them in seed order.
///
/// Deterministic result ordering is the point of collecting by seed rather than by completion:
/// two runs of the same batch produce byte-identical reports, so a difference between them is a
/// change in the engine rather than in the scheduler.
#[must_use]
pub fn run(
    content: &'static ContentStore,
    players: &[PlayerId],
    seeds: impl IntoIterator<Item = u64>,
    horizon: Horizon,
) -> Batch {
    let seeds: Vec<u64> = seeds.into_iter().collect();
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let chunk = seeds.len().div_ceil(workers.max(1)).max(1);

    let mut results: Vec<GameResult> = std::thread::scope(|scope| {
        let handles: Vec<_> = seeds
            .chunks(chunk)
            .map(|batch| {
                let players = players.to_vec();
                scope.spawn(move || {
                    batch
                        .iter()
                        .map(|seed| play(content, &players, POK, *seed, horizon))
                        .collect::<Vec<GameResult>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .flatten()
            .collect()
    });
    results.sort_by_key(|result| result.seed);
    Batch { results }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seats(names: &[&str]) -> Vec<PlayerId> {
        names.iter().map(|name| PlayerId::new(*name)).collect()
    }

    #[test]
    fn a_game_plays_to_an_end_and_reports_what_happened() {
        let players = seats(&["a", "b", "c"]);
        let result = play(
            ContentStore::embedded(),
            &players,
            POK,
            7,
            Horizon::default(),
        );

        assert_eq!(result.error, None, "a seeded game runs clean");
        assert!(result.rounds > 0, "it played at least a round");
        assert!(result.decisions > 0, "and answered decisions");
        assert_eq!(result.victory_points.len(), 3, "one score per seat");
        assert!(
            result.events.values().sum::<usize>() > 0,
            "and emitted events"
        );
    }

    #[test]
    fn the_same_seed_plays_the_same_game() {
        // The property the whole harness rests on. Without it a batch is a pile of anecdotes.
        let players = seats(&["a", "b", "c"]);
        let once = play(
            ContentStore::embedded(),
            &players,
            POK,
            11,
            Horizon::default(),
        );
        let twice = play(
            ContentStore::embedded(),
            &players,
            POK,
            11,
            Horizon::default(),
        );

        assert_eq!(once.victory_points, twice.victory_points);
        assert_eq!(once.rounds, twice.rounds);
        assert_eq!(once.decisions, twice.decisions);
        assert_eq!(once.events, twice.events);
    }

    #[test]
    fn different_seeds_play_different_games() {
        // If they did not, the seed would not be reaching the decisions and a batch of a hundred
        // would be one game counted a hundred times.
        let players = seats(&["a", "b", "c"]);
        let games: Vec<GameResult> = (0..8)
            .map(|seed| {
                play(
                    ContentStore::embedded(),
                    &players,
                    POK,
                    seed,
                    Horizon::default(),
                )
            })
            .collect();

        let distinct: std::collections::BTreeSet<Vec<i32>> = games
            .iter()
            .map(|game| game.victory_points.values().copied().collect())
            .collect();
        assert!(
            distinct.len() > 1,
            "eight seeds produced one outcome: {distinct:?}"
        );
    }

    #[test]
    fn a_batch_comes_back_in_seed_order_however_it_was_scheduled() {
        let players = seats(&["a", "b"]);
        let batch = run(
            ContentStore::embedded(),
            &players,
            0..12,
            Horizon::default(),
        );

        let seeds: Vec<u64> = batch.results.iter().map(|result| result.seed).collect();
        assert_eq!(seeds, (0..12).collect::<Vec<u64>>());
        assert_eq!(batch.errors().len(), 0, "no game failed");
    }

    #[test]
    fn a_batch_run_twice_is_the_same_batch() {
        let players = seats(&["a", "b"]);
        let once = run(ContentStore::embedded(), &players, 0..6, Horizon::default());
        let twice = run(ContentStore::embedded(), &players, 0..6, Horizon::default());

        for (a, b) in once.results.iter().zip(&twice.results) {
            assert_eq!(a.seed, b.seed);
            assert_eq!(a.victory_points, b.victory_points);
            assert_eq!(a.rounds, b.rounds);
        }
    }

    #[test]
    fn a_batch_actually_exercises_the_engine() {
        // The guard this harness needed on its first run. Without a galaxy every game drafted
        // strategy cards, passed, and ended — forty of them, no errors, completion rate 1.00,
        // and not one tactical action. A harness that measures nothing can look perfectly
        // healthy, so the check is that the subsystems were reached, not that the batch ran.
        let players = seats(&["a", "b", "c", "d", "e", "f"]);
        let batch = run(ContentStore::embedded(), &players, 0..8, Horizon::default());

        assert_eq!(batch.errors().len(), 0, "no game failed");
        let silent = batch.never_happened(&[
            "TACTICAL_ACTION_BEGAN",
            "SYSTEM_ACTIVATED",
            "SHIP_MOVED",
            "PRODUCTION_RESOLVED",
            "INVASION_RESOLVED",
            "SPACE_COMBAT_RESOLVED",
            "STATUS_SCORING_BEGAN",
        ]);
        assert!(
            silent.is_empty(),
            "these subsystems were never reached in eight games: {silent:?}"
        );
    }

    #[test]
    fn a_tied_lead_names_nobody() {
        let tied: BTreeMap<String, i32> = [("a".to_owned(), 3), ("b".to_owned(), 3)]
            .into_iter()
            .collect();
        assert_eq!(leader(&tied), None, "seat order must not decide it");

        let clear: BTreeMap<String, i32> = [("a".to_owned(), 4), ("b".to_owned(), 3)]
            .into_iter()
            .collect();
        assert_eq!(leader(&clear), Some("a".to_owned()));

        let scoreless: BTreeMap<String, i32> = [("a".to_owned(), 0), ("b".to_owned(), 0)]
            .into_iter()
            .collect();
        assert_eq!(leader(&scoreless), None, "nobody has led anything");
    }

    #[test]
    fn a_short_horizon_is_reported_as_a_horizon_not_an_ending() {
        let players = seats(&["a", "b"]);
        let result = play(
            ContentStore::embedded(),
            &players,
            POK,
            3,
            Horizon {
                rounds: 1,
                steps: 2_000_000,
            },
        );

        assert_eq!(result.ended_because, Ending::HorizonReached);
        assert!(!result.finished, "the game was cut off, not concluded");
        assert_eq!(result.error, None, "and that is not a failure");
    }
}
