//! M10-033: critic warm-up on a distilled bundle.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example warm_up -- --bundle out/checkpoints/mlp/checkpoint-N
//! ```
//!
//! Loads a distilled schema-6 bundle, fits `V` on the corpus's captured four-round returns while
//! the actor stays frozen, and writes the result as a new bundle in `shared` critic mode — or
//! reports the fallback §6.2 pre-registers if the threshold is missed.

#![allow(
    clippy::collapsible_if,
    clippy::single_match_else,
    clippy::too_many_lines,
    reason = "a linear driver: load, warm up, verify, write"
)]

use sha2::Digest;
use ti4_mlp::bundle::{CriticMode, Provenance};
use ti4_mlp::critic_warmup::{CriticSample, Settings, warm_up, warm_up_separate};
use ti4_mlp::distill::Sample;
use ti4_mlp::{FactionRow, SparseOption};
use ti4_training::teacher_corpus::{Cluster, ExpectedCorpus, FIXED_POOL_SHA256, stream_shard};

/// How many decisions the logit fingerprint probes. A few hundred is plenty: the claim is that the
/// policy computation reads none of the changed parameters, so one counterexample is enough and
/// more probes only make the check slower.
const PROBES: usize = 256;

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

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| {
        // The highest complete checkpoint, per §4.6's recovery rule.
        ti4_mlp::bundle::latest_complete(std::path::Path::new("out/checkpoints/mlp"))
            .unwrap_or_else(|error| refuse(&format!("scanning for a bundle: {error}")))
            .map_or_else(
                || refuse("no complete bundle under out/checkpoints/mlp"),
                |path| path.display().to_string(),
            )
    });
    let corpus = argument("--corpus").unwrap_or_else(|| "out/corpus/teacher-v1".to_owned());
    let corpus = std::path::Path::new(&corpus);
    let out = argument("--out").unwrap_or_else(|| "out/checkpoints/mlp-critic".to_owned());

    ti4_tensor::configure_deterministic(20_260_821)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let mut actor = loaded.actor;
    let vocabulary = loaded.vocabulary;
    let slots_text = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("reading bundle slots.json: {error}")));
    let slots_sha256 = format!("{:x}", sha2::Sha256::digest(slots_text.as_bytes()));
    let expected_corpus = ExpectedCorpus {
        teacher_sha256: ti4_sim::baseline::R6_CHECKPOINT_SHA256,
        pool_sha256: FIXED_POOL_SHA256,
        slots_sha256: &slots_sha256,
    };
    ti4_training::teacher_corpus::validate_manifest(corpus, &expected_corpus)
        .unwrap_or_else(|error| refuse(&format!("validating the corpus manifest: {error}")));

    println!("M10-033 critic warm-up");
    println!("  bundle      {bundle_path}");
    println!("  from mode   {:?}", loaded.critic_mode);
    println!("  width       {}", actor.width());
    let rows = ti4_mlp::critic_warmup::critic_rows(&vocabulary);
    println!("  critic rows {}", rows.len());
    if rows.is_empty() {
        refuse("the vocabulary has no critic-state rows, so there is nothing to train");
    }

    // Load both splits. Critic samples for the fit; a fixed slice of policy decisions as probes.
    let mut probes: Vec<Sample> = Vec::new();
    let mut load = |cluster: Cluster| -> Vec<CriticSample> {
        let mut samples = Vec::new();
        let mut refused = 0usize;
        stream_shard(corpus, cluster, &expected_corpus, |decision| {
            let Ok(row) = FactionRow::of(&decision.faction) else {
                refused += 1;
                return;
            };
            if let Ok(sample) =
                CriticSample::new(row, &decision.critic, &vocabulary, decision.value_target)
            {
                samples.push(sample);
            } else {
                refused += 1;
            }
            if cluster == Cluster::Train && probes.len() < PROBES {
                if let Some(head) = ti4_mlp::heads().iter().position(|h| *h == decision.head) {
                    probes.push(Sample {
                        row,
                        head,
                        options: decision
                            .actor
                            .iter()
                            .map(|vector| {
                                let mut columns = Vec::with_capacity(vector.len());
                                let mut values = Vec::with_capacity(vector.len());
                                for (name, value) in vector {
                                    columns.push(
                                        i64::try_from(vocabulary.column_of(name)).unwrap_or(0),
                                    );
                                    #[allow(clippy::cast_possible_truncation)]
                                    values.push(*value as f32);
                                }
                                SparseOption { columns, values }
                            })
                            .collect(),
                        teacher: decision.teacher.clone(),
                    });
                }
            }
        })
        .unwrap_or_else(|error| {
            refuse(&format!("reading the {} shard: {error}", cluster.as_str()))
        });
        if refused > 0 {
            // A refusal here means a captured critic vector carried a non-critic name, which would
            // be a corpus defect rather than something to work around.
            refuse(&format!(
                "{refused} {} records were refused as critic samples",
                cluster.as_str()
            ));
        }
        samples
    };

    let started = std::time::Instant::now();
    let train = load(Cluster::Train);
    let validation = load(Cluster::Validation);
    println!(
        "  loaded      {} train, {} validation positions, {} probes, in {:.1?}",
        train.len(),
        validation.len(),
        probes.len(),
        started.elapsed()
    );
    if train.is_empty() || validation.is_empty() || probes.is_empty() {
        refuse("a split or the probe set is empty, so the warm-up would prove nothing");
    }

    let settings = Settings::default();
    println!(
        "\n  adam lr {} | batch {} | clip {} | <= {} epochs | threshold EV >= {}\n",
        settings.learning_rate,
        settings.batch,
        settings.clip,
        settings.max_epochs,
        settings.threshold
    );

    let result = warm_up(
        &mut actor,
        &vocabulary,
        &train,
        &validation,
        &probes,
        settings,
        |epoch| {
            println!(
                "  epoch {:>2}  train MSE {:>10.5}  validation MSE {:>10.5}  explained variance {:>8.4}",
                epoch.number, epoch.train_mse, epoch.validation_mse, epoch.explained_variance
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
        },
    )
    .unwrap_or_else(|error| refuse(&format!("critic warm-up failed: {error}")));

    let validate_run = |name: &str, run: &ti4_mlp::critic_warmup::WarmUp| {
        println!("\n  {name} moved  {:.6} L2", run.parameter_movement);
        println!(
            "  {name} logits {}",
            if run.logits_unchanged {
                "bit-identical"
            } else {
                "CHANGED"
            }
        );
        if !run.logits_unchanged {
            refuse(&format!("the {name} warm-up changed a policy logit"));
        }
        if run.parameter_movement <= 0.0 {
            refuse(&format!("the {name} warm-up trained nothing"));
        }
    };
    validate_run("shared", &result);

    let (mode, epoch) = if let Some(epoch) = result.selected {
        (CriticMode::Shared, epoch)
    } else {
        let best = result
            .epochs
            .iter()
            .map(|epoch| epoch.explained_variance)
            .fold(f64::NEG_INFINITY, f64::max);
        println!(
            "\n  shared missed: best explained variance {best:.4}; running the one permitted separate retry"
        );
        let separate = warm_up_separate(
            &mut actor,
            &vocabulary,
            &train,
            &validation,
            &probes,
            settings,
            |epoch| {
                println!(
                    "  separate {:>2} train MSE {:>10.5} validation MSE {:>10.5} EV {:>8.4}",
                    epoch.number, epoch.train_mse, epoch.validation_mse, epoch.explained_variance
                );
            },
        )
        .unwrap_or_else(|error| refuse(&format!("separate critic warm-up failed: {error}")));
        validate_run("separate", &separate);
        if let Some(epoch) = separate.selected {
            (CriticMode::Separate, epoch)
        } else {
            actor.set_separate_critic(None);
            println!("\n  separate missed; selecting the pre-registered batch-mean fallback");
            (CriticMode::BatchMean, 0)
        }
    };

    println!("  selected    {mode:?}, epoch {epoch}");
    let destination = std::path::Path::new(&out).join(format!(
        "checkpoint-{}-{epoch}",
        match mode {
            CriticMode::Shared => "shared",
            CriticMode::Separate => "separate",
            CriticMode::BatchMean => "batch-mean",
        }
    ));
    let bundle = ti4_mlp::bundle::write(
        &destination,
        &actor,
        &slots_text,
        mode,
        &Provenance {
            source: "M10-033 critic warm-up and fixed fallback".to_owned(),
            git_commit: std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unrecorded".to_owned()),
            update: u64::try_from(epoch).unwrap_or(0),
        },
    )
    .unwrap_or_else(|error| refuse(&format!("writing the selected critic bundle: {error}")));
    let reloaded = ti4_mlp::bundle::read(&bundle.directory)
        .unwrap_or_else(|error| refuse(&format!("reloading selected critic bundle: {error}")));
    if reloaded.critic_mode != mode {
        refuse("the reloaded bundle changed critic mode");
    }
    println!("  bundle      {}", bundle.directory.display());
}
