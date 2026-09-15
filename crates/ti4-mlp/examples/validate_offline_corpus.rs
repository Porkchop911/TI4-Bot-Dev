//! Prove that an offline corpus shard reconstructs the MLP's exact sparse inputs.
//!
//! This is intentionally stronger than JSON/schema validation: it resolves every stored feature
//! name through the checkpoint vocabulary, verifies the stored column, rebuilds `SparseOption`s,
//! runs the faction-conditioned decision head, and computes one-hot behavior-cloning NLL from the
//! recorded chosen index.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;

use serde::Deserialize;
use sha2::Digest;
use ti4_mlp::{FactionRow, SparseOption};

const CAPTURE_SCHEMA_V1: &str = "ti4-offline-selfplay-v1";
const CAPTURE_SCHEMA_V2: &str = "ti4-offline-selfplay-v2";
const OBSERVATION_SCHEMA_V1: &str = "seat-authorized-canonical-mlp-v1";
const OBSERVATION_SCHEMA_V2: &str = "seat-authorized-canonical-mlp-v2";
const DIPLOMACY_SCHEMA: &str = "ti4-diplomacy-log-v1";

#[derive(Deserialize)]
struct Manifest {
    schema: String,
    observation_schema: String,
    games: usize,
    records: BTreeMap<String, usize>,
    shards: BTreeMap<String, String>,
    #[serde(default)]
    diplomacy_enabled: bool,
}

#[derive(Deserialize)]
struct GameRecord {
    game_id: String,
}

#[derive(Deserialize)]
struct Feature {
    name: String,
    column: usize,
    value: f64,
}

#[derive(Deserialize)]
struct Action {
    actor_features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Decision {
    game_id: String,
    faction: String,
    head: String,
    legal_actions: Vec<Action>,
    chosen_action_index: usize,
}

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: impl std::fmt::Display) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn digest(path: &std::path::Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("opening {}: {error}", path.display()))?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("reading {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn checked_shard(
    corpus: &std::path::Path,
    manifest: &Manifest,
    name: &str,
) -> Result<PathBuf, String> {
    let expected = manifest
        .shards
        .get(name)
        .ok_or_else(|| format!("manifest does not authenticate {name}"))?;
    let path = corpus.join(name);
    let actual = digest(&path)?;
    if &actual != expected {
        return Err(format!("checksum mismatch for {}", path.display()));
    }
    Ok(path)
}

fn zstd_lines(
    path: &std::path::Path,
) -> Result<impl Iterator<Item = Result<String, String>>, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("opening {}: {error}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(BufReader::new(file))
        .map_err(|error| format!("opening {} zstd stream: {error}", path.display()))?;
    Ok(BufReader::new(decoder)
        .lines()
        .map(|line| line.map_err(|error| error.to_string())))
}

fn main() {
    if let Err(error) = run() {
        refuse(error);
    }
}

