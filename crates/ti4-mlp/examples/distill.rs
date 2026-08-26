//! M10-032: multi-teacher factual distillation.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example distill -- [--width 256|128] [--epochs 20]
//! ```
//!
//! Reads the fixed teacher corpus, compiles every decision to dense columns once, runs phase 0,
//! and writes the selected epoch's weights as a schema-6 bundle.

use std::collections::BTreeMap;

use sha2::Digest;
use ti4_mlp::bundle::{CriticMode, Provenance};
use ti4_mlp::distill::{Sample, Settings, initialize, train};
use ti4_mlp::{FactionRow, SparseOption, Width};
use ti4_policy::vocabulary::Vocabulary;
use ti4_training::teacher_corpus::{Cluster, Decision, stream_shard};

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

/// Compile one captured decision into the column form training consumes.
///
/// Returns `None` for a decision this build cannot represent — an unknown faction or head — rather
/// than substituting a default. A silently redirected decision trains the wrong row.
fn compile(decision: &Decision, vocabulary: &Vocabulary, heads: &[&str]) -> Option<Sample> {
    let row = FactionRow::of(&decision.faction).ok()?;
    let head = heads.iter().position(|name| *name == decision.head)?;
    let options = decision
        .actor
        .iter()
        .map(|vector| {
            let mut columns = Vec::with_capacity(vector.len());
            let mut values = Vec::with_capacity(vector.len());
            for (name, value) in vector {
                columns.push(i64::try_from(vocabulary.column_of(name)).unwrap_or(0));
                #[allow(clippy::cast_possible_truncation)]
                values.push(*value as f32);
            }
            SparseOption { columns, values }
        })
        .collect();
    Some(Sample {
        row,
        head,
        options,
        teacher: decision.teacher.clone(),
    })
}

fn load(
    directory: &std::path::Path,
    cluster: Cluster,
    vocabulary: &Vocabulary,
    heads: &[&str],
) -> (Vec<Sample>, usize) {
    // Streamed: materialising the training shard would hold roughly 27 GB of feature *names*
    // that this function converts to columns and drops immediately.
    let mut samples: Vec<Sample> = Vec::new();
    let total = stream_shard(directory, cluster, |decision| {
        if let Some(sample) = compile(&decision, vocabulary, heads) {
            samples.push(sample);
        }
    })
    .unwrap_or_else(|error| refuse(&format!("reading the {} shard: {error}", cluster.as_str())));
    (samples, total)
}

