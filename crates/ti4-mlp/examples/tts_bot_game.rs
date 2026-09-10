//! Offline acceptance runner for the authoritative-Rust TTS mode.
//!
//! This deliberately uses [`ti4_engine::game::Game`] directly. No Python engine constructs,
//! filters, renames, or applies a choice, so the decision surface is exactly the current Rust
//! simulation surface. TTS command mirroring will attach to the event stream after this gate.

use std::collections::BTreeMap;
use std::rc::Rc;

use sha2::{Digest, Sha256};
use ti4_engine::game::Game;
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["xxcha", "letnev", "sol", "l1z1x", "jolnar", "hacan"];

fn argument(name: &str) -> Option<String> {
    let mut arguments = std::env::args().skip(1);
    while let Some(found) = arguments.next() {
        if found == name {
            return arguments.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("REFUSED: {reason}");
    std::process::exit(1)
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let seed: u64 = argument("--seed").map_or(20_260_910, |value| {
        value.parse().unwrap_or_else(|_| refuse("--seed"))
    });
    let rounds: u32 = argument("--rounds").map_or(50, |value| {
        value.parse().unwrap_or_else(|_| refuse("--rounds"))
    });
    let max_steps: usize = argument("--max-steps").map_or(2_000_000, |value| {
        value.parse().unwrap_or_else(|_| refuse("--max-steps"))
    });
    let temperature: f64 = argument("--temperature").map_or(0.001, |value| {
        value.parse().unwrap_or_else(|_| refuse("--temperature"))
    });

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ti4_content::ContentStore::embedded();
    let players: Vec<PlayerId> = FACTIONS.iter().map(|name| PlayerId::new(*name)).collect();
    let factions: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .map(|player| (player.clone(), FactionId::new(player.as_str())))
        .collect();

    let mut state = ti4_engine::setup::start_game_seeded(content, &players, FULL, None, seed)
        .unwrap_or_else(|error| refuse(&format!("setup: {error}")));
    for (player, faction) in &factions {
        state.player_mut(player).expect("seat exists").faction = faction.clone();
    }
    let filler: Vec<String> = ti4_engine::seating::neutral_systems(content, 30, FULL)
        .into_iter()
        .map(|system| system.to_string())
        .collect();
    let borrowed: Vec<&str> = filler.iter().map(String::as_str).collect();
    let galaxy = ti4_engine::seating::build_board(content, &factions, &borrowed, FULL)
        .unwrap_or_else(|error| refuse(&format!("board: {error}")));
    for (player, faction) in &factions {
        ti4_engine::seating::deploy(&mut state, content, player, faction, FULL)
            .unwrap_or_else(|error| refuse(&format!("deploying {faction}: {error}")));
    }

    let bundle = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let actor = Rc::new(bundle.actor);
    let mut game = Game::with_seeded_random(state, content, seed).with_galaxy(galaxy);
    let mut statuses = Vec::new();
    for (index, player) in players.iter().enumerate() {
        let row = ti4_mlp::FactionRow::of(player.as_str())
            .unwrap_or_else(|error| refuse(&format!("{player}: {error}")));
        let (decider, status) = ti4_mlp::bot::MlpBot::sharing(
            &actor,
            bundle.vocabulary.clone(),
            row,
            seed.wrapping_mul(1_000_003)
                .wrapping_add(u64::try_from(index).unwrap_or(0)),
        )
        .at_temperature(temperature)
        .seat();
        game.table.seat(player.clone(), decider);
        statuses.push(status);
    }

    let error = game
        .run(rounds, max_steps)
        .err()
        .map(|error| error.to_string());
    let assigned: usize = statuses
        .iter()
        .map(|status| {
            status
                .counters()
                .assigned
                .load(std::sync::atomic::Ordering::Relaxed)
        })
        .sum();
    let oov: usize = statuses
        .iter()
        .map(|status| {
            status
                .counters()
                .oov
                .load(std::sync::atomic::Ordering::Relaxed)
        })
        .sum();

    println!("seed       {seed}");
    println!("round      {}", game.state.round);
    println!("phase      {:?}", game.state.phase);
    println!("finished   {}", game.state.finished);
    println!("decisions  {}", game.table.log.records.len());
    println!("events     {}", game.events.len());
    println!("features   {assigned} assigned, {oov} OOV");
    println!(
        "decision_fingerprint {}",
        hex_digest(&serde_json::to_vec(&game.table.log.records).expect("decision log serialises"))
    );
    println!(
        "event_fingerprint    {}",
        hex_digest(&serde_json::to_vec(&game.events).expect("event log serialises"))
    );
    println!(
        "state_fingerprint    {}",
        hex_digest(&serde_json::to_vec(&game.state).expect("game state serialises"))
    );
    for player in &game.state.players {
        println!(
            "seat       {:<8} {} VP",
            player.id.to_string(),
            player.victory_points
        );
    }
    if let Some(error) = error {
        refuse(&format!("game stopped early: {error}"));
    }
    if !game.state.finished {
        refuse("round horizon reached before the game finished");
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