fn run() -> Result<(), String> {
    let corpus =
        PathBuf::from(argument("--corpus").unwrap_or_else(|| "E:/ti4-corpus/pilot-v1".to_owned()));
    let checkpoint = PathBuf::from(argument("--checkpoint").unwrap_or_else(|| {
        "out/blank-shaped-4layers/shaped-r1bonus3-20260912/checkpoint-318956".to_owned()
    }));
    let limit = argument("--decisions")
        .map_or(Ok(2_048usize), |value| value.parse::<usize>())
        .map_err(|_| "--decisions must be a positive integer".to_owned())?;
    if limit == 0 {
        return Err("--decisions must be non-zero".to_owned());
    }

    ti4_tensor::configure_deterministic(1_026_091_300)
        .map_err(|error| format!("configuring tensor backend: {error}"))?;
    let loaded = ti4_mlp::bundle::read(&checkpoint)
        .map_err(|error| format!("reading {}: {error}", checkpoint.display()))?;
    let manifest_path = corpus.join("manifest.json");
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|error| format!("reading {}: {error}", manifest_path.display()))?,
    )
    .map_err(|error| format!("parsing {}: {error}", manifest_path.display()))?;
    match (
        manifest.schema.as_str(),
        manifest.observation_schema.as_str(),
    ) {
        (CAPTURE_SCHEMA_V1, OBSERVATION_SCHEMA_V1) => {
            if manifest.diplomacy_enabled {
                return Err("v1 corpus cannot enable diplomacy".to_owned());
            }
        }
        (CAPTURE_SCHEMA_V2, OBSERVATION_SCHEMA_V2) => {
            if manifest.diplomacy_enabled
                && loaded.actor.head_layout() != ti4_mlp::HeadLayout::Diplomacy
            {
                return Err("diplomacy corpus requires a schema-9/10 checkpoint".to_owned());
            }
        }
        _ => return Err("unsupported or mismatched corpus/observation schemas".to_owned()),
    }
    if manifest.records.get("games") != Some(&manifest.games) {
        return Err("manifest games count is inconsistent".to_owned());
    }
    let games_shard = checked_shard(&corpus, &manifest, "games.jsonl.zst")?;
    let shard = checked_shard(&corpus, &manifest, "decisions.jsonl.zst")?;
    let mut game_ids = BTreeSet::new();
    for line in zstd_lines(&games_shard)? {
        let game: GameRecord = serde_json::from_str(&line?)
            .map_err(|error| format!("parsing game metadata: {error}"))?;
        if !game_ids.insert(game.game_id) {
            return Err("duplicate game id in metadata shard".to_owned());
        }
    }
    if game_ids.len() != manifest.games {
        return Err("games shard count does not match manifest".to_owned());
    }

    if manifest.schema == CAPTURE_SCHEMA_V2 {
        let diplomacy_shard = checked_shard(&corpus, &manifest, "diplomacy.jsonl.zst")?;
        let mut deal_ids = BTreeSet::new();
        let mut diplomacy_records = 0_usize;
        for line in zstd_lines(&diplomacy_shard)? {
            let record: ti4_model::DiplomacyLogRecord = serde_json::from_str(&line?)
                .map_err(|error| format!("parsing diplomacy record: {error}"))?;
            if record.schema != DIPLOMACY_SCHEMA {
                return Err(format!("unsupported diplomacy schema {}", record.schema));
            }
            if !game_ids.contains(&record.game_id) {
                return Err(format!(
                    "diplomacy record references unknown game {}",
                    record.game_id
                ));
            }
            if !deal_ids.insert((record.game_id.clone(), record.deal_id)) {
                return Err("duplicate deal id within a game".to_owned());
            }
            diplomacy_records += 1;
        }
        if manifest.records.get("diplomacy_deals") != Some(&diplomacy_records) {
            return Err("diplomacy shard count does not match manifest".to_owned());
        }
    }

    let mut decisions = 0usize;
    let mut options = 0usize;
    let mut non_forced = 0usize;
    let mut nll = 0.0f64;
    let mut minimum_chosen_probability = 1.0f64;
    for (line_number, line) in zstd_lines(&shard)?.enumerate() {
        if decisions >= limit {
            break;
        }
        let line = line.map_err(|error| format!("reading line {}: {error}", line_number + 1))?;
        let decision: Decision = serde_json::from_str(&line)
            .map_err(|error| format!("parsing line {}: {error}", line_number + 1))?;
        if !game_ids.contains(&decision.game_id) {
            return Err(format!(
                "line {} references unknown game {}",
                line_number + 1,
                decision.game_id
            ));
        }
        if decision.legal_actions.len() < 2 {
            continue;
        }
        if decision.chosen_action_index >= decision.legal_actions.len() {
            return Err(format!(
                "line {} chosen index is out of range",
                line_number + 1
            ));
        }
        let sparse: Vec<SparseOption> = decision
            .legal_actions
            .iter()
            .map(|action| {
                let mut columns = Vec::with_capacity(action.actor_features.len());
                let mut values = Vec::with_capacity(action.actor_features.len());
                for feature in &action.actor_features {
                    let resolved = loaded.vocabulary.column_of(&feature.name);
                    if resolved != feature.column {
                        return Err(format!(
                            "line {} feature {:?}: stored column {}, checkpoint resolves {}",
                            line_number + 1,
                            feature.name,
                            feature.column,
                            resolved
                        ));
                    }
                    columns.push(i64::try_from(feature.column).map_err(|_| {
                        format!("line {} column does not fit i64", line_number + 1)
                    })?);
                    #[expect(clippy::cast_possible_truncation, reason = "model inputs are f32")]
                    let value = feature.value as f32;
                    if !value.is_finite() {
                        return Err(format!("line {} has a non-finite feature", line_number + 1));
                    }
                    values.push(value);
                }
                Ok(SparseOption { columns, values })
            })
            .collect::<Result<_, String>>()?;
        let row = FactionRow::of(&decision.faction)
            .map_err(|error| format!("line {}: {error}", line_number + 1))?;
        if decision.head == "diplomacy" && manifest.schema == CAPTURE_SCHEMA_V1 {
            return Err(format!(
                "line {} gives diplomacy supervision to a v1 corpus",
                line_number + 1
            ));
        }
        let head = loaded.actor.resolve_layout_head(&decision.head);
        loaded
            .actor
            .layout_head_index(head)
            .map_err(|error| format!("line {}: {error}", line_number + 1))?;
        let probabilities = loaded
            .actor
            .probabilities(&sparse, head, row, 1.0)
            .map_err(|error| format!("line {} forward pass: {error}", line_number + 1))?;
        if probabilities.len() != sparse.len()
            || probabilities
                .iter()
                .any(|probability| !probability.is_finite() || *probability < 0.0)
        {
            return Err(format!(
                "line {} produced malformed probabilities",
                line_number + 1
            ));
        }
        let chosen = probabilities[decision.chosen_action_index];
        if chosen <= 0.0 {
            return Err(format!(
                "line {} chosen probability is not positive",
                line_number + 1
            ));
        }
        nll -= chosen.ln();
        minimum_chosen_probability = minimum_chosen_probability.min(chosen);
        options += sparse.len();
        non_forced += 1;
        decisions += 1;
    }
    if decisions == 0 {
        return Err("the shard yielded no non-forced decisions".to_owned());
    }
    #[expect(clippy::cast_precision_loss, reason = "validation counts are bounded")]
    let mean_nll = nll / decisions as f64;
    println!("offline corpus tensor validation passed");
    println!("  corpus                       {}", corpus.display());
    println!("  checkpoint                   {}", checkpoint.display());
    println!("  non-forced decisions         {non_forced}");
    println!("  reconstructed legal options {options}");
    println!("  finite one-hot BC mean NLL   {mean_nll:.6}");
    println!("  minimum chosen probability  {minimum_chosen_probability:.3e}");
    Ok(())
}
