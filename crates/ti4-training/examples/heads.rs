//! Decisions the engine asked, against decisions the policy recorded — and how the policy changes
//! the count.
//!
//! Written to answer why a Rust rollout raises about half the decisions the oracle does over the
//! same four rounds, after two plausible explanations turned out to be wrong: the games are not
//! shorter, and nothing is answered without the policy seeing it.
//!
//! `cargo run -p ti4-training --example heads --release [-- --scored]`
use std::collections::BTreeMap;
use ti4_content::ContentStore;
use ti4_engine::choice::{SeededRandom, Table};
use ti4_engine::game::Game;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::PlayerId;
use ti4_policy::inference::LearnedBot;
use ti4_policy::learned::{DEFAULT_DIMENSIONS, blank_profile};

fn main() {
    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = (0..3).map(|i| PlayerId::new(format!("p{i}"))).collect();
    let factions = ti4_engine::seating::seat_in_scope(&players);
    let mut state =
        ti4_engine::setup::start_game_seeded(content, &players, DEFAULT, None, 0).unwrap();
    for (p, f) in &factions {
        state.player_mut(p).unwrap().faction = f.clone();
    }
    let filler: Vec<String> = ti4_engine::seating::map_filler(content, 30, DEFAULT, 0)
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let refs: Vec<&str> = filler.iter().map(String::as_str).collect();
    let galaxy = ti4_engine::seating::build_board(content, &factions, &refs, DEFAULT).unwrap();
    for (p, f) in &factions {
        ti4_engine::seating::deploy(&mut state, content, p, f, DEFAULT).unwrap();
    }

    let mut table = Table::with_default(Box::new(SeededRandom::new(0)));
    let mut handles = BTreeMap::new();
    for (i, p) in players.iter().enumerate() {
        let faction = factions.get(p).map(ToString::to_string).unwrap_or_default();
        if std::env::args().any(|a| a == "--scored") {
            table.seat(
                p.clone(),
                Box::new(ti4_policy::bot::ScoredBot::new(
                    u64::try_from(i).unwrap_or(0),
                )),
            );
        } else {
            let bot =
                LearnedBot::new(blank_profile(&faction, DEFAULT_DIMENSIONS), i as u64).recording();
            handles.insert(p.clone(), bot.trajectory());
            table.seat(p.clone(), Box::new(bot));
        }
    }
    let mut game = Game::with_table(state, content, table).with_galaxy(galaxy);
    let _ = game.run(4, 500_000);

    let asked = game.table.log.records.len();
    let recorded: usize = handles.values().map(|h| h.borrow().len()).sum();
    println!(
        "engine asked          {asked} decisions   (policy: {})",
        if std::env::args().any(|a| a == "--scored") {
            "scored"
        } else {
            "blank learned"
        }
    );
    println!(
        "learned policy saw    {recorded}   ({:.0}%)",
        100.0 * f64::from(u32::try_from(recorded).unwrap_or(0))
            / f64::from(u32::try_from(asked.max(1)).unwrap_or(1))
    );
    println!("answered blind        {}", asked - recorded);
    let mut by_prompt: BTreeMap<String, usize> = BTreeMap::new();
    for r in &game.table.log.records {
        *by_prompt.entry(r.prompt.clone()).or_default() += 1;
    }
    let mut rows: Vec<(&String, usize)> = by_prompt.iter().map(|(k, v)| (k, *v)).collect();
    rows.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
    println!("\ncommonest prompts the engine raised:");
    for (p, n) in rows.iter().take(12) {
        println!("  {n:4}  {p}");
    }
}
