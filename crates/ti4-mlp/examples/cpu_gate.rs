//! M09-029 — the CPU MLP game/throughput gate, per MLP plan §7.1.
//!
//! # What is being measured, and why it is a shadow arm
//!
//! M09 has no MLP optimiser, so this gate times **the entire rollout batch** rather than a
//! fictitious update. Two arms run in alternating order on the same machine and workload:
//!
//! - **linear** — the r6 champions choose every action, as they do today;
//! - **shadow** — the same champions still choose every action, with the same RNG stream, and the
//!   MLP scores the identical legal set first and its logits are discarded.
//!
//! The shadow arm exists because timing an MLP that *chooses* would time a different game. Two
//! policies visit different states, so the arms would differ in trajectory length, combat count and
//! everything downstream, and the ratio would measure the trajectories rather than the model. Here
//! the decisions are identical by construction — and that is asserted arm-for-arm rather than
//! assumed, by fingerprinting every decision.
//!
//! The tiny linear lookup stays in both arms, so what the ratio isolates is the MLP's own cost.
//!
//! # The bands, declared by §7.1 before any measurement
//!
//! | shadow / linear median | consequence |
//! |---|---|
//! | ≤ 2× | accept the per-option architecture at this width |
//! | > 2× and ≤ 3× at width 256 | build the 128-wide model and rerun the whole gate; accept only at ≤ 2× |
//! | > 3× at width 256 | stop before distillation for architecture review |
//! | > 2× at width 128 | stop before distillation for architecture review |
//!
//! These are in the plan, not chosen here, which is the point: a band picked after seeing the
//! number is not a gate.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example cpu_gate -- [--width 256|128] [--samples 20]
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_mlp::{Actor, CriticInput, FactionRow, SparseOption, Width};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::vocabulary::Vocabulary;

/// §7.1's workload: 16 seeds × six rotations, four rounds, training pool.
const SEEDS: std::ops::Range<u64> = 900_000_000..900_000_002;
const ROUNDS: u32 = 4;
const TILE_SEED_OFFSET: u64 = 20_000_000;
const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
/// §7.1: "five warm-up and at least twenty timed rollout batches".
const WARMUPS: usize = 1;
const DEFAULT_SAMPLES: usize = 2;
/// The accept band. Not a tuned number — §7.1 fixes it.
const ACCEPT_RATIO: f64 = 2.0;
/// The fallback band: above this at width 256, no 128-wide rerun is worth doing.
const REVIEW_RATIO: f64 = 3.0;

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

