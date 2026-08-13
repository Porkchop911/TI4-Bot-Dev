//! What the engine actually asks, and what the bot answers.
//!
//! A batch report says a subsystem was reached; this says whether the answers to it were any
//! good. Written after a scored bot doubled the number of invasions without moving the
//! scoreboard, which no event count could explain.
//!
//! `cargo run -p ti4-sim --example prompts --release [--random]`

use std::collections::BTreeMap;

/// A prompt, how often it was asked, and its commonest answers.
type PromptRow<'a> = (&'a String, usize, Vec<(&'a String, usize)>);

use ti4_content::ContentStore;
use ti4_engine::choice::{SeededRandom, Table};
use ti4_engine::game::Game;
use ti4_engine::setup::start_game;
use ti4_model::content_types::POK;
use ti4_model::id::{FactionId, PlayerId};

fn main() {
    let scored = !std::env::args().any(|arg| arg == "--random");
    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|name| PlayerId::new(*name))
        .collect();

    let mut state = start_game(content, &players, POK, None).expect("setup");
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

    let mut table = Table::with_default(Box::new(SeededRandom::new(5)));
    if scored {
        for (index, player) in players.iter().enumerate() {
            table.seat(
                player.clone(),
                Box::new(ti4_policy::bot::ScoredBot::new(
                    5000 + u64::try_from(index).unwrap_or(0),
                )),
            );
        }
    }

    let mut game = Game::with_table(state, content, table).with_galaxy(galaxy);
    let outcome = game.run(50, 2_000_000).err().map(|error| error.to_string());
    println!("seats: {}", if scored { "scored" } else { "random" });
    println!("outcome: {outcome:?}   round {}", game.state.round);
    println!(
        "scores: {:?}",
        game.state
            .players
            .iter()
            .map(|seat| (seat.id.to_string(), seat.victory_points))
            .collect::<Vec<(String, i32)>>()
    );
    println!(
        "planets held: {:?}",
        game.state
            .players
            .iter()
            .map(|seat| (
                seat.id.to_string(),
                game.state.controlled_planets(&seat.id).len()
            ))
            .collect::<Vec<(String, usize)>>()
    );

    // Prompt → how often it was asked, and the three commonest answers.
    let mut asked: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for record in &game.table.log.records {
        *asked
            .entry(record.prompt.clone())
            .or_default()
            .entry(record.chosen.clone())
            .or_default() += 1;
    }
    let mut rows: Vec<PromptRow<'_>> = asked
        .iter()
        .map(|(prompt, answers)| {
            let total: usize = answers.values().sum();
            let mut top: Vec<(&String, usize)> =
                answers.iter().map(|(id, count)| (id, *count)).collect();
            top.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
            top.truncate(3);
            (prompt, total, top)
        })
        .collect();
    rows.sort_by_key(|(_, total, _)| std::cmp::Reverse(*total));
    for (prompt, total, top) in rows {
        let listed: Vec<String> = top
            .iter()
            .map(|(id, count)| format!("{id}×{count}"))
            .collect();
        println!("{total:>5}  {prompt}  →  {}", listed.join(", "));
    }
}
