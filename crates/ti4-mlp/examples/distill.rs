//! M10-032: multi-teacher factual distillation.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example distill -- [--width 256|128] [--epochs 20]
//! ```
//!
//! Reads the fixed teacher corpus, compiles every decision to dense columns once, runs phase 0,
//! and writes the selected epoch's weights as a schema-6 bundle.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use sha2::Digest;
use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_mlp::bundle::{CriticMode, Provenance};
use ti4_mlp::distill::{Sample, Settings, initialize, train};
use ti4_mlp::{FactionRow, SparseOption, Width};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::vocabulary::Vocabulary;
use ti4_training::teacher_corpus::{
    Cluster, Decision, ExpectedCorpus, FIXED_POOL_SHA256, stream_shard,
};

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
/// A decision this build cannot represent is an error, never a record to drop.
fn compile(decision: &Decision, vocabulary: &Vocabulary, heads: &[&str]) -> Result<Sample, String> {
    let row = FactionRow::of(&decision.faction)
        .map_err(|error| format!("unknown faction {}: {error}", decision.faction))?;
    let head = heads
        .iter()
        .position(|name| *name == decision.head)
        .ok_or_else(|| format!("unknown head {}", decision.head))?;
    let options = decision
        .actor
        .iter()
        .map(|vector| {
            let mut columns = Vec::with_capacity(vector.len());
            let mut values = Vec::with_capacity(vector.len());
            for (name, value) in vector {
                if !vocabulary.is_assigned(name) {
                    return Err(format!(
                        "feature {name} is not assigned in the accepted vocabulary"
                    ));
                }
                columns.push(i64::try_from(vocabulary.column_of(name)).unwrap_or(0));
                #[allow(clippy::cast_possible_truncation)]
                values.push(*value as f32);
            }
            Ok(SparseOption { columns, values })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Sample {
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
    expected: &ExpectedCorpus<'_>,
) -> Result<Vec<Sample>, String> {
    // Streamed: materialising the training shard would hold roughly 27 GB of feature *names*
    // that this function converts to columns and drops immediately.
    let mut samples: Vec<Sample> = Vec::new();
    let mut first_error = None;
    stream_shard(directory, cluster, expected, |decision| {
        if first_error.is_none() {
            match compile(&decision, vocabulary, heads) {
                Ok(sample) => samples.push(sample),
                Err(error) => first_error = Some(error),
            }
        }
    })
    .map_err(|error| format!("reading the {} shard: {error}", cluster.as_str()))?;
    if let Some(error) = first_error {
        return Err(format!(
            "the {} shard contains an unrepresentable decision: {error}",
            cluster.as_str()
        ));
    }
    Ok(samples)
}

const GAMEPLAY_SEEDS: std::ops::Range<u64> = 380_000_000..380_000_200;
const GAMEPLAY_ROUNDS: u32 = 4;
const TILE_SEED_OFFSET: u64 = 20_000_000;
const VALIDATION_POOL_SHA256: &str =
    "aba33c81aa04cefb15857b8ed1d40173f6f3de5e9b6e9633a6855c1d5a4c27e5";
const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

fn gameplay_mean_vp(
    actor: Option<&ti4_mlp::Actor>,
    vocabulary: &Vocabulary,
    champions: &BTreeMap<String, ti4_policy::learned::Profile>,
    pool: &Arc<ti4_sim::MapPool>,
) -> Result<f64, String> {
    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let mut total_vp = 0i64;
    let mut seats = 0usize;

    for seed in GAMEPLAY_SEEDS {
        for rotation in 0..FACTIONS.len() {
            let seated: BTreeMap<PlayerId, FactionId> = players
                .iter()
                .enumerate()
                .map(|(index, player)| {
                    (
                        player.clone(),
                        FactionId::new(FACTIONS[(index + rotation) % FACTIONS.len()]),
                    )
                })
                .collect();
            let mut statuses = Vec::new();
            let deciders: BTreeMap<PlayerId, Box<dyn Decider>> = players
                .iter()
                .enumerate()
                .map(|(index, player)| {
                    let stream = seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(u64::try_from(index).unwrap_or(0));
                    let decider = if let Some(actor) = actor {
                        let row = FactionRow::of(seated[player].as_str())
                            .expect("fixed faction is in the roster");
                        let (bot, status) = ti4_mlp::bot::MlpBot::new(
                            actor.inference_copy(),
                            vocabulary.clone(),
                            row,
                            stream,
                        )
                        .seat();
                        statuses.push(status);
                        bot
                    } else {
                        let profile = champions
                            .get(seated[player].as_str())
                            .ok_or_else(|| {
                                format!("teacher has no profile for {}", seated[player])
                            })?
                            .clone();
                        Box::new(ti4_policy::inference::LearnedBot::from_shared(
                            Arc::new(profile),
                            stream,
                        )) as Box<dyn Decider>
                    };
                    Ok((player.clone(), decider))
                })
                .collect::<Result<_, String>>()?;
            let rollout = ti4_training::rollout::play_with_deciders(
                content,
                &players,
                &seated,
                DEFAULT,
                seed,
                ti4_training::rollout::Horizon {
                    rounds: GAMEPLAY_ROUNDS,
                    steps: 10_000,
                },
                ti4_engine::opening::DEFAULT_REQUIREMENT,
                &ti4_training::rollout::OpeningMap::PythonPool {
                    pool: Arc::clone(pool),
                    tile_seed_offset: TILE_SEED_OFFSET,
                },
                deciders,
            );
            if let Some(error) = rollout.error {
                return Err(format!("game {seed}/{rotation} failed: {error}"));
            }
            for status in statuses {
                status
                    .into_result()
                    .map_err(|error| format!("game {seed}/{rotation}: {error}"))?;
            }
            for seat in rollout.seats {
                total_vp += seat.episode.final_progress.victory_points;
                seats += 1;
            }
        }
    }
    if seats != GAMEPLAY_SEEDS.count() * FACTIONS.len() * FACTIONS.len() {
        return Err(format!("gameplay panel produced {seats} seat results"));
    }
    #[expect(clippy::cast_precision_loss, reason = "panel totals are exact in f64")]
    Ok(total_vp as f64 / seats as f64)
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

    let generation = ti4_training::vocabulary_corpus::accepted_generation(std::path::Path::new(
        "out/vocabulary",
    ))
    .unwrap_or_else(|error| refuse(&format!("no accepted vocabulary generation: {error}")));
    let slots_text = std::fs::read_to_string(&generation.slots)
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
    let slots_sha256 = format!("{:x}", sha2::Sha256::digest(slots_text.as_bytes()));
    let expected_corpus = ExpectedCorpus {
        teacher_sha256: ti4_sim::baseline::R6_CHECKPOINT_SHA256,
        pool_sha256: FIXED_POOL_SHA256,
        slots_sha256: &slots_sha256,
    };
    ti4_training::teacher_corpus::validate_manifest(corpus, &expected_corpus)
        .unwrap_or_else(|error| refuse(&format!("validating the corpus manifest: {error}")));
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
    let train_samples = load(corpus, Cluster::Train, &vocabulary, heads, &expected_corpus)
        .unwrap_or_else(|error| refuse(&error));
    let validation_samples = load(
        corpus,
        Cluster::Validation,
        &vocabulary,
        heads,
        &expected_corpus,
    )
    .unwrap_or_else(|error| refuse(&error));
    println!(
        "  loaded      {} train, {} validation decisions in {:.1?}",
        train_samples.len(),
        validation_samples.len(),
        started.elapsed()
    );
    if train_samples.is_empty() || validation_samples.is_empty() {
        refuse("a split is empty, so distillation would measure nothing");
    }
    let _ = std::io::Write::flush(&mut std::io::stdout());

    // Only rows the factual corpus actually reaches are initialised. Initialising every assigned
    // slot would put random weights in objective/ability/critic rows before those phases are
    // enabled, contradicting §6.1's explicit zero-extension contract.
    let active: Vec<i64> = train_samples
        .iter()
        .chain(&validation_samples)
        .flat_map(|sample| sample.options.iter())
        .flat_map(|option| option.columns.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if active.is_empty() {
        refuse("the factual corpus reaches no input rows");
    }
    // §7.1's one permitted switch. Distillation is the cleanest case for it: there are no rollouts
    // at all, only optimisation over a corpus already captured on CPU, so nothing that selects an
    // action moves off the deterministic path.
    let optimizer_device = match argument("--device").as_deref() {
        None | Some("cpu") => ti4_tensor::OptimizerDevice::Cpu,
        Some("cuda") => ti4_tensor::OptimizerDevice::Cuda,
        Some(other) => refuse(&format!("--device {other}: expected cpu or cuda")),
    };
    let device = optimizer_device
        .resolve()
        .unwrap_or_else(|error| refuse(&format!("--device cuda: {error}")));
    println!("  optimiser   {device:?}");

    let mut actor = initialize(width, capacity, &active).to_device(device);

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
    )
    .unwrap_or_else(|error| refuse(&format!("distillation failed: {error}")));

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

    let imitation = ti4_mlp::distill::validation_metrics(&actor, &validation_samples)
        .unwrap_or_else(|error| refuse(&format!("imitation validation failed: {error}")));
    println!(
        "\n  imitation   mean KL {:.5}, top-1 {:.3}%",
        imitation.mean_kl,
        imitation.top1_agreement * 100.0
    );
    for (head, kl) in &imitation.per_head {
        println!("    {head:<14} KL {kl:.5}");
    }
    if imitation.per_head.len() != heads.len() {
        refuse(&format!(
            "validation covers {} schema-4 heads, expected {}",
            imitation.per_head.len(),
            heads.len()
        ));
    }
    // §6.1's imitation gate, less top-1 agreement.
    //
    // **Operator decision D-2026-08-26-1.** §6.1 requires top-1 agreement >= 97%; this run measured
    // 93.7% at a mean KL of 0.00129 nats. At that KL the two distributions are all but identical,
    // so the disagreements are argmax flips on options the teacher holds near-tied — which fitting
    // harder cannot remove and DAgger does not address, since DAgger corrects distribution drift
    // rather than tie-breaking.
    //
    // The operator's ruling is that agreement is not a meaningful acceptance criterion here because
    // PPO deliberately diverges from the teacher from this point on. Recorded rather than deleted:
    // the number is still computed and printed every run, so a future reader sees what was set
    // aside and can reinstate it.
    //
    // The distribution gates keep their force: mean KL and the per-head bound both still refuse.
    if imitation.mean_kl > 0.02 || imitation.per_head.values().any(|kl| *kl > 0.05) {
        refuse("the selected checkpoint fails a predeclared §6.1 imitation gate");
    }
    if imitation.top1_agreement < 0.97 {
        println!(
            "  NOTE        top-1 agreement {:.3}% is below §6.1's 97%; waived by operator decision              D-2026-08-26-1 (we diverge from the teacher from here)",
            imitation.top1_agreement * 100.0
        );
    }

    // The gameplay gate is part of selection, not post-publication commentary. Load only the
    // logical validation pool and the exact accepted teacher, then compare both policies on the
    // predeclared paired seeds before a bundle path is created.
    let checkpoint_path =
        argument("--teacher").unwrap_or_else(|| "out/stage2_r6/final10000.json".to_owned());
    let checkpoint = std::fs::read(&checkpoint_path)
        .unwrap_or_else(|error| refuse(&format!("reading {checkpoint_path}: {error}")));
    let teacher_sha256 = format!("{:x}", sha2::Sha256::digest(&checkpoint));
    if teacher_sha256 != ti4_sim::baseline::R6_CHECKPOINT_SHA256 {
        refuse("the gameplay teacher is not the accepted r6 checkpoint");
    }
    let champions =
        ti4_training::vocabulary_corpus::champion_profiles(&checkpoint, &checkpoint_path)
            .unwrap_or_else(|error| refuse(&format!("loading gameplay teachers: {error}")));
    let pool_path = argument("--validation-pool")
        .unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Validation],
    )
    .unwrap_or_else(|error| refuse(&format!("reading validation pool: {error}")));
    let pool_sha256 = format!("{:x}", sha2::Sha256::digest(&pool_bytes));
    if pool_sha256 != VALIDATION_POOL_SHA256 {
        refuse("the gameplay pool is not the pinned seed-777 validation pool");
    }
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing validation pool: {error}"))),
    );
    println!("\n  gameplay    running paired 200 seeds x 6 rotations");
    let teacher_vp = gameplay_mean_vp(None, &vocabulary, &champions, &pool)
        .unwrap_or_else(|error| refuse(&format!("teacher gameplay gate: {error}")));
    let student_vp = gameplay_mean_vp(Some(&actor), &vocabulary, &champions, &pool)
        .unwrap_or_else(|error| refuse(&format!("student gameplay gate: {error}")));
    let vp_delta = (student_vp - teacher_vp).abs();
    println!(
        "  gameplay    teacher {teacher_vp:.4} VP, student {student_vp:.4} VP, |delta| {vp_delta:.4}"
    );
    if vp_delta > 0.1 {
        refuse(
            "the base pass fails the gameplay gate; do not publish or begin PPO (a predeclared \
             DAgger round is required)",
        );
    }
    println!("  DAgger      not required: the base pass cleared both exit gates");

    let destination = std::path::Path::new(&out).join(format!("checkpoint-{}", selected.steps));
    // §4.4: "Weights are always stored on CPU in the file and moved at load, so a checkpoint
    // written from a CUDA run loads on a CPU-only machine."
    let actor = actor.to_device(ti4_tensor::Device::Cpu);
    let git_commit = std::env::var("GIT_COMMIT")
        .unwrap_or_else(|_| refuse("GIT_COMMIT is required to publish a bundle"));
    let bundle = ti4_mlp::bundle::write(
        &destination,
        &actor,
        &slots_text,
        CriticMode::BatchMean,
        &Provenance {
            source: "M10-032 factual distillation".to_owned(),
            git_commit,
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
        ti4_mlp::distill::evaluate(&loaded.actor, &validation_samples)
            .unwrap_or_else(|error| refuse(&format!("reloaded evaluation failed: {error}")));
    let reloaded_kl = ti4_mlp::distill::mean_of_means(&reloaded);
    println!("  reloaded    validation KL {reloaded_kl:.5}");
    if (reloaded_kl - selected.validation_kl).abs() > 1e-6 {
        refuse(&format!(
            "the reloaded bundle scores {reloaded_kl:.6}, the selected epoch scored {:.6}",
            selected.validation_kl
        ));
    }
}
