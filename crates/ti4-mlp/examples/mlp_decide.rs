//! Answer one decision with the trained policy, from a real table.
//!
//! ```text
//! cargo run -p ti4-mlp --example mlp_decide -- \
//!     --bundle out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-241428 \
//!     --capture out/bridge-captures/<file>.json \
//!     --choice  a-choice.json
//! ```
//!
//! This is the seam between the Python driver and the Rust policy. `tools/play_tts.py` already
//! reads the table, seats a game, offers each bot its legal choices and carries the answer back to
//! TTS. The only thing it cannot do is ask the *trained* checkpoint, because the MLP scores a
//! decision against an observation built by the Rust engine and refuses to answer without one.
//!
//! So the split is: Python owns the table and the legality, Rust owns the judgement.
//!
//! - The **choice** comes from Python. `engine.choice.Option` and `ti4_engine::ChoiceOption` are
//!   direct ports — `id`, `kind`, `label`, `payload` on both sides — so it crosses as JSON.
//! - The **state** is rebuilt here from the same telemetry the driver read, because the MLP's
//!   features come from the Rust engine's own view of the board.
//! - The **answer** goes back as an option id, and [`ti4_engine::choice::ask_private`] validates it
//!   was on offer before it is printed. Rust cannot invent an option Python did not present.
//!
//! It prints the assigned/out-of-vocabulary counters alongside the answer, which is the measurement
//! that says whether a Python-originated choice featurises at all: a decision answered entirely
//! from OOV columns is a coin flip wearing a policy's coat.

use std::collections::BTreeMap;

use ti4_engine::choice::{Choice, ask_private};
use ti4_model::content_types::FULL;
use ti4_model::id::PlayerId;

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