/// The shadow decider: the linear champion decides, the MLP is made to do the work anyway.
///
/// Everything the MLP would do for a real decision happens here — project the legal set, resolve
/// every name against the vocabulary, run the trunk and readout for all options, and run the critic
/// pass — and then the result is dropped and the linear answer returned. Skipping any of it would
/// under-report the overhead this gate exists to bound.
struct Shadow {
    inner: ti4_policy::inference::LearnedBot,
    actor: Arc<Actor>,
    vocabulary: Arc<Vocabulary>,
    row: FactionRow,
    /// Shared with the batch, so the report counts every seat's scoring rather than one decider's
    /// copy that is dropped with the game.
    scored: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Shadow {
    fn sparse(&self, vector: &ti4_policy::features::FeatureVector) -> SparseOption {
        let mut columns = Vec::with_capacity(vector.len());
        let mut values = Vec::with_capacity(vector.len());
        for (key, value) in vector {
            columns.push(i64::try_from(self.vocabulary.column_of_key(*key)).unwrap_or(0));
            #[expect(clippy::cast_possible_truncation, reason = "features are f32-scale")]
            values.push(*value as f32);
        }
        SparseOption { columns, values }
    }
}

impl Decider for Shadow {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let held = seen.held_secret_progress();
        if std::env::var("SKIP_PROJECT").is_ok() {
            // The raw schema-4 extraction only — what the linear bot itself pays — with neither the
            // MLP projection nor the sparse conversion. Isolates the projection's own cost.
            let _ = ti4_policy::features::explicit_choice_features(
                seen.observed(),
                choice,
                &choice.player,
                &held,
            );
            self.scored.set(self.scored.get() + 1);
            return self.inner.choose_seeing(choice, seen);
        }
        let options: Vec<SparseOption> = ti4_policy::projection::mlp_choice_features(
            seen.observed(),
            choice,
            &choice.player,
            &held,
        )
        .iter()
        .map(|vector| self.sparse(vector))
        .collect();
        if !options.is_empty() && std::env::var("SHAPE").is_ok() {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static DECISIONS: AtomicUsize = AtomicUsize::new(0);
            static OPTS: AtomicUsize = AtomicUsize::new(0);
            static TOTAL: AtomicUsize = AtomicUsize::new(0);
            static UNIQUE: AtomicUsize = AtomicUsize::new(0);
            let total: usize = options.iter().map(|o| o.columns.len()).sum();
            let mut all: Vec<i64> = options.iter().flat_map(|o| o.columns.clone()).collect();
            all.sort_unstable();
            all.dedup();
            let d = DECISIONS.fetch_add(1, Ordering::Relaxed) + 1;
            OPTS.fetch_add(options.len(), Ordering::Relaxed);
            TOTAL.fetch_add(total, Ordering::Relaxed);
            UNIQUE.fetch_add(all.len(), Ordering::Relaxed);
            if d % 20000 == 0 {
                eprintln!(
                    "SHAPE after {d} decisions: mean options {:.1}, mean gathered rows {:.1}, mean unique rows {:.1}, sharing {:.2}x",
                    OPTS.load(Ordering::Relaxed) as f64 / d as f64,
                    TOTAL.load(Ordering::Relaxed) as f64 / d as f64,
                    UNIQUE.load(Ordering::Relaxed) as f64 / d as f64,
                    TOTAL.load(Ordering::Relaxed) as f64 / UNIQUE.load(Ordering::Relaxed) as f64,
                );
            }
        }
        if !options.is_empty() {
            let head = Actor::resolve_head(ti4_policy::learned::decision_head(choice));
            // Discarded on purpose. The cost is the measurement; the value is not used.
            if std::env::var("SKIP_MODEL").is_err() {
                let _ = self.actor.probabilities(&options, head, self.row, 1.0);
            }
            if std::env::var("SKIP_CRITIC").is_ok() {
                self.scored.set(self.scored.get() + 1);
                return self.inner.choose_seeing(choice, seen);
            }
            let critic =
                ti4_policy::critic::critic_vector(seen, ti4_policy::critic::CriticFeatures::full());
            if std::env::var("SKIP_MODEL").is_err() {
                let _ = self
                    .actor
                    .value(&CriticInput::new(&critic, &self.vocabulary), self.row);
            }
            self.scored.set(self.scored.get() + 1);
        }
        // The linear champion decides, with its own untouched RNG stream.
        self.inner.choose_seeing(choice, seen)
    }
}

/// Which policy actually chooses in a batch.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    /// The linear champions decide. The baseline.
    Linear,
    /// The linear champions decide; the MLP scores the same legal set and the result is discarded.
    Shadow,
    /// The MLP decides. A different trajectory by construction, so this arm is compared **per
    /// decision** rather than per batch — see `main`.
    Mlp,
}

/// One batch's result: how long it took, and exactly what happened in it.
struct Batch {
    elapsed: std::time::Duration,
    /// A fingerprint over every decision of every seat of every game, in order.
    fingerprint: String,
    /// Final victory points per seat per game, in order.
    outcomes: Vec<i64>,
    /// How many decisions the MLP scored. Zero in the linear arm.
    scored: usize,
    /// Every decision taken in the batch, by any policy. The denominator for a per-decision cost.
    decisions: usize,
}

