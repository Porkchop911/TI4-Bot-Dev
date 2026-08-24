//! Single-game decision trace for oracle-parity diffing (T5).
//!
//! Plays one deterministic six-faction game from an explicit checkpoint profile table and, at
//! every decision, records: seat/faction, prompt, legal options with per-option raw scores and
//! probabilities, the path taken (`seeing` or `blind`) and the option chosen. The output is a
//! JSON array meant to be diffed against the Python oracle's matching trace
//! (`out/diff_py_game.py`).
//!
//! Usage:
//!   `single_game_trace` --checkpoint <oracle-profile.json> --seed <u64> [--rotation n]
//!                     [--rounds 4] [--greedy-temperature f]
//!                     [--map-pool <save52 pool path>]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{Value, json};
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice};
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::FeatureVector;
use ti4_policy::inference::LearnedBot;
use ti4_policy::learned::{Profile, decision_head};
use ti4_training::rollout::{Horizon, OpeningMap, play_with_deciders};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

/// The per-seat decision log, shared with the trace writer.
struct SeatLog {
    log: Rc<RefCell<Vec<Value>>>,
}

struct TraceBot {
    inner: LearnedBot,
    faction: String,
    log: Rc<RefCell<Vec<Value>>>,
    full_features: bool,
}

impl Decider for TraceBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let options = option_rows(choice);
        let picked = self.inner.choose(choice);
        push(
            &self.faction,
            &self.log,
            choice,
            &options,
            None,
            "blind",
            &picked,
            None,
        );
        picked
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &ti4_engine::choice::SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let (features, probabilities) = self.inner.consider(
            seen.observed(),
            choice,
            &seen.held_secret_progress(),
        );
        // Raw scores from the same public primitives `consider` uses internally.
        let requested_head = decision_head(choice);
        let resolved_head = self.inner.profile().resolved_head(requested_head);
        let temperature = self
            .inner
            .profile()
            .head(resolved_head)
            .map_or(1.0, |head| head.temperature);
        let scores: BTreeMap<String, f64> = features
            .iter()
            .map(|(id, vector)| {
                (
                    id.clone(),
                    self.inner.profile().score_vector(resolved_head, vector),
                )
            })
            .collect();
        // Owned before the mutable sampling call below; `resolved_head` borrows the bot.
        let resolved_name = resolved_head.to_owned();
        let mut options = option_rows(choice);
        for row in &mut options {
            if let Some(score) = scores.get(row["id"].as_str().unwrap_or("")) {
                row["score"] = json!(round6(*score));
            }
            if let Some(probability) = probabilities.get(row["id"].as_str().unwrap_or("")) {
                row["prob"] = json!(probability);
            }
        }
        let raw = if self.full_features {
            Some(&features)
        } else {
            None
        };
        let picked = self.inner.choose_seeing(choice, seen);
        push(
            &self.faction,
            &self.log,
            choice,
            &options,
            Some(
                json!({"head": requested_head, "resolved": resolved_name, "temperature": temperature}),
            ),
            "seeing",
            &picked,
            raw,
        );
        picked
    }
}

fn option_rows(choice: &Choice) -> Vec<Value> {
    choice
        .options
        .iter()
        .map(|option| json!({"id": option.id, "kind": option.kind}))
        .collect()
}

