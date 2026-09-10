//! Drive the current Rust MLP from the latest TTS upload.
//!
//! Dry-run is the default. `--live` is deliberately limited to one engine step until the
//! post-command telemetry reconciler is complete; acknowledgements alone do not prove that the
//! next decision would see the same table Rust just produced.

use std::rc::Rc;

use ti4_bridge::{BridgeClient, Coordinator, TtsCommands};
use ti4_model::content_types::FULL;

fn argument(name: &str) -> Option<String> {
    let mut arguments = std::env::args().skip(1);
    while let Some(found) = arguments.next() {
        if found == name {
            return arguments.next();
        }
    }
    None
}

fn flag(name: &str) -> bool {
    std::env::args().any(|argument| argument == name)
}

fn refuse(reason: impl std::fmt::Display) -> ! {
    eprintln!("REFUSED: {reason}");
    std::process::exit(1)
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let steps: usize = argument("--steps").map_or(1, |value| {
        value.parse().unwrap_or_else(|_| refuse("--steps"))
    });
    let temperature: f64 = argument("--temperature").map_or(0.001, |value| {
        value.parse().unwrap_or_else(|_| refuse("--temperature"))
    });
    let seed: u64 = argument("--seed").map_or(20_260_910, |value| {
        value.parse().unwrap_or_else(|_| refuse("--seed"))
    });
    let live = flag("--live");
    if live && steps != 1 {
        refuse("live mode is one step until post-command telemetry reconciliation lands");
    }

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(format!("configuring CPU inference: {error}")));
    let content = ti4_content::ContentStore::embedded();
    let client = BridgeClient::default();
    let payload = client
        .latest()
        .unwrap_or_else(|error| refuse(format!("reading the latest TTS upload: {error}")));
    let telemetry: ti4_bridge::import::Telemetry =
        serde_json::from_value(serde_json::Value::Object(payload))
            .unwrap_or_else(|error| refuse(format!("decoding TTS telemetry: {error}")));
    let imported = ti4_bridge::import::import(content, &telemetry, FULL)
        .unwrap_or_else(|error| refuse(format!("importing the TTS table: {error}")));

    let mut translator = TtsCommands::new(
        imported
            .seats
            .iter()
            .map(|(colour, player)| (player.clone(), colour.name().to_owned())),
    )
    .with_neutral_colour("Brown");
    for system_id in imported.galaxy.system_ids() {
        if let Some(system) = ti4_content::galaxy::system(content, system_id, FULL) {
            for (offset, planet) in system.planets().into_iter().enumerate() {
                translator = translator.with_planet_index(
                    ti4_model::id::SystemId::new(system_id),
                    ti4_model::id::PlanetId::new(planet),
                    u32::try_from(offset + 1).expect("planet index fits"),
                );
            }
        }
    }

    let bundle = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(format!("reading {bundle_path}: {error}")));
    let actor = Rc::new(bundle.actor);
    let mut game =
        ti4_engine::game::Game::with_seeded_random(imported.state.clone(), content, seed)
            .with_galaxy(imported.galaxy.clone());
    for (index, player) in imported.state.players.iter().enumerate() {
        let row = ti4_mlp::FactionRow::of(player.id.as_str())
            .unwrap_or_else(|error| refuse(format!("{}: {error}", player.id)));
        let (decider, _) = ti4_mlp::bot::MlpBot::sharing(
            &actor,
            bundle.vocabulary.clone(),
            row,
            seed.wrapping_mul(1_000_003)
                .wrapping_add(u64::try_from(index).unwrap_or(0)),
        )
        .at_temperature(temperature)
        .seat();
        game.table.seat(player.id.clone(), decider);
    }

    let coordinator = Coordinator::new(client);
    println!("mode       {}", if live { "LIVE" } else { "dry-run" });
    println!("round      {}", game.state.round);
    println!("seats      {}", game.state.players.len());
    for player in &game.state.players {
        println!(
            "seat       {} strategy={:?}",
            player.id, player.strategy_cards
        );
    }
    for gap in &imported.gaps {
        println!("gap        {gap}");
    }
    for step in 0..steps {
        let before = game.state.clone();
        let result = game.step();
        if let Some(error) = result.error {
            refuse(format!("engine step {step}: {error}"));
        }
        let commands = translator
            .transition(&before, &game.state, content, FULL)
            .unwrap_or_else(|error| refuse(format!("translating step {step}: {error}")));
        println!("step       {step}: {} command(s)", commands.len());
        for command in &commands {
            println!(
                "  {}",
                serde_json::to_string(command).expect("command serialises")
            );
        }
        if live {
            let outcomes = coordinator
                .execute(&commands)
                .unwrap_or_else(|error| refuse(format!("executing step {step}: {error}")));
            println!("ack        {} successful result(s)", outcomes.len());
        }
    }
}