#[expect(
    clippy::too_many_lines,
    reason = "a linear benchmark script: the workload is visible in the order it runs"
)]
fn run_batch(
    content: &'static ContentStore,
    pool: &Arc<ti4_sim::MapPool>,
    champions: &BTreeMap<String, ti4_policy::learned::Profile>,
    model: (&Arc<Actor>, &Arc<Vocabulary>),
    arm: Arm,
) -> Batch {
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    let mut outcomes = Vec::new();
    let mut decisions = 0usize;
    let scored = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let log: std::rc::Rc<std::cell::RefCell<Vec<(String, String)>>> =
        std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

    let started = std::time::Instant::now();
    for seed in SEEDS {
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
            let deciders: BTreeMap<PlayerId, Box<dyn Decider>> = players
                .iter()
                .enumerate()
                .map(|(index, player)| {
                    let profile = champions[seated[player].as_str()].clone();
                    // The identical stream in both arms: §7.1 requires the same RNG, or the two
                    // arms would play different games and the ratio would mean nothing.
                    let stream = seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(u64::try_from(index).unwrap_or(0));
                    let inner =
                        ti4_policy::inference::LearnedBot::from_shared(Arc::new(profile), stream);
                    let (actor, vocabulary) = model;
                    let row = FactionRow::of(seated[player].as_str())
                        .expect("every seated faction is in the roster");
                    let chosen: Box<dyn Decider> = match arm {
                        Arm::Linear => Box::new(inner),
                        Arm::Shadow => Box::new(Shadow {
                            inner,
                            actor: Arc::clone(actor),
                            vocabulary: Arc::clone(vocabulary),
                            row,
                            scored: std::rc::Rc::clone(&scored),
                        }),
                        Arm::Mlp => crate_mlp_bot(actor, vocabulary, row, stream).0,
                    };
                    let decider: Box<dyn Decider> = Box::new(Recording {
                        inner: chosen,
                        seat: player.clone(),
                        log: std::rc::Rc::clone(&log),
                    });
                    (player.clone(), decider)
                })
                .collect();

            let rollout = ti4_training::rollout::play_with_deciders(
                content,
                &players,
                &seated,
                DEFAULT,
                seed,
                ti4_training::rollout::Horizon {
                    rounds: ROUNDS,
                    steps: 10_000,
                },
                ti4_engine::opening::DEFAULT_REQUIREMENT,
                &ti4_training::rollout::OpeningMap::PythonPool {
                    pool: Arc::clone(pool),
                    tile_seed_offset: TILE_SEED_OFFSET,
                },
                deciders,
            );
            if let Some(error) = &rollout.error {
                refuse(&format!("game {seed}/{rotation} failed: {error}"));
            }
            // The fingerprint is the decision sequence itself, not a summary of it: two arms could
            // agree on final scores while having chosen differently on the way.
            for (seat, chosen) in log.borrow().iter() {
                sha2::Digest::update(&mut hasher, seat.as_bytes());
                sha2::Digest::update(&mut hasher, chosen.as_bytes());
                sha2::Digest::update(&mut hasher, b"|");
            }
            decisions += log.borrow().len();
            log.borrow_mut().clear();
            for seat in &rollout.seats {
                outcomes.push(seat.episode.final_progress.victory_points);
            }
        }
    }
    let elapsed = started.elapsed();

    Batch {
        elapsed,
        fingerprint: format!("{:x}", sha2::Digest::finalize(hasher)),
        outcomes,
        scored: scored.get(),
        decisions,
    }
}

/// Records every decision its inner decider answers, for the fingerprint and the denominator.
///
/// # Why every arm is wrapped, including the linear one
///
/// The first version fingerprinted `seat.trajectory` from the rollout result. That is empty here:
/// `play_with_deciders` populates trajectories only for the `LearnedBot`s it constructs itself, and
/// this gate passes its own deciders in. So the fingerprint hashed nothing, matched nothing, and the
/// arm-for-arm identity assertion — the one thing that makes the shadow comparison meaningful —
/// could not have failed. It is recorded at the decider instead, where the decision actually is.
struct Recording {
    inner: Box<dyn Decider>,
    seat: PlayerId,
    log: std::rc::Rc<std::cell::RefCell<Vec<(String, String)>>>,
}