#[allow(
    clippy::too_many_lines,
    reason = "a linear driver: load, train, write, verify"
)]
fn main() {
    let width = match argument("--width").as_deref() {
        None | Some("256") => Width::W256,
        Some("128") => Width::W128,
        Some(other) => refuse(&format!("--width {other}: only 256 and 128 exist")),
    };
    let corpus = argument("--corpus").unwrap_or_else(|| "out/corpus/teacher-v1".to_owned());
    let corpus = std::path::Path::new(&corpus);
    let out = argument("--out").unwrap_or_else(|| "out/checkpoints/mlp".to_owned());

    let backend = ti4_tensor::configure_deterministic(20_260_821)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    // The corpus names the vocabulary it was captured against. Distilling against a different one
    // would resolve every feature to a different column, which no later check would notice.
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(corpus.join("manifest.json"))
            .unwrap_or_else(|error| refuse(&format!("reading the corpus manifest: {error}"))),
    )
    .unwrap_or_else(|error| refuse(&format!("the corpus manifest is not JSON: {error}")));
    let corpus_slots = manifest
        .get("slots_sha256")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| refuse("the corpus manifest names no vocabulary"));

    let generation = ti4_training::vocabulary_corpus::accepted_generation(std::path::Path::new(
        "out/vocabulary",
    ))
    .unwrap_or_else(|error| refuse(&format!("no accepted vocabulary generation: {error}")));
    let slots_text = std::fs::read_to_string(&generation.slots)
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
    let slots_sha256 = format!("{:x}", sha2::Sha256::digest(slots_text.as_bytes()));
    if slots_sha256 != corpus_slots {
        refuse(&format!(
            "the corpus was captured against vocabulary {corpus_slots}, the accepted one is \
             {slots_sha256}"
        ));
    }
    let vocabulary = Vocabulary::from_json(&slots_text)
        .unwrap_or_else(|error| refuse(&format!("slots.json does not load: {error}")));
    let capacity = i64::try_from(vocabulary.capacity()).unwrap_or(i64::MAX);
    let heads = ti4_mlp::heads();

    println!("M10-032 factual distillation");
    println!("  corpus      {}", corpus.display());
    println!("  vocabulary  {slots_sha256}");
    println!(
        "  width       {} | capacity {capacity} | slots {}",
        width.dim(),
        vocabulary.slot_count()
    );
    println!(
        "  backend     intra-op {} inter-op {}",
        backend.intra_op_threads, backend.inter_op_threads
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());

    let started = std::time::Instant::now();
    let (train_samples, train_total) = load(corpus, Cluster::Train, &vocabulary, heads);
    let (validation_samples, validation_total) =
        load(corpus, Cluster::Validation, &vocabulary, heads);
    println!(
        "  loaded      {} train, {} validation decisions in {:.1?}",
        train_samples.len(),
        validation_samples.len(),
        started.elapsed()
    );
    // A decision this build cannot represent is dropped by `compile`. Silence about that would let
    // a head rename quietly shrink the corpus, so it is reported and bounded.
    let dropped =
        (train_total - train_samples.len()) + (validation_total - validation_samples.len());
    if dropped > 0 {
        println!("  WARNING     {dropped} decisions did not compile and were dropped");
    }
    if train_samples.is_empty() || validation_samples.is_empty() {
        refuse("a split is empty, so distillation would measure nothing");
    }
    let _ = std::io::Write::flush(&mut std::io::stdout());

    // Only rows a feature can actually reach are initialised; the rest stay zero.
    let active: Vec<i64> = (0..i64::try_from(vocabulary.slot_count()).unwrap_or(0)).collect();
    let mut actor = initialize(width, capacity, &active);

    let settings = Settings {
        max_epochs: argument("--epochs")
            .and_then(|value| value.parse().ok())
            .unwrap_or(20),
        ..Settings::default()
    };
    println!(
        "\n  adam lr {} betas ({}, {}) eps {} wd {} | batch {} | clip {} | <= {} epochs, patience {}\n",
        settings.learning_rate,
        settings.beta1,
        settings.beta2,
        settings.eps,
        settings.weight_decay,
        settings.batch,
        settings.clip,
        settings.max_epochs,
        settings.patience
    );

    let trained = std::time::Instant::now();
    let result = train(
        &mut actor,
        &train_samples,
        &validation_samples,
        settings,
        |epoch| {
            let worst = epoch
                .per_faction
                .iter()
                .max_by(|left, right| left.1.total_cmp(right.1));
            println!(
                "  epoch {:>2}  train KL {:>8.5}  validation KL {:>8.5}  steps {:>6}  worst {}",
                epoch.number,
                epoch.train_kl,
                epoch.validation_kl,
                epoch.steps,
                worst.map_or_else(String::new, |(faction, kl)| format!("{faction} {kl:.5}"))
            );
            // Rust block-buffers stdout when it is redirected, so a multi-hour run would report
            // nothing at all until it exited. Flushed per epoch so progress is watchable.
            let _ = std::io::Write::flush(&mut std::io::stdout());
        },
    );

    // Non-vacuity for the whole run, checked rather than eyeballed: `Adam::step` skips a parameter
    // whose gradient is undefined, so a break anywhere between the loss and the leaf tensors would
    // still produce a complete, plausible table of KLs above.
    if result.parameter_movement <= 0.0 {
        refuse("the weights did not move; the run reported KLs it never trained toward");
    }

    println!("\n  stopped     {}", result.stopped);
    println!(
        "  moved       {:.4} (L2 from initialisation)",
        result.parameter_movement
    );
    println!("  selected    epoch {}", result.selected);
    println!("  wall time   {:.1?}", trained.elapsed());

    let selected = result
        .epochs
        .iter()
        .find(|epoch| epoch.number == result.selected)
        .unwrap_or_else(|| refuse("the selected epoch is not in the record"));
    println!("\n  validation KL by faction at the selected epoch");
    for (faction, kl) in &selected.per_faction {
        println!("    {faction:<10} {kl:.5}");
    }

    // Non-vacuity, stated as a check rather than left to the reader: distillation that did not
    // move the student is not distillation, however clean the run looked.
    let first = result.epochs.first().map_or(f64::NAN, |e| e.validation_kl);
    if result.epochs.len() > 1 && selected.validation_kl >= first {
        println!(
            "\n  WARNING: the selected epoch is no better than the first ({:.5} against {first:.5})",
            selected.validation_kl
        );
    }

    let destination = std::path::Path::new(&out).join(format!("checkpoint-{}", selected.steps));
    let bundle = ti4_mlp::bundle::write(
        &destination,
        &actor,
        &slots_text,
        CriticMode::BatchMean,
        &Provenance {
            source: "M10-032 factual distillation".to_owned(),
            git_commit: std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unrecorded".to_owned()),
            update: u64::try_from(selected.steps).unwrap_or(0),
        },
    )
    .unwrap_or_else(|error| refuse(&format!("writing the bundle: {error}")));
    println!("\n  bundle      {}", bundle.directory.display());
    println!("  manifest    {}", bundle.manifest_sha256);

    // Read it back through the verifying loader before claiming a checkpoint exists.
    let loaded = ti4_mlp::bundle::read(&bundle.directory)
        .unwrap_or_else(|error| refuse(&format!("the bundle does not load: {error}")));
    let reloaded: BTreeMap<String, f64> =
        ti4_mlp::distill::evaluate(&loaded.actor, &validation_samples);
    let reloaded_kl = ti4_mlp::distill::mean_of_means(&reloaded);
    println!("  reloaded    validation KL {reloaded_kl:.5}");
    if (reloaded_kl - selected.validation_kl).abs() > 1e-6 {
        refuse(&format!(
            "the reloaded bundle scores {reloaded_kl:.6}, the selected epoch scored {:.6}",
            selected.validation_kl
        ));
    }
}
