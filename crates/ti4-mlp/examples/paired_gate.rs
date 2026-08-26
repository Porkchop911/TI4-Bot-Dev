//! M09-029, corrected: the paired scorer gate (F-M09-029-R3).
//!
//! # Why the original harness could not pass
//!
//! §7.1 times a shadow arm in which the linear champion decides while the MLP scores the same legal
//! set. Its premise is that "the tiny linear lookup remains in both arms". On this codebase that is
//! false: the linear bot's per-decision work *is* the schema-4 feature extraction the MLP also
//! needs, so the shadow arm performs that extraction twice and charges the difference to the model.
//! Measured, raw extraction alone with no tensor operation executed is 1.53× — so a model costing
//! nothing would still fail a 2.0× band.
//!
//! My first response was to report a different number (an MLP-choosing arm, 1.681×) and recommend
//! accepting on that basis. F-M09-029-R3 rejected it, correctly: it compares different policy
//! trajectories, and it was chosen after watching the specified metric fail.
//!
//! # What this measures instead
//!
//! The reviewer's own suggested remedy: *"replay one fixed captured decision stream through both
//! scorers or otherwise share extraction while keeping inputs identical."*
//!
//! One rollout. At every decision, on **the identical legal set**, three things are timed
//! separately:
//!
//! | quantity | how |
//! |---|---|
//! | `raw` | `explicit_choice_features` alone — the extraction both scorers need |
//! | `linear` | `consider` (extraction + linear scoring) minus `raw` |
//! | `mlp` | projection + sparse conversion + forward, minus `raw` |
//!
//! Extraction is therefore counted once and attributed to neither scorer, which is the whole
//! correction. Engine time is whatever the batch took that none of the above accounts for.
//!
//! The rollout ratio §7.1's bands are stated in is then reconstructed from measured parts:
//!
//! ```text
//! ratio = (engine + raw + mlp) / (engine + raw + linear)
//! ```
//!
//! Nothing here is estimated and no arm plays a different game: both scorers see the same decisions
//! in the same order, because only one policy is choosing.

#![allow(
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::arc_with_non_send_sync,
    reason = "a measurement driver: counts are small and the workload reads in order"
)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_mlp::{Actor, FactionRow, SparseOption, Width};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::vocabulary::Vocabulary;

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;
const SEEDS: std::ops::Range<u64> = 900_000_000..900_000_016;
const ROUNDS: u32 = 4;
const WARMUPS: usize = 5;
const SAMPLES: usize = 20;
/// §7.1's accept band, unchanged.
const ACCEPT_RATIO: f64 = 2.0;
/// §7.1's review band, unchanged.
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

/// What one batch cost, split by who is responsible for it.
#[derive(Debug, Clone, Copy, Default)]
struct Split {
    total: Duration,
    raw: Duration,
    linear: Duration,
    mlp: Duration,
    decisions: usize,
}

impl Split {
    /// Engine and everything else: whatever the batch took that no scorer accounts for.
    fn engine(&self) -> f64 {
        (self.total.as_secs_f64()
            - self.raw.as_secs_f64()
            - self.linear.as_secs_f64()
            - self.mlp.as_secs_f64())
        .max(0.0)
    }

    /// §7.1's rollout ratio, reconstructed from measured parts with extraction counted once.
    fn ratio(&self) -> f64 {
        let shared = self.engine() + self.raw.as_secs_f64();
        (shared + self.mlp.as_secs_f64()) / (shared + self.linear.as_secs_f64())
    }
}

/// Times both scorers on the identical legal set, and lets the linear one decide.
struct Paired {
    inner: ti4_policy::inference::LearnedBot,
    actor: Arc<Actor>,
    vocabulary: Arc<Vocabulary>,
    row: FactionRow,
    split: std::rc::Rc<std::cell::RefCell<Split>>,
}

impl Decider for Paired {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let held = seen.held_secret_progress();
        let observed = seen.observed();

        // 1. The extraction both scorers need, timed alone and charged to neither.
        let started = Instant::now();
        let raw =
            ti4_policy::features::explicit_choice_features(observed, choice, &choice.player, &held);
        let raw_cost = started.elapsed();
        std::hint::black_box(&raw);

        // 2. The linear scorer, on that same decision. `consider` re-extracts, so its own cost is
        //    what remains after subtracting an identical extraction.
        let started = Instant::now();
        let scored = self.inner.consider(observed, choice, &held);
        let consider_cost = started.elapsed();
        std::hint::black_box(&scored);

        // 3. The MLP on the identical legal set: projection, sparse conversion, forward. Also
        //    re-extracts, so the same subtraction applies and the two are treated alike.
        let started = Instant::now();
        let projected =
            ti4_policy::projection::mlp_choice_features(observed, choice, &choice.player, &held);
        let options: Vec<SparseOption> = projected
            .iter()
            .map(|vector| {
                let mut columns = Vec::with_capacity(vector.len());
                let mut values = Vec::with_capacity(vector.len());
                for (key, value) in vector {
                    columns.push(i64::try_from(self.vocabulary.column_of_key(*key)).unwrap_or(0));
                    #[expect(clippy::cast_possible_truncation, reason = "features are f32-scale")]
                    values.push(*value as f32);
                }
                SparseOption { columns, values }
            })
            .collect();
        if !options.is_empty() {
            let head = Actor::resolve_head(ti4_policy::learned::decision_head(choice));
            let _ = self.actor.probabilities(&options, head, self.row, 1.0);
        }
        let mlp_cost = started.elapsed();