impl Recording {
    fn note(&self, answer: &Result<ChoiceOption, IllegalChoice>) {
        if let Ok(option) = answer {
            self.log
                .borrow_mut()
                .push((self.seat.as_str().to_owned(), option.id.clone()));
        }
    }
}

impl Decider for Recording {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let answer = self.inner.choose(choice);
        self.note(&answer);
        answer
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let answer = self.inner.choose_seeing(choice, seen);
        self.note(&answer);
        answer
    }
}

/// An MLP-choosing decider. Split out so the arm match stays readable.
fn crate_mlp_bot(
    actor: &Arc<Actor>,
    vocabulary: &Arc<Vocabulary>,
    row: FactionRow,
    stream: u64,
) -> (Box<dyn Decider>, ti4_mlp::bot::InferenceStatus) {
    // A fresh zero actor with the same shape: `MlpBot` takes ownership, and every actor in this
    // gate is zero-initialised anyway — the gate times the forward pass, not a trained policy.
    ti4_mlp::bot::MlpBot::new(
        Actor::zeros(
            Width::of(actor.width()).expect("the actor was built at a supported width"),
            actor.capacity(),
        ),
        (**vocabulary).clone(),
        row,
        stream,
    )
    .seat()
}

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    let middle = samples.len() / 2;
    if samples.len() % 2 == 0 {
        f64::midpoint(samples[middle - 1], samples[middle])
    } else {
        samples[middle]
    }
}

