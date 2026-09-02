//! How well does a policy predict the decisions of demonstrations that worked?
//!
//! # What this is for
//!
//! An architecture question should not be answered by an eight-hour PPO run whose result is
//! confounded by exploration, reward shaping and seed luck. The corpora are frozen and deterministic,
//! so the same question can be asked as supervised prediction: given a position from a line that
//! cleared, what probability and rank does the policy assign the action that line took?
//!
//! Two corpora, and the difference between them is the point:
//!
//! - the **positive** corpus is ordinary successful play, from positions the champion already
//!   clears. A policy that generated it should predict it well, and doing so proves little.
//! - the **rescued** corpus is clearing lines from positions the champion greedily **fails**. These
//!   are the decisions it demonstrably does not make. If a policy ranks those actions poorly, the
//!   hard decisions are not well represented; if it ranks them near the top, representation is not
//!   the bottleneck and something else — credit assignment, exploration, the reward — is stopping it
//!   choosing them.
//!
//! That second number is the one worth having before building a new architecture.
//!
//! # The split is by starting position, never by trajectory
//!
//! A position contributes up to ~17 distinct clearing lines. Splitting by trajectory would put lines
//! from the same map on both sides and a model that memorised the position would score as one that
//! generalised. Seeds are partitioned instead, and the partition is by hash of the seed so it does
//! not move when a corpus is regenerated with more of them.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example demo_benchmark -- \
//!   --bundle out/checkpoints/sweep-A-250/checkpoint-14476 \
//!   --corpus out/corpus/positive --rescued out/corpus/rescued --per-corpus 400
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::{ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_mlp::positive_corpus::{Trajectory, read_all};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 0;

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn number<T: std::str::FromStr>(flag: &str, fallback: T) -> T {
    argument(flag).map_or(fallback, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{flag} expects a number")))
    })
}

/// Whether a seed belongs to the validation half.
///
/// A hash rather than a range, so regenerating a corpus over more seeds does not reshuffle which
/// positions are held out and make two runs incomparable.
fn is_validation(seed: u64) -> bool {
    let mut hash = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    hash ^= hash >> 29;
    hash = hash.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    hash ^= hash >> 32;
    hash % 5 == 0
}

/// Forces a recorded line so the position it visited can be scored.
struct Replaying {
    inner: Box<dyn Decider>,
    script: Vec<String>,
    at: usize,
    forced: Rc<RefCell<Vec<usize>>>,
    broken: Rc<RefCell<bool>>,
}

impl Replaying {
    fn answer(
        &mut self,
        choice: &Choice,
        delegate: impl FnOnce(&mut Box<dyn Decider>) -> Result<ChoiceOption, IllegalChoice>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if choice.options.len() < 2 {
            return delegate(&mut self.inner);
        }
        let _ = delegate(&mut self.inner)?;
        let Some(wanted) = self.script.get(self.at).cloned() else {
            *self.broken.borrow_mut() = true;
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "replay script exhausted".to_owned(),
            });
        };
        self.at += 1;
        let Some(index) = choice.options.iter().position(|option| option.id == wanted) else {
            *self.broken.borrow_mut() = true;
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: format!("recorded option {wanted:?} is not on offer"),
            });
        };
        self.forced.borrow_mut().push(index);
        Ok(choice.options[index].clone())
    }
}

impl Decider for Replaying {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.answer(choice, |inner| inner.choose(choice))
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.answer(choice, |inner| inner.choose_seeing(choice, seen))
    }
}

/// One scored decision.
struct Scored {
    head: String,
    options: usize,
    /// Probability the policy gave the demonstrated action.
    probability: f64,
    /// Its rank, 1 being the policy's own top choice.
    rank: usize,
}

/// Running totals for one head.
#[derive(Default)]
struct Tally {
    decisions: usize,
    top1: usize,
    log_prob: f64,
    rank: usize,
    options: usize,
}

impl Tally {
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    fn report(&self, name: &str) {
        if self.decisions == 0 {
            return;
        }
        let n = self.decisions as f64;
        println!(
            "    {name:<12} {:>7}   {:>6.1}   {:>7.3}   {:>6.2}   {:>6.1}%",
            self.decisions,
            self.options as f64 / n,
            -self.log_prob / n,
            self.rank as f64 / n,
            self.top1 as f64 / n * 100.0
        );
    }
}

struct Table<'a> {
    content: &'static ContentStore,
    factions: &'a [FactionId],
    pool: &'a Arc<ti4_sim::MapPool>,
    vocabulary: &'a ti4_policy::vocabulary::Vocabulary,
}

