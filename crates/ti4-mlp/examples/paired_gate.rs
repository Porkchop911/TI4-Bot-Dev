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
//! A probe rollout and an uncontaminated linear rollout use the same deterministic inputs and must
//! produce identical outcomes. At every non-forced probe decision, two complete scorer paths are
//! timed:
//!
//! | quantity | how |
//! |---|---|
//! | `linear` | explicit extraction plus linear scoring |
//! | `mlp` | MLP projection, sparse conversion and forward |
//!
//! Probe overhead never enters the uncontaminated rollout total. The rollout ratio §7.1's bands
//! are stated in is reconstructed by replacing its complete linear scorer cost with the complete
//! MLP scorer cost:
//!
//! ```text
//! ratio = (linear rollout total - linear scorer + MLP scorer) / linear rollout total
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
    linear: Duration,
    mlp: Duration,
    decisions: usize,
    evaluations: usize,
}

impl Split {
    /// §7.1's rollout ratio, replacing the complete linear scorer with the complete MLP scorer.
    fn ratio(&self, linear_total: Duration) -> Result<f64, String> {
        let total = linear_total.as_secs_f64();
        let linear = self.linear.as_secs_f64();
        if total <= 0.0 || linear >= total {
            return Err(format!(
                "linear scorer cost {linear:.3}s is not smaller than rollout total {total:.3}s"
            ));
        }
        Ok((total - linear + self.mlp.as_secs_f64()) / total)
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
        if choice.options.len() < 2 {
            return self.inner.choose_seeing(choice, seen);
        }
        let held = seen.held_secret_progress();
        let observed = seen.observed();

        // Complete production scorer costs: each owns its actual extraction/projection.
        let started = Instant::now();
        let scored = self.inner.consider(observed, choice, &held);
        let linear_cost = started.elapsed();
        if scored.0.len() != choice.options.len() || scored.1.len() != choice.options.len() {
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "linear probe did not score every legal option".to_owned(),
            });
        }
        std::hint::black_box(&scored);

        let started = Instant::now();
        let projected =
            ti4_policy::projection::mlp_choice_features(observed, choice, &choice.player, &held);
        let options: Vec<SparseOption> = projected
            .iter()
            .map(|vector| {
                let mut columns = Vec::with_capacity(vector.len());
                let mut values = Vec::with_capacity(vector.len());
                for (key, value) in vector {
                    columns.push(i64::try_from(self.vocabulary.column_of_key(*key)).map_err(
                        |_| IllegalChoice::DeciderFailed {
                            player: choice.player.clone(),
                            prompt: choice.prompt.clone(),
                            reason: "vocabulary column does not fit i64".to_owned(),
                        },
                    )?);
                    #[expect(clippy::cast_possible_truncation, reason = "features are f32-scale")]
                    values.push(*value as f32);
                }
                Ok(SparseOption { columns, values })
            })
            .collect::<Result<Vec<_>, IllegalChoice>>()?;
        let head = Actor::resolve_head(ti4_policy::learned::decision_head(choice));
        let probabilities = self
            .actor
            .probabilities(&options, head, self.row, 1.0)
            .map_err(|error| IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: format!("MLP probe failed on {head}: {error}"),
            })?;
        if probabilities.len() != choice.options.len() {
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "MLP probe did not score every legal option".to_owned(),
            });
        }
        std::hint::black_box(&probabilities);
        let mlp_cost = started.elapsed();

        {
            let mut split = self.split.borrow_mut();
            split.linear += linear_cost;
            split.mlp += mlp_cost;
            split.decisions += 1;
            split.evaluations += 1;
        }

        self.inner.choose_seeing(choice, seen)
    }
}

fn run_probe_batch(
    content: &'static ContentStore,
    pool: &Arc<ti4_sim::MapPool>,
    champions: &BTreeMap<String, ti4_policy::learned::Profile>,
    actor: &Arc<Actor>,
    vocabulary: &Arc<Vocabulary>,
) -> (Split, Vec<ti4_training::rollout::Rollout>) {
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let split = std::rc::Rc::new(std::cell::RefCell::new(Split::default()));

    let mut outcomes = Vec::new();
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
            outcomes.push(rollout);
        }
    }
    let out = *split.borrow();
    (out, outcomes)
}

/// The uncontaminated denominator: the ordinary linear champion rollout with no probes or timing
/// calls inside its decisions.
fn run_linear_batch(
    content: &'static ContentStore,
    pool: &Arc<ti4_sim::MapPool>,
    champions: &BTreeMap<String, ti4_policy::learned::Profile>,
) -> (Duration, Vec<ti4_training::rollout::Rollout>) {
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let mut outcomes = Vec::new();
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
                    let decider: Box<dyn Decider> = Box::new(
                        ti4_policy::inference::LearnedBot::from_shared(Arc::new(profile), stream),
                    );
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
                refuse(&format!("linear game {seed}/{rotation} failed: {error}"));
            }
            outcomes.push(rollout);
        }
    }
    (started.elapsed(), outcomes)
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
        let (probe, probed_outcomes) =
            run_probe_batch(content, &pool, &champions, &actor, &vocabulary);
        let (_, linear_outcomes) = run_linear_batch(content, &pool, &champions);
        if probe.decisions == 0
            || probe.evaluations != probe.decisions
            || probed_outcomes != linear_outcomes
        {
            refuse("a warm-up did not reproduce identical outcomes and exact model evaluations");
        }
    }

    let mut ratios = Vec::with_capacity(SAMPLES);
    let mut last = Split::default();
    let mut last_linear_total = Duration::ZERO;
    for index in 0..SAMPLES {
        // Alternate which full batch runs first to prevent monotonic machine drift favouring one.
        let (split, probed_outcomes, linear_total, linear_outcomes) = if index % 2 == 0 {
            let (split, probed) = run_probe_batch(content, &pool, &champions, &actor, &vocabulary);
            let (total, linear) = run_linear_batch(content, &pool, &champions);
            (split, probed, total, linear)
        } else {
            let (total, linear) = run_linear_batch(content, &pool, &champions);
            let (split, probed) = run_probe_batch(content, &pool, &champions, &actor, &vocabulary);
            (split, probed, total, linear)
        };
        if split.decisions == 0 || split.evaluations != split.decisions {
            refuse("the probe did not complete exactly one MLP evaluation per decision");
        }
        if probed_outcomes != linear_outcomes {
            refuse("probe and uncontaminated linear runs produced different outcomes");
        }
        let ratio = split
            .ratio(linear_total)
            .unwrap_or_else(|error| refuse(&error));
        ratios.push(ratio);
        last = split;
        last_linear_total = linear_total;
        let shared = linear_total.as_secs_f64() - split.linear.as_secs_f64();
        println!(
            "  sample {:>2}   total {:>6.3}s  linear {:>6.3}s  mlp {:>6.3}s  shared {:>6.3}s  ratio {:>5.3}x",
            index + 1,
            linear_total.as_secs_f64(),
            split.linear.as_secs_f64(),
            split.mlp.as_secs_f64(),
            shared,
            ratio
        );
    }

    let ratio = median(&mut ratios.clone());
    let per = |d: Duration| d.as_secs_f64() * 1e6 / last.decisions as f64;
    println!("\n  non-forced decisions {}", last.decisions);
    println!(
        "  linear rollout total {:>7.3}s",
        last_linear_total.as_secs_f64()
    );
    println!(
        "  complete linear path {:>7.1} us/decision",
        per(last.linear)
    );
    println!("  complete MLP path    {:>7.1} us/decision", per(last.mlp));
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