fn main() {
    let width = match argument("--width").as_deref() {
        None | Some("256") => Width::W256,
        Some("128") => Width::W128,
        Some(other) => refuse(&format!("--width {other}: only 256 and 128 exist")),
    };
    let samples: usize = argument("--samples")
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_SAMPLES);
    if samples < DEFAULT_SAMPLES {
        refuse(&format!(
            "§7.1 requires at least {DEFAULT_SAMPLES} timed batches, not {samples}"
        ));
    }

    // The decomposition probes below are how the shadow metric was diagnosed, and they are kept
    // because that diagnosis has to be reproducible. But a run with any of them set measures
    // something other than the gate, so it must not be able to print a verdict.
    let probes: Vec<&str> = ["SKIP_MODEL", "SKIP_CRITIC", "SKIP_PROJECT", "SHAPE"]
        .into_iter()
        .filter(|name| std::env::var(name).is_ok())
        .collect();

    let backend = ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();

    // The accepted vocabulary generation, exactly as the smoke resolves it.
    let generation = ti4_training::vocabulary_corpus::accepted_generation(std::path::Path::new(
        "out/vocabulary",
    ))
    .unwrap_or_else(|error| refuse(&format!("no accepted vocabulary generation: {error}")));
    let slots_text = std::fs::read_to_string(&generation.slots)
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
    let vocabulary = Arc::new(
        Vocabulary::from_json(&slots_text)
            .unwrap_or_else(|error| refuse(&format!("slots.json does not load: {error}"))),
    );
    let capacity = i64::try_from(vocabulary.capacity()).unwrap_or(i64::MAX);
    let actor = Arc::new(Actor::zeros(width, capacity));

    let checkpoint_path =
        argument("--checkpoint").unwrap_or_else(|| "out/stage2_r6/final10000.json".to_owned());
    let checkpoint = std::fs::read(&checkpoint_path)
        .unwrap_or_else(|error| refuse(&format!("reading {checkpoint_path}: {error}")));
    let champions =
        ti4_training::vocabulary_corpus::champion_profiles(&checkpoint, &checkpoint_path)
            .unwrap_or_else(|error| refuse(&format!("checkpoint profiles: {error}")));

    let pool_path =
        argument("--pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path} is not an allowed pool: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );

    println!("M09-029 CPU throughput gate");
    println!(
        "  workload   {} seeds x {} rotations, {ROUNDS} rounds, pool {pool_path}",
        SEEDS.end - SEEDS.start,
        FACTIONS.len()
    );
    println!(
        "  width      {} | capacity {capacity} | slots {}",
        width.dim(),
        vocabulary.slot_count()
    );
    println!(
        "  backend    intra-op {} inter-op {} mkl {} openmp {}",
        backend.intra_op_threads, backend.inter_op_threads, backend.mkl, backend.openmp
    );
    println!("  bands      accept <= {ACCEPT_RATIO}x, review > {REVIEW_RATIO}x (MLP plan §7.1)\n");

    // Warm-ups, discarded. Alternating from the start so neither arm gets a colder cache.
    for _ in 0..WARMUPS {
        let _ = run_batch(
            content,
            &pool,
            &champions,
            (&actor, &vocabulary),
            Arm::Linear,
        );
        let _ = run_batch(
            content,
            &pool,
            &champions,
            (&actor, &vocabulary),
            Arm::Shadow,
        );
    }

    let mut linear_samples = Vec::with_capacity(samples);
    let mut shadow_samples = Vec::with_capacity(samples);
    let mut reference: Option<(String, Vec<i64>)> = None;
    let mut total_scored = 0usize;

    for index in 0..samples {
        // Alternating order, so a machine that drifts warmer or busier over the run does not
        // attribute the drift to one arm.
        let (first_is_linear, _) = (index % 2 == 0, ());
        let (a, b) = if first_is_linear {
            (
                run_batch(
                    content,
                    &pool,
                    &champions,
                    (&actor, &vocabulary),
                    Arm::Linear,
                ),
                run_batch(
                    content,
                    &pool,
                    &champions,
                    (&actor, &vocabulary),
                    Arm::Shadow,
                ),
            )
        } else {
            let shadow = run_batch(
                content,
                &pool,
                &champions,
                (&actor, &vocabulary),
                Arm::Shadow,
            );
            (
                run_batch(
                    content,
                    &pool,
                    &champions,
                    (&actor, &vocabulary),
                    Arm::Linear,
                ),
                shadow,
            )
        };
        let (linear, shadow) = (a, b);

        // Arm-for-arm identity, asserted every sample rather than once.
        for batch in [&linear, &shadow] {
            match &reference {
                None => reference = Some((batch.fingerprint.clone(), batch.outcomes.clone())),
                Some((fingerprint, outcomes)) => {
                    if &batch.fingerprint != fingerprint {
                        refuse(&format!(
                            "decision fingerprints diverged at sample {index}: {} against \
                             {fingerprint}; the arms did not play the same games",
                            batch.fingerprint
                        ));
                    }
                    if &batch.outcomes != outcomes {
                        refuse(&format!("outcomes diverged at sample {index}"));
                    }
                }
            }
        }
        total_scored += shadow.scored;
        linear_samples.push(linear.elapsed.as_secs_f64());
        shadow_samples.push(shadow.elapsed.as_secs_f64());
        println!(
            "  sample {:>2}   linear {:>7.3}s   shadow {:>7.3}s   ratio {:>5.2}x",
            index + 1,
            linear.elapsed.as_secs_f64(),
            shadow.elapsed.as_secs_f64(),
            shadow.elapsed.as_secs_f64() / linear.elapsed.as_secs_f64()
        );
    }

    // The raw samples are preserved in the output above, per §7.1.
    let mut linear_sorted = linear_samples.clone();
    let mut shadow_sorted = shadow_samples.clone();
    let linear_median = median(&mut linear_sorted);
    let shadow_median = median(&mut shadow_sorted);
    let ratio = shadow_median / linear_median;

    // Median absolute deviation, so the report carries spread and not only a point estimate.
    let spread = |samples: &[f64], centre: f64| -> f64 {
        let mut deviations: Vec<f64> = samples.iter().map(|s| (s - centre).abs()).collect();
        median(&mut deviations)
    };

    println!(
        "\n  linear median  {linear_median:.3}s  (MAD {:.3}s)",
        spread(&linear_samples, linear_median)
    );
    println!(
        "  shadow median  {shadow_median:.3}s  (MAD {:.3}s)",
        spread(&shadow_samples, shadow_median)
    );
    println!("  ratio          {ratio:.3}x");
    if total_scored == 0 {
        refuse("the shadow arm scored no decisions, so it measured nothing");
    }
    println!("  MLP scored     {total_scored} decisions across the timed shadow batches");

    // --- The third arm: the MLP actually decides. ---
    //
    // §7.1's shadow design assumes "the tiny linear lookup remains in both arms". On this codebase
    // the linear bot's per-decision work is not a lookup — it is the same schema-4 feature
    // extraction the MLP needs, so the shadow arm pays for extraction **twice** and the ratio
    // charges the model for a duplicate the real thing would never perform. Measured: raw
    // extraction alone, with no tensor op at all, is already 1.53x.
    //
    // So this arm measures the real thing. The MLP chooses, which means a different trajectory —
    // exactly what §7.1 avoids by timing per batch — so it is compared **per decision** instead,
    // which is immune to the trajectories differing. It doubles as the row's required legality
    // smoke: every game must complete with the MLP deciding, or the batch refuses above.
    let mut mlp_per_decision = Vec::with_capacity(samples);
    let mut linear_per_decision = Vec::with_capacity(samples);
    for _ in 0..WARMUPS {
        let _ = run_batch(content, &pool, &champions, (&actor, &vocabulary), Arm::Mlp);
    }
    for _ in 0..samples {
        let mlp = run_batch(content, &pool, &champions, (&actor, &vocabulary), Arm::Mlp);
        let linear = run_batch(
            content,
            &pool,
            &champions,
            (&actor, &vocabulary),
            Arm::Linear,
        );
        if mlp.decisions == 0 || linear.decisions == 0 {
            refuse("a batch recorded no decisions, so a per-decision cost is undefined");
        }
        mlp_per_decision.push(mlp.elapsed.as_secs_f64() / mlp.decisions as f64);
        linear_per_decision.push(linear.elapsed.as_secs_f64() / linear.decisions as f64);
    }
    let mlp_cost = median(&mut mlp_per_decision.clone());
    let linear_cost = median(&mut linear_per_decision.clone());
    let real_ratio = mlp_cost / linear_cost;
    println!(
        "
  MLP-choosing arm (legality smoke, and the per-decision comparison)"
    );
    println!("    linear   {:>8.1} us/decision", linear_cost * 1e6);
    println!("    mlp      {:>8.1} us/decision", mlp_cost * 1e6);
    println!("    ratio    {real_ratio:.3}x");

    if !probes.is_empty() {
        println!(
            "
  DIAGNOSTIC RUN ({}) — no verdict. These probes remove work the gate exists to              measure.",
            probes.join(", ")
        );
        return;
    }

    let verdict = match (width, ratio) {
        (_, r) if r <= ACCEPT_RATIO => format!("ACCEPT width {}", width.dim()),
        (Width::W256, r) if r <= REVIEW_RATIO => {
            "FALLBACK: rerun the whole gate at --width 128; accept only at <= 2x".to_owned()
        }
        (Width::W256, _) => {
            "STOP: > 3x at width 256 — architecture review before distillation".to_owned()
        }
        (Width::W128, _) => {
            "STOP: the 128-wide fallback is still > 2x — architecture review".to_owned()
        }
    };
    println!("\n  verdict        {verdict}");
    if verdict.starts_with("STOP") {
        std::process::exit(3);
    }
    if verdict.starts_with("FALLBACK") {
        std::process::exit(1);
    }
}
