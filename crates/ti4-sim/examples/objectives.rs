//! Why nobody scores: is it the requirements, or the gate in front of them?
//!
//! Written after a nine-round six-player game offered the scoring window four times in total, and
//! only ever for one objective. Four opportunities out of fifty-four is as consistent with a
//! broken requirement as with a hard one, and the difference is not something to reason about.
//!
//! Samples **every status phase**, not the final position. An end-of-game snapshot answers the
//! wrong question: what matters is what a seat could score at the moment it was asked, and a seat
//! that held its home for six rounds and lost it in the seventh looks identical at the end to one
//! that never held it at all.
//!
//! Reports:
//!
//! - how often a seat held its whole home system when a scoring window opened. 61.16 makes that a
//!   precondition for scoring *any* public objective, so a seat that has lost one planet of its
//!   home scores nothing again, however well it plays;
//! - what that gate actually costs — requirements met but refused — against how often nothing was
//!   met in the first place. Those are different problems with different fixes;
//! - which revealed objectives were ever met by anybody.
//!
//! `cargo run -p ti4-sim --example objectives --release [--games N]`

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_engine::choice::{SeededRandom, Table};
use ti4_engine::game::Game;
use ti4_engine::objectives::{Position, controls_home_system, requirement_for};
use ti4_engine::setup::start_game_seeded;
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;
use ti4_model::state::Phase;

/// What the seats' positions looked like when scoring windows opened.
#[derive(Default)]
struct Tally {
    /// Windows where the seat held its whole home system.
    home_held: usize,
    /// Windows where it did not, and so could score nothing (61.16).
    home_lost: usize,
    /// Requirements met at a window the seat was allowed to score at.
    met_and_allowed: usize,
    /// Requirements met at a window 61.16 refused. The cost of the gate.
    met_but_refused: usize,
    /// Windows where the seat held its home and still met nothing.
    allowed_but_empty: usize,
    /// The round each seat first lost its home, if it did.
    lost_at: BTreeMap<String, u32>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let games: u64 = args
        .iter()
        .position(|arg| arg == "--games")
        .and_then(|at| args.get(at + 1))
        .and_then(|count| count.parse().ok())
        .unwrap_or(12);

    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|name| PlayerId::new(*name))
        .collect();

    let mut tally = Tally::default();
    let mut met_by_someone: BTreeMap<String, usize> = BTreeMap::new();
    let mut revealed_seen: BTreeMap<String, usize> = BTreeMap::new();

    for seed in 0..games {
        let mut game = seated(content, &players, seed);
        let mut sampled_round = 0;

        for _ in 0..2_000_000 {
            if game.state.finished {
                break;
            }
            // One sample per round, the first time the status phase is seen. Sampling every step
            // would weigh a long status phase over a short one and count one position many times.
            if game.state.phase == Phase::Status && game.state.round != sampled_round {
                sampled_round = game.state.round;
                sample(
                    content,
                    &game,
                    &players,
                    &mut tally,
                    &mut met_by_someone,
                    seed,
                );
            }
            if game.step().error.is_some() {
                break;
            }
        }

        for alias in &game.state.revealed_objectives {
            *revealed_seen.entry(alias.to_string()).or_default() += 1;
        }
    }

    let windows = tally.home_held + tally.home_lost;
    println!("games: {games}   scoring windows sampled: {windows}  (seat x round)");
    println!(
        "  held their whole home system: {:>5} ({:.0}%)",
        tally.home_held,
        percent(tally.home_held, windows)
    );
    println!(
        "  locked out by 61.16:          {:>5} ({:.0}%)",
        tally.home_lost,
        percent(tally.home_lost, windows)
    );
    println!();
    println!("what each obstacle costs, in requirements met per window:");
    println!("  met and scoreable:  {:>5}", tally.met_and_allowed);
    println!(
        "  met but refused:    {:>5}   <- the cost of 61.16",
        tally.met_but_refused
    );
    println!(
        "  allowed, met none:  {:>5}   <- the cost of everything else",
        tally.allowed_but_empty
    );
    println!();
    let mut rounds: Vec<u32> = tally.lost_at.values().copied().collect();
    rounds.sort_unstable();
    println!(
        "seats that lost a home planet: {} of {}, first lost in rounds {rounds:?}",
        tally.lost_at.len(),
        usize::try_from(games).unwrap_or(0) * players.len()
    );
    println!();
    println!("revealed objectives, and how often any seat met the requirement at a window:");
    let mut rows: Vec<(&String, usize, usize)> = revealed_seen
        .iter()
        .map(|(alias, seen)| {
            (
                alias,
                *seen,
                met_by_someone.get(alias).copied().unwrap_or(0),
            )
        })
        .collect();
    rows.sort_by_key(|(_, _, met)| std::cmp::Reverse(*met));
    for (alias, seen, met) in rows {
        let registered =
            if requirement_for(&ti4_model::id::ObjectiveId::new(alias.as_str())).is_some() {
                ""
            } else {
                "   (bought, not met — 61.10)"
            };
        println!("  revealed in {seen:>3} games, met {met:>4} times  {alias}{registered}");
    }
}