/// Replay one trajectory and score every decision it took.
/// How the five opponents were sampled when the line was recorded.
///
/// The two corpora differ and neither records it, so the caller states it. `build_positive_corpus`
/// puts every seat at the corpus temperature; `rescue_search` holds the opponents greedy and heats
/// only the searching seat. Replaying with the wrong convention gives different opponents, a
/// different game, and a line that does not exist -- it silently discarded 263 of 300 ordinary
/// trajectories before this argument existed, leaving a biased remnant.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Opponents {
    Greedy,
    SameTemperature,
}

fn score(
    table: &Table<'_>,
    replay_actor: &Rc<ti4_mlp::Actor>,
    scoring_actor: &ti4_mlp::Actor,
    trajectory: &Trajectory,
    opponents: Opponents,
) -> Result<Vec<Scored>, String> {
    let records: Rc<RefCell<Option<Rc<RefCell<Vec<ti4_mlp::bot::PpoRecord>>>>>> =
        Rc::new(RefCell::new(None));
    let forced = Rc::new(RefCell::new(Vec::new()));
    let broken = Rc::new(RefCell::new(false));
    let handle = Rc::clone(&records);
    let taken = Rc::clone(&forced);
    let fault = Rc::clone(&broken);
    let script = trajectory.decisions.clone();
    let want = trajectory.faction.clone();
    #[expect(clippy::cast_precision_loss, reason = "thousandths, well within f64")]
    let temperature = trajectory.temperature_milli as f64 / 1_000.0;

    let played = ti4_training::rollout::audit_game_with_deciders(
        table.content,
        table.factions,
        DEFAULT,
        trajectory.seed,
        trajectory.rotation,
        ti4_training::rollout::Horizon {
            rounds: 1,
            steps: 200_000,
        },
        &ti4_training::rollout::OpeningMap::PythonPool {
            pool: Arc::clone(table.pool),
            tile_seed_offset: TILE_SEED_OFFSET,
        },
        |seated, baselines| {
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (index, (player, faction)) in seated.iter().enumerate() {
                let row = ti4_mlp::FactionRow::of(faction.as_str())
                    .map_err(|error| format!("{player}: {error}"))?;
                let baseline = baselines
                    .get(player)
                    .copied()
                    .ok_or_else(|| format!("{player} has no baseline"))?;
                let mine = faction.as_str() == want;
                let stream = trajectory
                    .seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0))
                    .wrapping_add(if mine || opponents == Opponents::SameTemperature {
                        trajectory.temperature_milli
                    } else {
                        0
                    });
                // Replay always uses the generating policy, so the line exists. Scoring is a
                // separate actor, which is what lets one corpus benchmark several models.
                let bot = ti4_mlp::bot::MlpBot::sharing(
                    replay_actor,
                    table.vocabulary.clone(),
                    row,
                    stream,
                )
                .at_temperature(if mine || opponents == Opponents::SameTemperature {
                    temperature
                } else {
                    0.001
                })
                .from_setup(baseline);
                if mine {
                    let bot = bot.recording_ppo(ti4_mlp::bundle::CriticMode::BatchMean);
                    *handle.borrow_mut() = Some(bot.ppo_records());
                    let (decider, _status) = bot.seat();
                    deciders.insert(
                        player.clone(),
                        Box::new(Replaying {
                            inner: decider,
                            script: script.clone(),
                            at: 0,
                            forced: Rc::clone(&taken),
                            broken: Rc::clone(&fault),
                        }),
                    );
                } else {
                    let (decider, _status) = bot.seat();
                    deciders.insert(player.clone(), decider);
                }
            }
            Ok(deciders)
        },
    );
    if *broken.borrow() {
        return Err("the line did not replay".to_owned());
    }
    played?;

    let records = records
        .borrow_mut()
        .take()
        .ok_or_else(|| "nothing recorded".to_owned())?;
    let steps: Vec<ti4_mlp::ppo::Step> = records.borrow().iter().map(|r| r.step.clone()).collect();
    let forced = forced.borrow();
    if steps.len() != forced.len() {
        return Err(format!(
            "{} recorded steps against {} forced decisions",
            steps.len(),
            forced.len()
        ));
    }

    let heads = ti4_mlp::heads();
    let mut out = Vec::new();
    for (step, chosen) in steps.iter().zip(forced.iter()) {
        let head = heads
            .get(step.head)
            .ok_or_else(|| format!("head {} out of range", step.head))?;
        // Temperature 1.0: this asks what the policy believes, not what a sharpened reading of it
        // would pick. Rank is unaffected by temperature; probability is not.
        let probabilities = scoring_actor
            .probabilities(&step.options, head, step.row, 1.0)
            .map_err(|error| format!("scoring: {error}"))?;
        let mine = probabilities.get(*chosen).copied().unwrap_or(0.0);
        let rank = 1 + probabilities.iter().filter(|p| **p > mine).count();
        out.push(Scored {
            head: (*head).to_owned(),
            options: step.options.len(),
            probability: mine,
            rank,
        });
    }
    Ok(out)
}