fn push(
    faction: &str,
    log: &Rc<RefCell<Vec<Value>>>,
    choice: &Choice,
    options: &[Value],
    head_info: Option<Value>,
    path: &str,
    picked: &Result<ChoiceOption, IllegalChoice>,
    raw_features: Option<&BTreeMap<String, FeatureVector>>,
) {
    let mut record = json!({
        "faction": faction,
        "idx": log.borrow().len(),
        "path": path,
        "prompt": choice.prompt.chars().take(120).collect::<String>(),
        "n_options": options.len(),
        "options": options,
        "head": Value::from(head_info),
        "chosen": picked.as_ref().ok().map(|option| option.id.clone()),
    });
    if let Some(features) = raw_features {
        record["raw"] = serde_json::to_value(features).expect("features serialize");
    }
    log.borrow_mut().push(record);
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

#[allow(
    clippy::too_many_lines,
    reason = "a diagnostic CLI; splitting it would obscure the trace flow"
)]
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let checkpoint = arg(&arguments, "--checkpoint").unwrap_or_else(|| {
        eprintln!("missing --checkpoint");
        std::process::exit(2);
    });
    // Optional in dump mode; required for a real trace.
    let seed: u64 = arg(&arguments, "--seed")
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| {
            if arguments.iter().any(|argument| argument == "--dump-head") {
                0
            } else {
                eprintln!("missing --seed");
                std::process::exit(2);
            }
        });
    let rotation: usize = arg(&arguments, "--rotation")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let rounds: u32 = arg(&arguments, "--rounds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let greedy =
        arg(&arguments, "--greedy-temperature").and_then(|value| value.parse::<f64>().ok());
    let full_features = arguments
        .iter()
        .any(|argument| argument == "--full-features");

    let content = ContentStore::embedded();
    let bytes = std::fs::read(&checkpoint).unwrap_or_else(|error| {
        eprintln!("read {checkpoint}: {error}");
        std::process::exit(1);
    });
    let document: Value =
        serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("parse checkpoint: {error}"));
    let table = document
        .get("learner_profiles")
        .or_else(|| document.get("profiles"))
        .or_else(|| document.get("accepted"))
        .unwrap_or(&document);
    let loaded: BTreeMap<String, Profile> = serde_json::from_value(table.clone())
        .unwrap_or_else(|error| panic!("read profile table: {error}"));

    // Diagnostics: write the deserialized weights of one head per faction, for a direct
    // comparison against the checkpoint file.
    if let Some(head_name) = arg(&arguments, "--dump-head") {
        let out_path = arg(&arguments, "--dump-out").expect("missing --dump-out");
        let mut dump = serde_json::Map::new();
        for (faction, profile) in &loaded {
            let value = match profile.learned.heads.get(&head_name) {
                Some(head) => {
                    json!({"temperature": head.temperature,
                           "weights": serde_json::to_value(&head.weights).expect("weights")})
                }
                None => json!({"missing": true}),
            };
            dump.insert(faction.clone(), value);
        }
        std::fs::write(
            &out_path,
            serde_json::to_string_pretty(&dump).expect("serialize dump"),
        )
        .unwrap_or_else(|error| panic!("write {out_path}: {error}"));
        return;
    }

    // Seat order follows the oracle's rotated_six_player_seats: position i holds faction
    // (i + rotation) % 6 of the canonical list.
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let mut factions = BTreeMap::new();
    for (index, player) in players.iter().enumerate() {
        factions.insert(
            player.clone(),
            FactionId::new(FACTIONS[(index + rotation % FACTIONS.len()) % FACTIONS.len()]),
        );
    }

    let pool = match arg(&arguments, "--map-pool") {
        Some(path) => {
            let path = std::path::Path::new(&path);
            ti4_sim::MapPool::load(path).unwrap_or_else(|error| panic!("pool: {error}"))
        }
        None => panic!("missing --map-pool"),
    };
    pool.validate_systems(content, FULL)
        .unwrap_or_else(|error| panic!("pool validate: {error}"));

    let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
    let mut seat_logs: BTreeMap<PlayerId, SeatLog> = BTreeMap::new();
    for (index, player) in players.iter().enumerate() {
        let faction = factions.get(player).expect("seated").to_string();
        let profile = loaded
            .get(&faction)
            .unwrap_or_else(|| panic!("no profile for {faction}"))
            .clone();
        assert!(
            profile.is_explicit(),
            "{}: schema {} is hashed; trace requires explicit profiles",
            faction,
            profile.schema
        );
        // In-memory temperature override only (the checkpoint file is never touched).
        let profile = match greedy {
            Some(temperature) => {
                let mut adjusted = profile.clone();
                for head in adjusted.learned.heads.values_mut() {
                    head.temperature = temperature;
                }
                adjusted
            }
            None => profile,
        };
        // Same per-seat sampling stream the training path uses: seed * 1_000_003 + index.
        let stream = seed
            .wrapping_mul(1_000_003)
            .wrapping_add(u64::try_from(index).unwrap_or(0));
        let log = Rc::new(RefCell::new(Vec::<Value>::new()));
        deciders.insert(
            player.clone(),
            Box::new(TraceBot {
                inner: LearnedBot::from_shared(std::sync::Arc::new(profile), stream),
                faction: faction.clone(),
                log: Rc::clone(&log),
                full_features,
            }),
        );
        seat_logs.insert(player.clone(), SeatLog { log });
    }

    let map = OpeningMap::PythonPool {
        pool: std::sync::Arc::new(pool),
        tile_seed_offset: TILE_SEED_OFFSET,
    };
    let rollout = play_with_deciders(
        content,
        &players,
        &factions,
        FULL,
        seed,
        Horizon::rounds(rounds),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        &map,
        deciders,
    );

    if let Some(error) = &rollout.error {
        eprintln!("game error: {error}");
        std::process::exit(1);
    }

    // Merge the six per-faction logs into one stream ordered by decision index. The engines are
    // only comparable when their decision order matches, so keep each faction's own index and let
    // the diff script align on (idx).
    // Per-faction decision streams; `idx` is that faction's own decision counter. The diff
    // script aligns on (faction, idx) — the two engines are only comparable when both agree.
    let mut merged: Vec<Value> = Vec::new();
    for info in seat_logs.values() {
        merged.extend(info.log.borrow().iter().cloned());
    }
    for seat in &rollout.seats {
        println!(
            "seat {} {}: final vp={} cleared={}",
            seat.player,
            seat.faction,
            seat.episode.final_progress.victory_points,
            seat.episode.cleared
        );
    }
    println!("{}", serde_json::to_string(&merged).expect("serialize"));
}