fn sample(
    content: &ContentStore,
    game: &Game<'_>,
    players: &[PlayerId],
    tally: &mut Tally,
    met_by_someone: &mut BTreeMap<String, usize>,
    seed: u64,
) {
    for player in players {
        let mut position = Position::new(&game.state, content, POK, player);
        position.galaxy = game.galaxy();

        let home = controls_home_system(&position);
        let scored = game.state.scored_by(player);
        let mut met = 0usize;
        for alias in &game.state.revealed_objectives {
            if scored.contains(alias) {
                continue;
            }
            // A bought objective is not met, it is afforded (61.10), so it is not evidence about
            // requirements either way.
            let Some(check) = requirement_for(alias) else {
                continue;
            };
            if check(&position) {
                met += 1;
                *met_by_someone.entry(alias.to_string()).or_default() += 1;
            }
        }

        if home {
            tally.home_held += 1;
            tally.met_and_allowed += met;
            if met == 0 {
                tally.allowed_but_empty += 1;
            }
        } else {
            tally.home_lost += 1;
            tally.met_but_refused += met;
            tally
                .lost_at
                .entry(format!("{seed}:{player}"))
                .or_insert(game.state.round);
        }
    }
}

fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    100.0 * f64::from(u32::try_from(part).unwrap_or(u32::MAX))
        / f64::from(u32::try_from(whole).unwrap_or(u32::MAX))
}

fn seated<'a>(content: &'a ContentStore, players: &[PlayerId], seed: u64) -> Game<'a> {
    let mut state = start_game_seeded(content, players, POK, None, seed).expect("setup");
    let factions = ti4_engine::seating::seat_in_scope(players);
    for (player, faction) in &factions {
        state.player_mut(player).unwrap().faction = faction.clone();
    }
    let filler: Vec<String> = ti4_engine::seating::neutral_systems(content, 30, POK)
        .into_iter()
        .map(|system| system.to_string())
        .collect();
    let borrowed: Vec<&str> = filler.iter().map(String::as_str).collect();
    let galaxy =
        ti4_engine::seating::build_board(content, &factions, &borrowed, POK).expect("board");
    for (player, faction) in &factions {
        ti4_engine::seating::deploy(&mut state, content, player, faction, POK).expect("deploy");
    }

    let mut table = Table::with_default(Box::new(SeededRandom::new(seed)));
    for (index, player) in players.iter().enumerate() {
        table.seat(
            player.clone(),
            Box::new(ti4_policy::bot::ScoredBot::new(
                seed.wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0)),
            )),
        );
    }
    Game::with_table(state, content, table).with_galaxy(galaxy)
}