fn benchmark(
    label: &str,
    directory: &str,
    table: &Table<'_>,
    actor: &ti4_mlp::Actor,
    per_corpus: usize,
    opponents: Opponents,
) {
    let mut trajectories: Vec<Trajectory> = Vec::new();
    for faction in FACTIONS {
        let path = std::path::Path::new(directory).join(format!("{faction}.corpus"));
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let parsed = read_all(&text)
            .unwrap_or_else(|error| refuse(&format!("parsing {}: {error}", path.display())));
        for (_, rows) in parsed {
            trajectories.extend(rows);
        }
    }
    // Held-out positions only. Scoring the training half would measure memorisation.
    trajectories.retain(|t| is_validation(t.seed));
    trajectories.sort_by_key(|t| (t.seed, t.rotation, t.faction.clone()));
    let held = trajectories.len();
    if held > per_corpus {
        let stride = held / per_corpus;
        trajectories = trajectories
            .into_iter()
            .step_by(stride.max(1))
            .take(per_corpus)
            .collect();
    }
    if trajectories.is_empty() {
        println!("  {label}: nothing held out to score");
        return;
    }

    let workers = rayon::current_num_threads().max(1);
    let per_worker = trajectories.len().div_ceil(workers).max(1);
    let harvest: Vec<(Vec<Scored>, usize)> = trajectories
        .chunks(per_worker)
        .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(local, chunk)| {
            let replay = Rc::new(local.inference_copy());
            let mut rows = Vec::new();
            let mut bad = 0usize;
            for trajectory in chunk {
                match score(table, &replay, &local, &trajectory, opponents) {
                    Ok(mut scored) => rows.append(&mut scored),
                    Err(_) => bad += 1,
                }
            }
            (rows, bad)
        })
        .collect();

    let mut by_head: BTreeMap<String, Tally> = BTreeMap::new();
    let mut all = Tally::default();
    let mut failed = 0usize;
    for (rows, bad) in harvest {
        failed += bad;
        for row in rows {
            for tally in [
                by_head.entry(row.head.clone()).or_default(),
                // `all` is updated below; the array is only to avoid repeating the body.
            ] {
                tally.decisions += 1;
                tally.top1 += usize::from(row.rank == 1);
                tally.log_prob += row.probability.max(1e-12).ln();
                tally.rank += row.rank;
                tally.options += row.options;
            }
            all.decisions += 1;
            all.top1 += usize::from(row.rank == 1);
            all.log_prob += row.probability.max(1e-12).ln();
            all.rank += row.rank;
            all.options += row.options;
        }
    }

    println!();
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let lost = failed as f64 / trajectories.len().max(1) as f64 * 100.0;
    println!(
        "  {label}  ({} of {held} held-out trajectories, {failed} unreplayable = {lost:.1}%)",
        trajectories.len()
    );
    if lost > 5.0 {
        println!("    WARNING: the scored rows are a remnant, not a sample. Do not read them.");
    }
    println!();
    println!("    head         decisions   options        CE     rank    top-1");
    let mut rows: Vec<(&String, &Tally)> = by_head.iter().collect();
    rows.sort_by(|a, b| b.1.decisions.cmp(&a.1.decisions));
    for (head, tally) in rows {
        tally.report(head);
    }
    all.report("ALL");
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let positive = argument("--corpus").unwrap_or_else(|| "out/corpus/positive".to_owned());
    let rescued = argument("--rescued").unwrap_or_else(|| "out/corpus/rescued".to_owned());
    let per_corpus: usize = number("--per-corpus", 400);

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            ti4_sim::artifacts::read_and_verify_pool_role(
                std::path::Path::new("out/pools/full_np8_12_train.json"),
                &[ti4_sim::artifacts::ArtifactRole::Train],
            )
            .unwrap_or_else(|error| refuse(&format!("train pool: {error}"))),
        ))
        .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();
    let table = Table {
        content,
        factions: &factions,
        pool: &pool,
        vocabulary: &vocabulary,
    };

    println!("demonstration benchmark for {bundle_path}");
    println!("  held out by starting position (1 seed in 5), never by trajectory");
    println!("  CE and probability read at temperature 1.0; rank is temperature-free");

    benchmark(
        "ORDINARY SUCCESSES",
        &positive,
        &table,
        &actor,
        per_corpus,
        Opponents::SameTemperature,
    );
    benchmark(
        "RESCUED SUCCESSES",
        &rescued,
        &table,
        &actor,
        per_corpus,
        Opponents::Greedy,
    );
    println!();
    println!(
        "  The gap between the two is the measurement: ordinary lines come from positions this"
    );
    println!("  policy already clears, rescued lines from positions it greedily fails.");
}