// A diagnostic that reads top to bottom in the order it prints; splitting it into helpers each
// called once would scatter that order across the file.
#[expect(clippy::too_many_lines, reason = "a report reads in printing order")]
#[expect(
    clippy::cast_precision_loss,
    reason = "feature counts are far below f64's exact-integer range"
)]
fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    // `--simulated` builds a fresh engine game instead of reading a table, so the *same* code path
    // can be run against a state the policy would have seen in training. Coverage on an imported
    // board is only interpretable next to this number.
    let simulated = std::env::args().any(|a| a == "--simulated");
    let capture_path = if simulated {
        String::new()
    } else {
        argument("--capture").unwrap_or_else(|| refuse("--capture is required"))
    };
    let choice_path = argument("--choice");
    let temperature: f64 = argument("--temperature").map_or(0.001, |value| {
        value.parse().unwrap_or_else(|_| refuse("--temperature"))
    });

    // -- the table -------------------------------------------------------------------------
    // The bundle records the thread settings it was built under and refuses to load into a
    // different backend, so this has to come before any bundle read.
    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let store = ti4_content::ContentStore::embedded();
    let imported = if simulated {
        let seats: Vec<ti4_model::id::PlayerId> =
            ["hacan", "jolnar", "l1z1x", "letnev", "sol", "xxcha"]
                .iter()
                .map(|name| ti4_model::id::PlayerId::new(*name))
                .collect();
        let state = ti4_engine::setup::start_game(store, &seats, FULL, None)
            .unwrap_or_else(|error| refuse(&format!("setting up a simulated game: {error}")));
        let tiles: Vec<String> = ti4_content::galaxy::all_systems(store, FULL)
            .keys()
            .take(37)
            .map(|id| (*id).to_owned())
            .collect();
        let refs: Vec<&str> = tiles.iter().map(String::as_str).collect();
        let galaxy = ti4_content::galaxy::Galaxy::build(store, &refs, FULL, 3)
            .unwrap_or_else(|error| refuse(&format!("building a simulated galaxy: {error}")));
        // Advance into the action phase. A fresh game is in the strategy phase, and comparing a
        // strategy-card pick against an imported action-phase decision would compare decision
        // families rather than states -- the confound this whole probe exists to avoid.
        let mut game = ti4_engine::game::Game::with_seeded_random(state, store, 20_260_909)
            .with_galaxy(galaxy.clone());
        let mut steps = 0;
        while game.state.phase != ti4_model::state::Phase::Action && steps < 4_000 {
            let result = game.step();
            if result.finished || result.error.is_some() {
                break;
            }
            steps += 1;
        }
        println!(
            "(simulated: stepped {steps} time(s) into {:?})
",
            game.state.phase
        );
        ti4_bridge::import::Imported {
            state: game.state.clone(),
            galaxy,
            seats: std::collections::BTreeMap::new(),
            gaps: Vec::new(),
        }
    } else {
        let telemetry_text = std::fs::read_to_string(&capture_path)
            .unwrap_or_else(|error| refuse(&format!("reading {capture_path}: {error}")));
        let telemetry: ti4_bridge::import::Telemetry = serde_json::from_str(&telemetry_text)
            .unwrap_or_else(|error| refuse(&format!("{capture_path} is not telemetry: {error}")));
        ti4_bridge::import::import(store, &telemetry, FULL)
            .unwrap_or_else(|error| refuse(&format!("importing the table: {error}")))
    };

    // -- the decision ----------------------------------------------------------------------
    //
    // With `--choice` the decision comes from the Python driver, which is the shipping shape:
    // Python owns legality, Rust owns judgement. Without it, the Rust engine is asked what it
    // would offer on this imported board -- which is the sharper test, because an *imported*
    // state carries gaps a simulated one never has (no hands, no exhaustion, an inferred phase),
    // and a state that cannot even produce a choice would fail long before featurisation.
    let choice: Choice = if let Some(path) = &choice_path {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|error| refuse(&format!("reading {path}: {error}")));
        serde_json::from_str(&text)
            .unwrap_or_else(|error| refuse(&format!("{path} is not a choice: {error}")))
    } else {
        let game = ti4_engine::game::Game::new(imported.state.clone(), store)
            .with_galaxy(imported.galaxy.clone());
        match game.legal_options() {
            Some(choice) => {
                println!(
                    "(no --choice given; asking the engine what it offers here)
    "
                );
                choice
            }
            None => refuse("the engine offers no choice on this board"),
        }
    };

    let seat = choice.player.clone();
    if !imported.state.players.iter().any(|p| p.id == seat) {
        refuse(&format!(
            "the choice is for {seat}, who is not at this table: {:?}",
            imported
                .state
                .players
                .iter()
                .map(|p| p.id.to_string())
                .collect::<Vec<_>>()
        ));
    }

    // -- the policy ------------------------------------------------------------------------
    let bundle = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let row = ti4_mlp::FactionRow::of(seat.as_str())
        .unwrap_or_else(|error| refuse(&format!("{seat} has no faction row: {error}")));
    let actor = std::rc::Rc::new(bundle.actor);
    let vocabulary = bundle.vocabulary.clone();
    let (mut bot, status) =
        ti4_mlp::bot::MlpBot::sharing(&actor, bundle.vocabulary.clone(), row, 0)
            .at_temperature(temperature)
            .seat();

    println!(
        "table   round {}, {} tiles",
        imported.state.round,
        imported.galaxy.system_ids().len()
    );
    println!("seat    {seat}");
    println!(
        "choice  {} option(s): {}",
        choice.options.len(),
        choice.prompt
    );
    println!("policy  {bundle_path} at temperature {temperature}");

    let answer = ask_private(
        &choice,
        &imported.state,
        store,
        FULL,
        Some(&imported.galaxy),
        &mut *bot,
    );

    // The counters are per-occurrence, so they measure this decision alone.
    let counters = status.counters();
    let assigned = counters.assigned.load(std::sync::atomic::Ordering::Relaxed);
    let oov = counters.oov.load(std::sync::atomic::Ordering::Relaxed);
    let total = assigned + oov;

    match answer {
        Ok(option) => {
            println!("\nchose   {} ({})", option.id, option.label);
            println!(
                "\nfeatures {assigned} assigned, {oov} out of vocabulary ({:.1}% known)",
                if total == 0 {
                    0.0
                } else {
                    100.0 * assigned as f64 / total as f64
                }
            );
            if total > 0 && assigned * 4 < total {
                println!(
                    "  WARNING: most of this decision was scored from OOV columns, which is a\n\
                                coin flip wearing a policy's coat. The choice does not featurise."
                );
            }
        }
        Err(error) => {
            println!("\nrefused {error}");
            println!("features {assigned} assigned, {oov} out of vocabulary");
            std::process::exit(1);
        }
    }

    // Which feature names the vocabulary does not carry, by family.
    //
    // The aggregate says *how much* of a decision was scored blind; only the names say what to do
    // about it. Two earlier attempts at this were wrong: one invented feature prefixes that do not
    // exist, and one compared a strategy-card pick against an action-phase decision. This asks the
    // projection for the actual features and checks each against the bundle's own vocabulary.
    let seen =
        ti4_engine::choice::Observed::new(&imported.state, store, FULL, Some(&imported.galaxy));
    let baseline = ti4_policy::progress::Baseline {
        planets: 0,
        units: 0,
        technologies: 0,
    };
    let vectors = ti4_policy::projection::mlp_choice_features(&seen, &choice, &seat, &[], baseline);
    let mut families: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    let mut seen_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for vector in &vectors {
        for key in vector.keys() {
            let name = ti4_policy::intern::name_of(*key);
            if !seen_names.insert(name.clone()) {
                continue;
            }
            if vocabulary.is_assigned(&name) {
                continue;
            }
            let family = ti4_policy::vocabulary::family_of(&name).to_owned();
            let entry = families.entry(family).or_insert_with(|| (0, Vec::new()));
            entry.0 += 1;
            if entry.1.len() < 5 {
                entry.1.push(name);
            }
        }
    }
    if families.is_empty() {
        println!(
            "
every distinct feature name is in the vocabulary"
        );
    } else {
        println!(
            "
distinct feature names the vocabulary does not carry, by family:"
        );
        let mut ranked: Vec<_> = families.iter().collect();
        ranked.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
        for (family, (count, examples)) in ranked {
            println!("  {family:<24} {count:>3}  e.g. {examples:?}");
        }
    }

    // A per-kind breakdown, so a choice family that does not featurise is named rather than
    // averaged away by the ones that do.
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for option in &choice.options {
        *kinds.entry(option.kind.as_str()).or_default() += 1;
    }
    println!("\noption kinds offered: {kinds:?}");
    let _ = PlayerId::new("unused");
}
