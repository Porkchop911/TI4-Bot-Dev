//! Why nobody scores: is it the requirements, or the gate in front of them?
//!
//! Written after a nine-round six-player game offered the scoring window four times in total, and
//! only ever for one objective. Four opportunities out of fifty-four is as consistent with a
//! broken requirement as with a hard one, and the difference is not something to reason about.
//!
//! Reports, over a batch, at the end of each game:
//!
//! - how often a seat held its whole home system, which 61.16 makes a precondition for scoring
//!   *any* public objective — a seat that has lost one planet of its home is scoring nothing for
//!   the rest of the game, however well it is playing;
//! - which revealed objectives any seat met, and which were never met by anybody.
//!
//! `cargo run -p ti4-sim --example objectives --release [--games N]`

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_engine::choice::{SeededRandom, Table};
use ti4_engine::game::Game;
use ti4_engine::objectives::{Position, controls_home_system, requirement_for, scoreable_on};
use ti4_engine::setup::start_game_seeded;
use ti4_model::content_types::POK;
use ti4_model::id::{FactionId, PlayerId};

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

    let mut home_held = 0usize;
    let mut home_lost = 0usize;
    let mut revealed_seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut met_by_someone: BTreeMap<String, usize> = BTreeMap::new();
    let mut scoreable_now = 0usize;
    let mut blocked_by_home = 0usize;

    for seed in 0..games {
        let mut game = seated(content, &players, seed);
        let _ = game.run(50, 2_000_000);

        for player in &players {
            let mut position = Position::new(&game.state, content, POK, player);
            let galaxy = game.galaxy();
            position.galaxy = galaxy;

            let home = controls_home_system(&position);
            if home {
                home_held += 1;
            } else {
                home_lost += 1;
            }

            let available = scoreable_on(&game.state, content, POK, player, galaxy);
            scoreable_now += available.len();

            // What the seat would have been offered but for 61.16. Counted separately, because a
            // requirement that is met and unreachable is a different problem from one that is
            // never met at all.
            let mut reachable = 0usize;
            for alias in &game.state.revealed_objectives {
                *revealed_seen.entry(alias.to_string()).or_default() += 1;
                let Some(check) = requirement_for(alias) else {
                    continue;
                };
                if check(&position) {
                    *met_by_someone.entry(alias.to_string()).or_default() += 1;
                    reachable += 1;
                }
            }
            if !home {
                blocked_by_home += reachable;
            }
        }
    }

    let seats = usize::try_from(games).unwrap_or(0) * players.len();
    let percent = |part: usize| {
        100.0 * f64::from(u32::try_from(part).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(seats.max(1)).unwrap_or(u32::MAX))
    };
    println!("games: {games}   seat-games: {seats}");
    println!(
        "held their whole home system at the end: {home_held} of {seats}  ({:.0}%)",
        percent(home_held)
    );
    println!("lost it: {home_lost}");
    println!("objectives scoreable at the final position: {scoreable_now}");
    println!("requirements met but blocked by 61.16: {blocked_by_home}");
    println!();
    println!("revealed objectives, and how often any seat met the requirement:");
    let mut rows: Vec<(&String, usize, usize)> = revealed_seen
        .iter()
        .map(|(alias, seen)| {
            (
                alias,
                *seen / players.len(),
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
                "   (no registered requirement)"
            };
        println!("  revealed in {seen:>3} games, met {met:>4} times  {alias}{registered}");
    }
}

fn seated<'a>(content: &'a ContentStore, players: &[PlayerId], seed: u64) -> Game<'a> {
    let mut state = start_game_seeded(content, players, POK, None, seed).expect("setup");
    let available: Vec<String> = ti4_content::factions::catalogue(content, POK)
        .keys()
        .map(|alias| (*alias).to_owned())
        .collect();
    let factions: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .cloned()
        .zip(available.into_iter().map(FactionId::new))
        .collect();
    for (player, faction) in &factions {
        state.player_mut(player).unwrap().faction = faction.clone();
    }
    let filler: Vec<String> = ti4_engine::seating::neutral_systems(content, 18, POK)
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
