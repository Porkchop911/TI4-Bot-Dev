//! Prove that an offline corpus shard reconstructs the MLP's exact sparse inputs.
//!
//! This is intentionally stronger than JSON/schema validation: it resolves every stored feature
//! name through the checkpoint vocabulary, verifies the stored column, rebuilds `SparseOption`s,
//! runs the faction-conditioned decision head, and computes one-hot behavior-cloning NLL from the
//! recorded chosen index.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde::Deserialize;
use ti4_mlp::{FactionRow, SparseOption};

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
    let shard = corpus.join("decisions.jsonl.zst");
    let file = std::fs::File::open(&shard)
        .map_err(|error| format!("opening {}: {error}", shard.display()))?;
    let decoder = zstd::stream::read::Decoder::new(BufReader::new(file))
        .map_err(|error| format!("opening zstd stream: {error}"))?;

    let mut decisions = 0usize;
    let mut options = 0usize;
    let mut non_forced = 0usize;
    let mut nll = 0.0f64;
    let mut minimum_chosen_probability = 1.0f64;
    for (line_number, line) in BufReader::new(decoder).lines().enumerate() {
        if decisions >= limit {
            break;
        }
        let line = line.map_err(|error| format!("reading line {}: {error}", line_number + 1))?;
        let decision: Decision = serde_json::from_str(&line)
            .map_err(|error| format!("parsing line {}: {error}", line_number + 1))?;
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
        let head = ti4_mlp::Actor::resolve_head(&decision.head);
        ti4_mlp::Actor::head_index(head)
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
