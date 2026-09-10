//! Offline acceptance runner for the authoritative-Rust TTS mode.
//!
//! This deliberately uses [`ti4_engine::game::Game`] directly. No Python engine constructs,
//! filters, renames, or applies a choice, so the decision surface is exactly the current Rust
//! simulation surface. TTS command mirroring will attach to the event stream after this gate.

use std::collections::BTreeMap;
use std::rc::Rc;

use sha2::{Digest, Sha256};
use ti4_bridge::{TtsCommands, UnitChange, UnitClass, UnitLocation, unit_changes};
use ti4_engine::game::Game;
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_model::state::GameState;

const FACTIONS: [&str; 6] = ["xxcha", "letnev", "sol", "l1z1x", "jolnar", "hacan"];
const COLOURS: [&str; 6] = ["White", "Blue", "Yellow", "Red", "Purple", "Green"];

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
    let mut translator = TtsCommands::new(
        players
            .iter()
            .zip(COLOURS)
            .map(|(player, colour)| (player.clone(), colour.to_owned())),
    )
    .with_neutral_colour("Brown");
    for system_id in galaxy.system_ids() {
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

    let target_round = game.state.round.saturating_add(rounds);
    let mut steps = 0usize;
    let mut unit_change_counts = BTreeMap::<&'static str, usize>::new();
    let mut pairing_counts = BTreeMap::<&'static str, usize>::new();
    let mut command_counts = BTreeMap::<String, usize>::new();
    let mut visible_delta_counts = BTreeMap::<&'static str, usize>::new();
    let mut error = None;
    while game.state.round < target_round && !game.state.finished && steps < max_steps {
        let before = game.state.clone();
        let result = game.step();
        let changes = unit_changes(&before, &game.state);
        summarize_visible_changes(&before, &game.state, &mut visible_delta_counts);
        if summarize_pairing(&changes, &mut pairing_counts) {
            eprintln!(
                "ambiguous step {steps} phase {:?}: {changes:#?}",
                game.state.phase
            );
        }
        match translator.transition(&before, &game.state, content, FULL) {
            Ok(commands) => {
                for command in commands {
                    *command_counts.entry(command.action).or_default() += 1;
                }
            }
            Err(found) => {
                error = Some(format!(
                    "translating step {steps}: {found}; changes={changes:#?}"
                ));
                break;
            }
        }
        for change in changes {
            let kind = match change {
                UnitChange::Damage { damaged: true, .. } => "damage",
                UnitChange::Damage { damaged: false, .. } => "repair",
                UnitChange::Add { .. } => "add",
                UnitChange::Remove { .. } => "remove",
            };
            *unit_change_counts.entry(kind).or_default() += 1;
        }
        if let Some(found) = result.error {
            error = Some(found.to_string());
            break;
        }
        steps += 1;
    }
    if steps == max_steps && !game.state.finished {
        error = Some(format!("step limit {max_steps} reached"));
    }
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
    println!("unit_delta {unit_change_counts:?}");
    println!("pairing    {pairing_counts:?}");
    println!("commands   {command_counts:?}");
    println!("visible    {visible_delta_counts:?}");
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

fn summarize_visible_changes(
    before: &GameState,
    after: &GameState,
    totals: &mut BTreeMap<&'static str, usize>,
) {
    let mut note = |name| *totals.entry(name).or_default() += 1;
    if before.speaker != after.speaker {
        note("speaker");
    }
    if before.unclaimed_strategy_cards != after.unclaimed_strategy_cards {
        note("strategy_pool");
    }
    if before.revealed_objectives != after.revealed_objectives {
        note("revealed_objectives");
    }
    if before.exhausted_planets != after.exhausted_planets {
        note("planet_exhaustion");
    }
    if before.frontier_tokens != after.frontier_tokens {
        note("frontier_tokens");
    }
    if before.planet_attachments != after.planet_attachments {
        note("planet_attachments");
    }
    for old in &before.players {
        let Some(new) = after.player(&old.id) else {
            note("players");
            continue;
        };
        if old.strategy_cards != new.strategy_cards {
            note("strategy_cards");
        }
        if old.exhausted_strategy_cards != new.exhausted_strategy_cards {
            note("strategy_card_exhaustion");
        }
        if (old.tactic_tokens, old.fleet_tokens, old.strategic_tokens)
            != (new.tactic_tokens, new.fleet_tokens, new.strategic_tokens)
        {
            note("command_pools");
        }
        if old.victory_points != new.victory_points {
            note("victory_points");
        }
        if old.trade_goods != new.trade_goods {
            note("trade_goods");
        }
        if old.commodities != new.commodities {
            note("commodities");
        }
        if old.technologies != new.technologies {
            note("technologies");
        }
        if old.exhausted_technologies != new.exhausted_technologies {
            note("technology_exhaustion");
        }
        if old.action_cards != new.action_cards {
            note("action_cards");
        }
        if old.secret_objectives != new.secret_objectives {
            note("secret_objectives");
        }
        if old.relics != new.relics {
            note("relics");
        }
        if old.exhausted_relics != new.exhausted_relics {
            note("relic_exhaustion");
        }
        if old.exploration_cards != new.exploration_cards {
            note("exploration_cards");
        }
    }
    let systems: std::collections::BTreeSet<_> = before
        .board
        .keys()
        .chain(after.board.keys())
        .cloned()
        .collect();
    for system in systems {
        let old = before.system_state(&system);
        let new = after.system_state(&system);
        if old.command_tokens != new.command_tokens {
            note("board_command_tokens");
        }
        if old.planet_control != new.planet_control {
            note("planet_control");
        }
        if old.purged_planets != new.purged_planets {
            note("purged_planets");
        }
    }
}

fn summarize_pairing(changes: &[UnitChange], totals: &mut BTreeMap<&'static str, usize>) -> bool {
    type Key = (UnitClass, bool);
    let mut adds = BTreeMap::<Key, Vec<(UnitLocation, usize)>>::new();
    let mut removes = BTreeMap::<Key, Vec<(UnitLocation, usize)>>::new();
    for change in changes {
        let (target, location, unit, damaged, count) = match change {
            UnitChange::Add {
                location,
                unit,
                damaged,
                count,
            } => (&mut adds, location, unit, damaged, count),
            UnitChange::Remove {
                location,
                unit,
                damaged,
                count,
            } => (&mut removes, location, unit, damaged, count),
            UnitChange::Damage { .. } => continue,
        };
        target
            .entry((unit.clone(), *damaged))
            .or_default()
            .push((location.clone(), *count));
    }
    let keys: std::collections::BTreeSet<_> = adds.keys().chain(removes.keys()).cloned().collect();
    let mut ambiguous = false;
    for key in keys {
        let added = adds.get(&key).map_or(0, |xs| xs.iter().map(|x| x.1).sum());
        let removed = removes
            .get(&key)
            .map_or(0, |xs| xs.iter().map(|x| x.1).sum());
        let paired = added.min(removed);
        *totals.entry("move_units").or_default() += paired;
        *totals.entry("place_units").or_default() += added - paired;
        *totals.entry("destroy_units").or_default() += removed - paired;
        if paired > 0
            && (adds.get(&key).is_some_and(|xs| xs.len() != 1)
                || removes.get(&key).is_some_and(|xs| xs.len() != 1))
        {
            ambiguous = true;
            *totals.entry("ambiguous_move_units").or_default() += paired;
            *totals.entry("ambiguous_move_steps").or_default() += 1;
        }
    }
    ambiguous
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