        {
            let mut split = self.split.borrow_mut();
            split.raw += raw_cost;
            // Saturating, because a subtraction of two independently noisy timings can go negative
            // on a fast decision; letting it wrap would silently produce an enormous cost.
            split.linear += consider_cost.saturating_sub(raw_cost);
            split.mlp += mlp_cost.saturating_sub(raw_cost);
            split.decisions += 1;
        }

        self.inner.choose_seeing(choice, seen)
    }
}

fn run_batch(
    content: &'static ContentStore,
    pool: &Arc<ti4_sim::MapPool>,
    champions: &BTreeMap<String, ti4_policy::learned::Profile>,
    actor: &Arc<Actor>,
    vocabulary: &Arc<Vocabulary>,
) -> Split {
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let split = std::rc::Rc::new(std::cell::RefCell::new(Split::default()));

    let started = Instant::now();
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
                    let stream = seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(u64::try_from(index).unwrap_or(0));
                    let decider: Box<dyn Decider> = Box::new(Paired {
                        inner: ti4_policy::inference::LearnedBot::from_shared(
                            Arc::new(profile),
                            stream,
                        ),
                        actor: Arc::clone(actor),
                        vocabulary: Arc::clone(vocabulary),
                        row: FactionRow::of(seated[player].as_str()).expect("roster"),
                        split: std::rc::Rc::clone(&split),
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
        }
    }
    let mut out = *split.borrow();
    out.total = started.elapsed();
    out
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        f64::midpoint(values[middle - 1], values[middle])
    } else {
        values[middle]
    }
}

fn main() {
    let width = match argument("--width").as_deref() {
        None | Some("256") => Width::W256,
        Some("128") => Width::W128,
        Some(other) => refuse(&format!("--width {other}: only 256 and 128 exist")),
    };
    let backend = ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();

    let generation = ti4_training::vocabulary_corpus::accepted_generation(std::path::Path::new(
        "out/vocabulary",
    ))
    .unwrap_or_else(|error| refuse(&format!("no accepted vocabulary: {error}")));
    let slots_text = std::fs::read_to_string(&generation.slots)
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
    let vocabulary = Arc::new(
        Vocabulary::from_json(&slots_text)
            .unwrap_or_else(|error| refuse(&format!("slots.json: {error}"))),
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
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );

    println!("M09-029 corrected: paired scorer gate (F-M09-029-R3)");
    println!(
        "  workload   {} seeds x {} rotations, {ROUNDS} rounds",
        SEEDS.end - SEEDS.start,
        FACTIONS.len()
    );
    println!("  width      {} | capacity {capacity}", width.dim());
    println!(
        "  backend    intra-op {} inter-op {}",
        backend.intra_op_threads, backend.inter_op_threads
    );
    println!(
        "  bands      accept <= {ACCEPT_RATIO}x, review > {REVIEW_RATIO}x (§7.1, unchanged)\n"
    );

    for _ in 0..WARMUPS {
        let _ = run_batch(content, &pool, &champions, &actor, &vocabulary);
    }

    let mut ratios = Vec::with_capacity(SAMPLES);
    let mut last = Split::default();
    for index in 0..SAMPLES {
        let split = run_batch(content, &pool, &champions, &actor, &vocabulary);
        if split.decisions == 0 {
            refuse("no decision was timed, so the ratio is undefined");
        }
        ratios.push(split.ratio());
        last = split;
        println!(
            "  sample {:>2}   raw {:>6.3}s  linear {:>6.3}s  mlp {:>6.3}s  engine {:>6.3}s  ratio {:>5.3}x",
            index + 1,
            split.raw.as_secs_f64(),
            split.linear.as_secs_f64(),
            split.mlp.as_secs_f64(),
            split.engine(),
            split.ratio()
        );
    }

    let ratio = median(&mut ratios.clone());
    let per = |d: Duration| d.as_secs_f64() * 1e6 / last.decisions as f64;
    println!("\n  decisions per batch  {}", last.decisions);
    println!("  extraction (shared)  {:>7.1} us/decision", per(last.raw));
    println!(
        "  linear scoring       {:>7.1} us/decision",
        per(last.linear)
    );
    println!("  mlp scoring          {:>7.1} us/decision", per(last.mlp));
    println!("  median ratio         {ratio:.3}x");

    // Non-vacuity: if the MLP were never scored the ratio would be 1.0 and look like a pass.
    if last.mlp.as_secs_f64() <= 0.0 {
        refuse("the MLP scored nothing, so the ratio measures nothing");
    }

    let verdict = match (width, ratio) {
        (_, r) if r <= ACCEPT_RATIO => format!("ACCEPT width {}", width.dim()),
        (Width::W256, r) if r <= REVIEW_RATIO => {
            "FALLBACK: rerun at --width 128; accept only at <= 2x".to_owned()
        }
        (Width::W256, _) => "STOP: > 3x at width 256 — architecture review".to_owned(),
        (Width::W128, _) => {
            "STOP: the 128-wide fallback is still > 2x — architecture review".to_owned()
        }
    };
    println!("\n  verdict              {verdict}");
    if verdict.starts_with("STOP") {
        std::process::exit(3);
    }
    if verdict.starts_with("FALLBACK") {
        std::process::exit(1);
    }
}
