//! M10-034: one PPO update from MLP self-play, end to end.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example ppo_update -- [--updates 1] [--device cuda]
//! ```
//!
//! §6.3's unit of work: 16 game seeds × six rotations of self-play, the behaviour
//! log-probabilities, returns and values stored **before** optimisation, then four epochs of the
//! clipped surrogate over 4,096-decision minibatches with the advantage frozen throughout.
//!
//! # Rollouts stay on the CPU
//!
//! §7.1 admits no CUDA inference backend, so every action is selected by the deterministic CPU
//! path. `--device cuda` places the trained model on the device; self-play runs from a CPU
//! inference *copy*, and the training actor is never moved — moving a tensor that requires a
//! gradient replaces it with a non-leaf view, and the gradients then land on the leaves left
//! behind.
//!
//! Games are played in parallel across rayon workers, one owned actor copy per worker.
//!
//! `--rollout-backend infer` plays them on `ti4-infer` instead (`Avx2Fma` where the CPU has it):
//! after each update the actor is packed in memory into one `ti4_infer::Model`, which every worker
//! shares by `Arc`, and the value of each recorded decision comes from the value head of that
//! model (a shared critic only). The update itself stays on libtorch. `--rollout-backend
//! infer-int8` packs the actor with int8 weights and plays on `Avx2Vnni` (AVX-VNNI CPUs only;
//! section H of `web2/plans/2026-10-15-MLP_INFERENCE_PLAN.md`); the behaviour probabilities are
//! those of the int8 model, and the update recomputes them on libtorch as before.
//!
//! This exists to be measured as much as to run: every estimate of what M10-038's 30,000 updates
//! would cost has so far been an extrapolation, and one real update replaces all of them.

#![allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::arc_with_non_send_sync,
    reason = "a driver: the phases read in the order they run"
)]

use rayon::prelude::*;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Write as _;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::{ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_mlp::bundle::CriticMode;
use ti4_mlp::positive_corpus::{AdvantageDemo, Demo, Trajectory, read_all};
use ti4_mlp::ppo::{Batch, Settings, Step};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

/// The six factions a `--roster six` game seats, in rotation order: the historical six, or
/// `--factions a,b,c,d,e,f`.
/// Overpay, unused ready-planet resources and build value (per-mille resources) and final fleet value (per-mille) summed over this
/// update's seat-games; read and reset when the update is reported.
static PRODUCTION_TELEMETRY: [std::sync::atomic::AtomicI64; 4] = [
    std::sync::atomic::AtomicI64::new(0),
    std::sync::atomic::AtomicI64::new(0),
    std::sync::atomic::AtomicI64::new(0),
    std::sync::atomic::AtomicI64::new(0),
];

static FACTIONS: std::sync::LazyLock<[&'static str; 6]> = std::sync::LazyLock::new(|| {
    let Some(list) = argument("--factions") else {
        return ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
    };
    let parsed: Vec<&'static str> = list
        .split(',')
        .map(|name| &*Box::leak(name.trim().to_owned().into_boxed_str()))
        .collect();
    let mut distinct = parsed.clone();
    distinct.sort_unstable();
    distinct.dedup();
    if distinct.len() != 6
        || parsed
            .iter()
            .any(|name| ti4_mlp::FactionRow::of(name).is_err())
    {
        refuse("--factions expects six distinct roster factions, comma separated");
    }
    parsed
        .try_into()
        .unwrap_or_else(|_| refuse("--factions expects six factions"))
});
const TILE_SEED_OFFSET: u64 = 20_000_000;
/// `build_positive_corpus`'s map convention. Corpus replay must reproduce that generator, not PPO.
const CORPUS_TILE_SEED_OFFSET: u64 = 0;
const DEMO_TEMPERATURES_MILLI: [u64; 4] = [1, 250, 500, 750];
/// §6.3: "Each update is 16 game seeds × six rotations."
const SEEDS_PER_UPDATE: u64 = 16;

enum WorkerInference {
    Cpu(ti4_mlp::Actor),
    Infer(Arc<ti4_mlp::infer::InferModel>),
    Gpu(ti4_mlp::gpu_batch::GpuInferenceClient),
}

enum WorkerScopedInference {
    Cpu(Rc<ti4_mlp::Actor>),
    Infer(Arc<ti4_mlp::infer::InferModel>),
    Gpu(ti4_mlp::gpu_batch::GpuInferenceClient),
}

/// What plays the benchmark seats of `--opponent`: the frozen actor on libtorch, or packed for
/// `ti4-infer` once at the start.
enum FrozenInference {
    Cpu(Rc<ti4_mlp::Actor>),
    Infer(Arc<ti4_mlp::infer::InferModel>),
}

enum RolloutBackend {
    Cpu,
    /// `ti4-infer` on this backend: the fastest f32 one of the CPU (`ti4_infer::Backend::fastest`)
    /// for `infer`, `Avx2Vnni` with int8 weights for `infer-int8`.
    Infer(ti4_infer::Backend),
    GpuBatched {
        batch_size: usize,
        flush: Duration,
    },
}

/// The temperature a frozen benchmark seat acts at.
///
/// Near-argmax on purpose: the benchmark is a fixed standard, so it must not wander between
/// updates. `crossplay_eval` freezes its opponents the same way.
const OPPONENT_TEMPERATURE: f64 = 0.001;

/// Recorded decisions one seat may contribute from one game before the game is refused.
///
/// Two orders of magnitude above a healthy seat-game (~43 decisions), so it catches a game that has
/// stopped progressing and never a long one. See the refusal in `play_one` for why a step limit
/// alone was not enough.
const MAX_DECISIONS_PER_SEAT: usize = 4_000;

/// Integer weight of one unit in `--faction-focus` draws (weight 1.0 = this many tickets).
const FOCUS_SCALE: u32 = 100;
/// How fast `--faction-focus` follows each faction's per-update mean VP.
const FOCUS_SMOOTHING: f64 = 0.2;

/// `--faction-focus` weights per wide-roster entry from smoothed per-faction mean VP: an entry's
/// weight is `1 + focus x (table mean - its mean)`, clamped to [0.25, 4], in [`FOCUS_SCALE`] units.
/// A faction not yet seen counts as the table mean; the Keleres family takes its variants' mean.
fn focus_weights(
    roster: &[&'static str],
    smoothed: &BTreeMap<String, f64>,
    focus: f64,
) -> BTreeMap<&'static str, u32> {
    #[expect(clippy::cast_precision_loss, reason = "a few dozen factions")]
    let table = if smoothed.is_empty() {
        0.0
    } else {
        smoothed.values().sum::<f64>() / smoothed.len() as f64
    };
    roster
        .iter()
        .map(|entry| {
            let known: Vec<f64> = if *entry == ti4_engine::seating::KELERES_FAMILY {
                ti4_engine::factions::keleres::VARIANTS
                    .iter()
                    .filter_map(|variant| smoothed.get(*variant).copied())
                    .collect()
            } else {
                smoothed.get(*entry).copied().into_iter().collect()
            };
            #[expect(clippy::cast_precision_loss, reason = "at most three variants")]
            let mean = if known.is_empty() {
                table
            } else {
                known.iter().sum::<f64>() / known.len() as f64
            };
            let weight = focus.mul_add(table - mean, 1.0).clamp(0.25, 4.0);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to [25, 400]"
            )]
            let tickets = (weight * f64::from(FOCUS_SCALE)).round() as u32;
            (*entry, tickets)
        })
        .collect()
}
/// Rounds per self-play game, by stage.
///
/// Stage 2 pays for victory points and needs the four-round horizon §6.1 defines. Stage 1 pays for
/// the opening, which is decided in round one — playing three more rounds would add three rounds of
/// noise to a signal that is already complete, and cost four times the compute to do it.
const fn rounds_for(stage: ti4_training::reward::Stage) -> u32 {
    match stage {
        ti4_training::reward::Stage::One => 1,
        ti4_training::reward::Stage::Two => 4,
    }
}
/// §6.3's pilot seed base, so a run is reproducible from its update number alone.
/// Where a run's self-play seeds start. `--seed-base` moves it.
///
/// A run consumes `SEEDS_PER_UPDATE` seeds per update, so a later run that starts here replays the
/// same maps and the same openings an earlier one already trained on. Fresh weights on stale seeds
/// measure how well the policy does on games it has seen, which is not the question.
const SEED_BASE: u64 = 650_000_000;

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

/// Every flag that takes a value, so a mistyped one is refused rather than silently ignored.
const VALUE_FLAGS: &[&str] = &[
    "--bundle",
    "--clear-bonus",
    "--clearance-weight",
    "--conjunctive-weight",
    "--curriculum-seeds",
    "--demo-corpus",
    "--demo-per-update",
    "--device",
    "--entropy-final",
    "--entropy-start",
    "--expansion-weight",
    "--fleet-hoard-penalty",
    "--fleet-weight",
    "--projection-weight",
    "--home-loss-weight",
    "--phase-lock",
    "--keep-terminal-projection",
    "--projection-rank-weight",
    "--negotiation-step-penalty",
    "--r1-expansion-weight",
    "--sftt-weight",
    "--require-faction",
    "--zero-terminal-objectives",
    "--status-income-tg",
    "--fracture-entry-bonus",
    "--fracture-planet-bonus",
    "--high-vp-bonus",
    "--learning-rate",
    "--map-pool",
    "--movement-entropy",
    "--objective-weight",
    "--opponent",
    "--out",
    "--r1-bonus",
    "--r1-shaping",
    "--report-every",
    "--rounds",
    "--secret-weight",
    "--seed-base",
    "--seeds-per-update",
    "--rotations",
    "--roster",
    "--factions",
    "--record-best",
    "--record-vp",
    "--stage",
    "--strategy-diversity-weight",
    "--styx-bonus",
    "--tech-weight",
    "--temperature",
    "--trade-goods-hoard-weight",
    "--overpay-penalty",
    "--unused-resources-weight",
    "--build-weight",
    "--unit-weight",
    "--updates",
    "--vp-weight",
    "--waste-penalties",
    "--waste-penalty",
    "--zero-fleet-penalty",
    "--diag",
    "--capture-batch",
    "--rollout-backend",
    "--gpu-batch",
    "--gpu-flush-ms",
    "--self-imitation",
    "--sil-margin",
    "--sil-keep",
    "--faction-focus",
    "--gae-lambda",
    "--async-batch-games",
    "--async-publish-games",
    "--async-queue-batches",
    "--rollout-workers",
];
/// Every flag that stands alone.
const BOOLEAN_FLAGS: &[&str] = &[
    "--no-checkpoint",
    "--faction-rows-only",
    "--check-likelihood",
    "--diag-sync",
    "--hash-games",
    "--diplomacy",
    "--separate-models",
    "--collect-only",
];

fn check_flags() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if BOOLEAN_FLAGS.contains(&arg) {
            index += 1;
        } else if VALUE_FLAGS.contains(&arg) {
            if index + 1 >= args.len() {
                refuse(&format!("{arg} expects a value"));
            }
            index += 2;
        } else {
            refuse(&format!("unknown argument {arg:?}"));
        }
    }
}

/// Force a stored successful line while allowing the MLP recorder to rebuild its features.
struct Replaying {
    inner: Box<dyn Decider>,
    script: Vec<String>,
    at: usize,
    forced: Rc<RefCell<Vec<usize>>>,
    broken: Rc<RefCell<Option<String>>>,
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
            *self.broken.borrow_mut() = Some("replay script exhausted".to_owned());
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "replay script exhausted".to_owned(),
            });
        };
        self.at += 1;
        let Some(index) = choice.options.iter().position(|option| option.id == wanted) else {
            *self.broken.borrow_mut() = Some(format!("{wanted:?} was not offered"));
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

fn replay_advantage(
    frozen: &Rc<ti4_mlp::Actor>,
    trajectory: &Trajectory,
    content: &'static ContentStore,
    factions: &[FactionId],
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    pool: &Arc<ti4_sim::MapPool>,
    reward: &ti4_training::reward::Reward,
) -> Result<Vec<AdvantageDemo>, String> {
    type RecordLog = Rc<RefCell<Vec<ti4_mlp::bot::PpoRecord>>>;
    type RecordHandle = Rc<RefCell<Option<RecordLog>>>;
    let handle: RecordHandle = Rc::new(RefCell::new(None));
    let forced = Rc::new(RefCell::new(Vec::new()));
    let broken = Rc::new(RefCell::new(None));
    let wanted_faction = trajectory.faction.clone();
    let script = trajectory.decisions.clone();
    let records_handle = Rc::clone(&handle);
    let forced_handle = Rc::clone(&forced);
    let broken_handle = Rc::clone(&broken);
    let (_events, _setup, assignments, openings, _final) =
        ti4_training::rollout::audit_game_with_deciders(
            content,
            factions,
            DEFAULT,
            trajectory.seed,
            trajectory.rotation,
            ti4_training::rollout::Horizon {
                rounds: 1,
                steps: 200_000,
            },
            &ti4_training::rollout::OpeningMap::PythonPool {
                pool: Arc::clone(pool),
                tile_seed_offset: CORPUS_TILE_SEED_OFFSET,
            },
            |seated, baselines| {
                let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                for (index, (player, faction)) in seated.iter().enumerate() {
                    let row = ti4_mlp::FactionRow::of(faction.as_str())
                        .map_err(|error| format!("{player}: {error}"))?;
                    let baseline = baselines
                        .get(player)
                        .copied()
                        .ok_or_else(|| format!("{player}: no baseline"))?;
                    let temperature = trajectory.temperature_milli as f64 / 1_000.0;
                    let stream = trajectory
                        .seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(index as u64)
                        .wrapping_add(trajectory.temperature_milli);
                    let bot =
                        ti4_mlp::bot::MlpBot::sharing(frozen, vocabulary.clone(), row, stream)
                            .at_temperature(temperature)
                            .from_setup(baseline);
                    if faction.as_str() == wanted_faction {
                        let bot = bot.recording_ppo(CriticMode::Shared);
                        *records_handle.borrow_mut() = Some(bot.ppo_records());
                        let (decider, _) = bot.seat();
                        deciders.insert(
                            player.clone(),
                            Box::new(Replaying {
                                inner: decider,
                                script: script.clone(),
                                at: 0,
                                forced: Rc::clone(&forced_handle),
                                broken: Rc::clone(&broken_handle),
                            }),
                        );
                    } else {
                        let (decider, _) = bot.seat();
                        deciders.insert(player.clone(), decider);
                    }
                }
                Ok(deciders)
            },
        )?;
    if let Some(reason) = broken.borrow().clone() {
        return Err(format!("{} replay: {reason}", trajectory.faction));
    }
    let records = handle
        .borrow_mut()
        .take()
        .ok_or_else(|| "replay recorded nothing".to_owned())?;
    let target = assignments
        .iter()
        .find(|(_, faction)| faction.as_str() == wanted_faction)
        .map(|(player, _)| player)
        .ok_or_else(|| "replay target assignment missing".to_owned())?;
    let opening = openings
        .get(target)
        .ok_or_else(|| "replay target opening missing".to_owned())?;
    let recorded = records.borrow();
    let returns = ti4_training::reward::returns(
        &ti4_training::reward::Episode {
            steps: recorded.iter().map(|record| record.progress).collect(),
            final_progress: ti4_policy::progress::Progress {
                planets_gained: i64::try_from(opening.planets_gained).unwrap_or(i64::MAX),
                systems: i64::try_from(opening.systems).unwrap_or(i64::MAX),
                units_gained: i64::try_from(opening.units_gained).unwrap_or(i64::MAX),
                capacity_ships: i64::try_from(opening.capacity_ships).unwrap_or(i64::MAX),
                infantry: i64::try_from(opening.infantry).unwrap_or(i64::MAX),
                round_number: 1,
                ..ti4_policy::progress::Progress::default()
            },
            cleared: opening.cleared(),
            shortfall: opening.weighted_shortfall(1.0, 1.0),
            traded_goods: 0.0,
            strategy_card_plays: std::collections::BTreeMap::new(),
        },
        reward,
    );
    let forced = forced.borrow();
    if recorded.len() != forced.len() || recorded.len() != returns.len() {
        return Err("replay record/target/return lengths disagree".to_owned());
    }
    let scale = 1.0 / recorded.len().max(1) as f64;
    recorded
        .iter()
        .zip(forced.iter())
        .zip(returns)
        .map(|((record, chosen), return_to_go)| {
            let critic = record
                .step
                .critic
                .clone()
                .ok_or_else(|| "shared replay omitted critic".to_owned())?;
            Ok(AdvantageDemo {
                demo: Demo {
                    row: record.step.row,
                    head: record.step.head,
                    options: record.step.options.clone(),
                    chosen: *chosen,
                    weight: scale,
                },
                return_to_go,
                critic,
            })
        })
        .collect()
}

/// Policy drift on the exact visible decisions in the current clean replay slice.
fn reference_drift(
    reference: &ti4_mlp::Actor,
    current: &ti4_mlp::Actor,
    demos: &[AdvantageDemo],
) -> Result<(f64, f64), String> {
    if demos.is_empty() {
        return Ok((0.0, 0.0));
    }
    let mut kl = 0.0;
    let mut flips = 0usize;
    tch::no_grad(|| -> Result<(), String> {
        for item in demos {
            let head = reference
                .head_names()
                .get(item.demo.head)
                .copied()
                .ok_or_else(|| format!("demo head {} is out of range", item.demo.head))?;
            let before = reference
                .logits(&item.demo.options, head, item.demo.row)
                .map_err(|error| format!("reference drift scoring failed: {error}"))?
                .log_softmax(0, ti4_tensor::Kind::Float);
            let after = current
                .logits(&item.demo.options, head, item.demo.row)
                .map_err(|error| format!("current drift scoring failed: {error}"))?
                .log_softmax(0, ti4_tensor::Kind::Float);
            kl += (before.exp() * (&before - &after))
                .sum(ti4_tensor::Kind::Float)
                .double_value(&[]);
            flips += usize::from(
                before.argmax(0, false).int64_value(&[]) != after.argmax(0, false).int64_value(&[]),
            );
        }
        Ok(())
    })?;
    let count = demos.len() as f64;
    Ok((kl / count, flips as f64 / count))
}

/// One self-play game, recorded as PPO steps with §6.1's shaped per-decision returns.
///
/// Records what a seat did, and never changes it.
///
/// With `--hash-games`, hashes every choice. The notes a wasted activation is charged against come
/// from the bot itself (`MlpBot::ppo_notes`), one per recorded step, because a fleet decision has
/// no engine prompt and a planned movement step has no record: notes rebuilt from the engine's
/// prompts would no longer line up with the steps.
struct Watching {
    inner: Box<dyn Decider>,
    /// With `--hash-games`, a running hash of every choice this seat made.
    all: Option<Rc<RefCell<sha2::Sha256>>>,
    /// Every question this seat was asked, forced ones included.
    asked: usize,
}

/// A seat asked this often in one game is in a loop that slips under the engine's per-step guard
/// (each step progresses, the game never ends); healthy games ask a seat a few hundred times.
const MAX_ASKS_PER_SEAT: usize = 6_000;

impl Watching {
    /// Refuse a runaway game before it eats memory; the trainer drops it like a no-progress stall.
    fn count(&mut self, choice: &Choice) -> Result<(), ti4_engine::choice::IllegalChoice> {
        self.asked += 1;
        if self.asked > MAX_ASKS_PER_SEAT {
            return Err(ti4_engine::choice::IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: format!(
                    "asked {} times in one game without progress to its end",
                    self.asked
                ),
            });
        }
        Ok(())
    }

    fn record(&self, choice: &Choice, chosen: &ti4_engine::choice::ChoiceOption) {
        // Every choice, forced ones included, in order: who chose, from what, and which.
        if let Some(all) = &self.all {
            use sha2::Digest as _;
            let mut hasher = all.borrow_mut();
            hasher.update(choice.player.as_str().as_bytes());
            hasher.update([0x1d]);
            for option in &choice.options {
                hasher.update(option.id.as_bytes());
                hasher.update([0x1f]);
            }
            hasher.update(chosen.id.as_bytes());
            hasher.update([0x1e]);
        }
    }
}

impl Decider for Watching {
    fn choose(
        &mut self,
        choice: &Choice,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        self.count(choice)?;
        let chosen = self.inner.choose(choice)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &ti4_engine::choice::SeatObservation<'_>,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        self.count(choice)?;
        let chosen = self.inner.choose_seeing(choice, seen)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }
}

/// Everything a game needs is passed in rather than captured, because this runs on a rayon worker:
/// `tch::Tensor` is `Send` but **not** `Sync`, so the actor cannot be shared by reference across
/// threads and each worker owns its own inference copy.
#[expect(
    clippy::too_many_arguments,
    reason = "a game's inputs; bundling them into a struct would move the list, not shorten it"
)]
fn play_one(
    // One per model: the shared model alone, or one per faction of FACTIONS (--separate-models).
    inference: &[WorkerScopedInference],
    opponent: Option<&FrozenInference>,
    learner_faction: usize,
    content: &ContentStore,
    players: &[PlayerId],
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    pool: &Arc<ti4_sim::MapPool>,
    reward: &ti4_training::reward::Reward,
    critic_mode: ti4_mlp::bundle::CriticMode,
    rounds: u32,
    seed: u64,
    rotation: usize,
    temperature: f64,
    waste_penalties: &[f64],
    waste_default: f64,
    roster: ti4_engine::seating::FactionRoster,
    diplomacy: bool,
    hash: bool,
    sil_margin: f64,
    focus: Option<&BTreeMap<&'static str, u32>>,
    gae_lambda: f64,
    record: Option<&Recorder>,
    update: usize,
) -> Result<Played, String> {
    // The six this game draws: FACTIONS itself, or a seeded wide draw shared by every rotation of
    // the seed (BF-22). With --faction-focus the wide draw is weighted toward trailing factions.
    let factions = match focus {
        None => ti4_training::rollout::game_factions(
            content, roster, &*FACTIONS, players, DEFAULT, seed,
        )?,
        Some(weights) => {
            let weight = |entry: &str| weights.get(entry).copied().unwrap_or(FOCUS_SCALE);
            let drawn =
                ti4_engine::seating::seat_wide_weighted(content, players, DEFAULT, seed, &weight)
                    .map_err(|error| format!("weighted wide roster for seed {seed}: {error}"))?;
            players
                .iter()
                .map(|player| {
                    drawn
                        .get(player)
                        .cloned()
                        .ok_or_else(|| format!("weighted roster seated no faction for {player}"))
                })
                .collect::<Result<Vec<_>, String>>()?
        }
    };
    let seated: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            (
                player.clone(),
                ti4_training::rollout::seated_faction(&factions, seed, rotation, index),
            )
        })
        .collect();

    // Deciders are built by a factory so each seat gets the **exact** post-deployment baseline the
    // rollout will score its final progress against. Constructing them earlier cannot supply that,
    // and a shaped return measured against a different baseline is not the return §6.1 defines
    // (F-M10-034-D1).
    //
    // The handles are `Rc`, which is exactly why they are created, filled and drained inside this
    // function: nothing thread-local ever crosses back to the caller.
    // Which physical seat the learner occupies this game: the one holding the faction this
    // rotation trains. `seated` is the seeded permutation, so this varies by seed as intended.
    let learner_seat = {
        let wanted = factions[learner_faction % factions.len()].as_str();
        players
            .iter()
            .position(|player| {
                seated
                    .get(player)
                    .is_some_and(|faction| faction.as_str() == wanted)
            })
            .unwrap_or(0)
    };
    let mut handles: BTreeMap<PlayerId, _> = BTreeMap::new();
    let mut statuses = Vec::new();
    let mut choice_hashers: BTreeMap<PlayerId, Rc<RefCell<sha2::Sha256>>> = BTreeMap::new();
    let mut watched: BTreeMap<
        PlayerId,
        std::rc::Rc<std::cell::RefCell<Vec<ti4_mlp::positive_corpus::Note>>>,
    > = BTreeMap::new();
    let (rollout, game_digest) =
        ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
            content,
            players,
            &seated,
            DEFAULT,
            seed,
            ti4_training::rollout::Horizon {
                rounds,
                steps: 10_000,
            },
            ti4_engine::opening::DEFAULT_REQUIREMENT,
            &ti4_training::rollout::OpeningMap::PythonPool {
                pool: Arc::clone(pool),
                tile_seed_offset: TILE_SEED_OFFSET,
            },
            ti4_training::rollout::SimulationCapabilities { diplomacy },
            hash,
            |baselines| {
                let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                for (index, player) in players.iter().enumerate() {
                    let row = ti4_mlp::FactionRow::of(seated[player].as_str())
                        .map_err(|error| format!("{player}: {error}"))?;
                    let baseline = baselines
                        .get(player)
                        .copied()
                        .ok_or_else(|| format!("{player} has no setup baseline"))?;
                    let stream = seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(u64::try_from(index).unwrap_or(0));
                    // CPU workers share one local read-only actor. The experimental GPU path keeps
                    // the per-seat state here but routes immutable sparse inputs to one owner.
                    let learns = opponent.is_none() || index == learner_seat;
                    let bot = if let (false, Some(frozen)) = (learns, opponent) {
                        // A benchmark seat: frozen weights, near-argmax, and no PPO recording, so
                        // nothing it does can reach the learner's batch.
                        match frozen {
                            FrozenInference::Cpu(actor) => ti4_mlp::bot::MlpBot::sharing(
                                actor,
                                vocabulary.clone(),
                                row,
                                stream,
                            ),
                            FrozenInference::Infer(model) => ti4_mlp::bot::MlpBot::on_infer(
                                model,
                                vocabulary.clone(),
                                row,
                                stream,
                            ),
                        }
                        .at_temperature(OPPONENT_TEMPERATURE)
                        .from_setup(baseline)
                    } else {
                        let model = if inference.len() == 1 {
                            0
                        } else {
                            FACTIONS
                                .iter()
                                .position(|faction| *faction == seated[player].as_str())
                                .ok_or_else(|| {
                                    format!("{player}: no model for {}", seated[player])
                                })?
                        };
                        match &inference[model] {
                            WorkerScopedInference::Cpu(actor) => ti4_mlp::bot::MlpBot::sharing(
                                actor,
                                vocabulary.clone(),
                                row,
                                stream,
                            ),
                            WorkerScopedInference::Infer(model) => ti4_mlp::bot::MlpBot::on_infer(
                                model,
                                vocabulary.clone(),
                                row,
                                stream,
                            ),
                            WorkerScopedInference::Gpu(client) => ti4_mlp::bot::MlpBot::batched(
                                client.clone(),
                                vocabulary.clone(),
                                row,
                                stream,
                            ),
                        }
                        .at_temperature(temperature)
                        .recording_ppo(critic_mode)
                        .from_setup(baseline)
                    };
                    if learns && handles.insert(player.clone(), bot.ppo_records()).is_some() {
                        return Err(format!("{player} was seated twice"));
                    }
                    let log = bot.ppo_notes();
                    let (decider, status) = bot.seat();
                    statuses.push(status);
                    let all = hash.then(|| {
                        let hasher = Rc::new(RefCell::new(<sha2::Sha256 as sha2::Digest>::new()));
                        choice_hashers.insert(player.clone(), Rc::clone(&hasher));
                        hasher
                    });
                    watched.insert(player.clone(), std::rc::Rc::clone(&log));
                    deciders.insert(
                        player.clone(),
                        Box::new(Watching {
                            inner: decider,
                            all,
                            asked: 0,
                        }),
                    );
                }
                Ok(deciders)
            },
        );
    if let Some(error) = &rollout.error {
        return Err(format!("self-play game {seed}/{rotation} failed: {error}"));
    }
    for status in statuses {
        status
            .into_result()
            .map_err(|error| format!("self-play game {seed}/{rotation}: {error}"))?;
    }

    // The returns, matched to the seat that earned them. A missing handle is refused rather than
    // skipped: silently dropping a seat shrinks the batch and nothing downstream would notice
    // (F-M10-034-D4).
    let mut steps: Vec<Step> = Vec::new();
    let mut outcomes: Vec<SeatOutcome> = Vec::new();
    let mut wasted = 0usize;
    // Tactical actions taken, reported per update beside the waste count. A policy can drive waste
    // to nothing by refusing to activate at all -- measured, a control arm did exactly that, cutting
    // tactical actions 37% and losing 1.68 points of clearance -- and the waste figure alone cannot
    // tell that apart from getting better. Seen together they can.
    let mut tactical = 0usize;
    // Which of the returned steps came from a seat that beat its own table's mean final VP by
    // `sil_margin`; aligned one-to-one with `steps`. The mean is over every seat of the game,
    // frozen opponents included, because the point is "better than the table", not "better than
    // the other learners".
    let mut qualifying: Vec<Option<usize>> = Vec::new();
    #[expect(clippy::cast_precision_loss, reason = "six seats; VP counts are tiny")]
    let table_mean = rollout
        .seats
        .iter()
        .map(|seat| seat.episode.final_progress.victory_points as f64)
        .sum::<f64>()
        / rollout.seats.len().max(1) as f64;
    // Projection rank per round: each seat's projection and non-home planets held at its last
    // recorded decision of the round.
    // shortcut: seats are measured at their own last decision, not one shared state; measure every
    // seat at the round boundary if rank timing turns out to matter.
    let mut round_projection: BTreeMap<u32, Vec<(PlayerId, i64, i64)>> = BTreeMap::new();
    if reward.projection_rank_weight != 0.0 {
        for (player, handle) in &handles {
            let mut last: BTreeMap<u32, (i64, i64)> = BTreeMap::new();
            for record in handle.borrow().iter() {
                let progress = &record.progress;
                // Planets beyond the setup baseline, plus home planets lost: the planets held
                // outside home.
                let non_home = progress.planets_gained + progress.home_planets_lost;
                last.insert(
                    progress.round_number,
                    (progress.projection_permille, non_home),
                );
            }
            for (round, (projection, non_home)) in last {
                round_projection.entry(round).or_default().push((
                    player.clone(),
                    projection,
                    non_home,
                ));
            }
        }
    }
    for (seat_number, seat) in rollout.seats.iter().enumerate() {
        // A frozen seat is not this policy: reporting its clearance and points as the learner's
        // would read the benchmark's play as progress.
        let Some(handle) = handles.get(&seat.player) else {
            if opponent.is_some() {
                continue;
            }
            return Err(format!(
                "seed {seed} rotation {rotation}: {} has no recording handle",
                seat.player
            ));
        };
        outcomes.push(SeatOutcome {
            faction: seat.faction.to_string(),
            cleared: seat.episode.cleared,
            victory_points: seat.episode.final_progress.victory_points,
        });
        let mut recorded = handle.borrow_mut();
        // A single seat cannot contribute more decisions than a sane game has.
        //
        // Each recorded decision stores the sparse feature vector of *every* legal option, which is
        // what the importance ratio needs and what makes memory scale with steps x options. A game
        // that stops progressing therefore does not merely run long: it allocates. One at
        // temperature 0.25 reached 53 GB on a 96 GB machine, at which point every engine step was a
        // page fault and the update that would have taken 1.5 seconds had not finished 38 minutes
        // later. It never reached the step limit, so nothing named it and nothing refused it.
        //
        // Refusing here turns that into a fast, named failure with the seed and seat attached,
        // *before* the machine starts swapping. The ceiling is far above any real game -- a whole
        // healthy update is ~25,000 decisions across 96 games and six seats, so ~43 per seat-game.
        if recorded.len() > MAX_DECISIONS_PER_SEAT {
            return Err(format!(
                "seed {seed} rotation {rotation} {}: {} recorded decisions exceeds the {} ceiling;                  the game was not progressing",
                seat.player,
                recorded.len(),
                MAX_DECISIONS_PER_SEAT
            ));
        }
        // §6.1's shaped per-decision return. Each recorded decision carries the progress measured
        // **at** that decision against the seat's own setup baseline, so `returns` can telescope
        // them into a return-to-go per decision.
        //
        // The first version built a one-step episode from the final progress and gave every
        // decision in the game the same number. The advantage is `return − V(s)`, so with a
        // constant return the only thing separating decisions was the critic, and the within-game
        // credit assignment §6.1's shaping exists for was gone. It trained; the objective was wrong.
        let episode = ti4_training::reward::Episode {
            steps: recorded.iter().map(|record| record.progress).collect(),
            final_progress: seat.episode.final_progress,
            cleared: seat.episode.cleared,
            shortfall: seat.episode.shortfall,
            traded_goods: seat.episode.traded_goods,
            strategy_card_plays: seat.episode.strategy_card_plays.clone(),
        };
        let per_decision = ti4_training::reward::returns(&episode, reward);
        {
            // Telemetry, always on: what the three production terms would price in this seat-game.
            let mut all = episode.steps.clone();
            all.push(episode.final_progress);
            let terms = ti4_training::reward::production_terms(&all);
            PRODUCTION_TELEMETRY[0].fetch_add(
                terms.overpay.iter().map(|t| t.1).sum(),
                std::sync::atomic::Ordering::Relaxed,
            );
            PRODUCTION_TELEMETRY[1].fetch_add(
                terms.unused.iter().map(|t| t.1).sum(),
                std::sync::atomic::Ordering::Relaxed,
            );
            PRODUCTION_TELEMETRY[2].fetch_add(
                terms.built_permille.iter().map(|t| t.1).sum(),
                std::sync::atomic::Ordering::Relaxed,
            );
            PRODUCTION_TELEMETRY[3].fetch_add(
                episode.final_progress.fleet_value_permille,
                std::sync::atomic::Ordering::Relaxed,
            );
        }
        if per_decision.len() != recorded.len() {
            return Err(format!(
                "seed {seed} rotation {rotation} {}: {} returns for {} decisions",
                seat.player,
                per_decision.len(),
                recorded.len()
            ));
        }
        for (record, value) in recorded.iter_mut().zip(&per_decision) {
            record.step.return_to_go = *value;
        }
        // The projection rank bonus, injected at the seat's last decision of each round like the
        // waste charge below: every return up to that decision carries it, discounted. The mean of
        // the projection rank and the non-home planet rank, so neither needs a scale.
        for (round, table) in &round_projection {
            let Some(mine) = table.iter().find(|(player, ..)| *player == seat.player) else {
                continue;
            };
            let Some(end) = recorded
                .iter()
                .rposition(|r| r.progress.round_number == *round)
            else {
                continue;
            };
            let bonus = 0.5
                * (projection_rank_bonus(mine.1, table.iter().map(|(_, p, _)| *p))
                    + projection_rank_bonus(mine.2, table.iter().map(|(_, _, n)| *n)))
                * reward.projection_rank_weight;
            for (index, record) in recorded.iter_mut().enumerate().take(end + 1) {
                let steps_away = i32::try_from(end - index).unwrap_or(i32::MAX);
                record.step.return_to_go += bonus * reward.discount.powi(steps_away);
            }
        }

        // Charge each wasted tactical action as a reward at the point the segment closes.
        //
        // Nothing in the stage-1 reward objects to a tactical action that activates a system and
        // then neither moves, builds, nor lands: clearance is all it prices, and a seat that has
        // already met the bar is free to spend its last turn on nothing. Measured, the champion
        // does exactly that in 60.6% of its games -- roughly a fifth of a seat's ~4.6 actions.
        //
        // The charge lands on the activation itself rather than on the episode. Spreading it over
        // fifty decisions would put 98% of the gradient on decisions that were not the mistake,
        // which is the credit-assignment failure this project has already paid for once.
        // Counted always, charged only when a penalty is set. Gating the *count* on the penalty
        // made the zero-penalty control report 0.000 wasted activations, which is the one arm whose
        // whole purpose is to say what PPO does to waste when nothing objects to it.
        if let Some(notes) = watched.get(&seat.player) {
            let notes = notes.borrow();
            // Diagnostic (TI4_DIPLOMACY_CENSUS=1): this seat's diplomacy choices by option kind,
            // digits stripped, one stderr line per seat-game.
            if std::env::var_os("TI4_DIPLOMACY_CENSUS").is_some() {
                let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
                for note in notes.iter().filter(|note| note.head == "diplomacy") {
                    let kind: String = note
                        .chosen
                        .chars()
                        .filter(|c| !c.is_ascii_digit())
                        .collect();
                    *kinds.entry(kind).or_default() += 1;
                }
                eprintln!("DIPLOMACY_CENSUS {}", serde_json::json!(kinds));
            }
            if notes.len() != recorded.len() {
                // The two logs must describe the same decisions. If they do not, the indices mean
                // nothing and charging by them would penalise arbitrary decisions.
                return Err(format!(
                    "seed {seed} rotation {rotation} {}: {} notes for {} recorded decisions",
                    seat.player,
                    notes.len(),
                    recorded.len()
                ));
            }
            tactical += notes
                .iter()
                .filter(|note| {
                    note.head == "turn"
                        && note.chosen == ti4_mlp::positive_corpus::TACTICAL_ACTION_ID
                })
                .count();
            // A reward of `-penalty` at every diplomacy decision, as one reverse pass: the same
            // discounted injection as the waste charge below, without its per-end loop (a haggling
            // seat makes thousands of these).
            if reward.negotiation_step_penalty != 0.0 {
                let flags: Vec<bool> = notes.iter().map(|note| note.head == "diplomacy").collect();
                let charges =
                    discounted_charges(&flags, reward.negotiation_step_penalty, reward.discount);
                for (record, charge) in recorded.iter_mut().zip(charges) {
                    record.step.return_to_go -= charge;
                }
            }
            let ends = ti4_mlp::positive_corpus::wasted_segment_ends(&notes);
            wasted += ends.len();

            // A reward of `-penalty` at the decision where the segment closes, which is what
            // "avoid wasting a tactical action" means. `returns` is a discounted suffix sum, so
            // injecting a reward at `end` is exactly equivalent to subtracting
            // `gamma^(end - t) * penalty` from every return at `t <= end` -- done here rather than
            // by rebuilding the reward vector, because the reward crate's shaping is not this
            // tool's to reach into.
            //
            // The first version subtracted the penalty from the activation decision alone. That is
            // not reward shaping: whether an activation *becomes* wasted is decided by the movement,
            // production and landing declines that follow it, and those received no signal at all.
            // It trained "avoid activations that tend to end up wasted under the current stochastic
            // continuation", which is a policy-relative label of exactly the kind that has already
            // failed once in this project.
            // The seat's own penalty, by faction. Charged where the seat sits, not where the
            // sweep's scalar happens to be.
            let waste_penalty = FACTIONS
                .iter()
                .position(|name| *name == seat.faction.as_str())
                .and_then(|index| waste_penalties.get(index))
                .copied()
                // A faction outside the six (`--roster wide`) takes the scalar penalty.
                .unwrap_or(waste_default);
            if waste_penalty > 0.0 {
                let gamma = reward.discount;
                for end in ends {
                    for (index, record) in recorded.iter_mut().enumerate().take(end + 1) {
                        let steps_away = u32::try_from(end - index).unwrap_or(u32::MAX);
                        record.step.return_to_go -= waste_penalty
                            * gamma.powi(i32::try_from(steps_away).unwrap_or(i32::MAX));
                    }
                }
            }
        }
        if gae_lambda < 1.0 {
            let returns: Vec<f64> = recorded.iter().map(|r| r.step.return_to_go).collect();
            let values: Vec<f64> = recorded
                .iter()
                .map(|r| r.step.behaviour_value.unwrap_or(0.0))
                .collect();
            let targets =
                ti4_mlp::ppo::lambda_returns(&returns, &values, reward.discount, gae_lambda);
            for (record, target) in recorded.iter_mut().zip(targets) {
                record.step.return_to_go = target;
            }
        }
        #[expect(clippy::cast_precision_loss, reason = "victory points are tiny")]
        let seat_qualifies =
            seat.episode.final_progress.victory_points as f64 >= table_mean + sil_margin;
        qualifying.extend(std::iter::repeat_n(
            seat_qualifies.then_some(seat_number),
            recorded.len(),
        ));
        let first = steps.len();
        steps.extend(recorded.drain(..).map(|record| record.step));
        // --record-best: this seat's exact training records, when it reached its faction's bar.
        if let Some(recorder) = record {
            let points = seat.episode.final_progress.victory_points;
            if points >= recorder.bar(seat.faction.as_str()) {
                recorder
                    .write(
                        &steps[first..],
                        seat.faction.as_str(),
                        update,
                        seed,
                        rotation,
                        &seat.player,
                        points,
                    )
                    .map_err(|error| format!("--record-best: {error}"))?;
            }
        }
    }
    // With --hash-games: what this game did, in forms two runs can be compared by exactly.
    let record = if hash {
        let choices: BTreeMap<String, String> = choice_hashers
            .iter()
            .map(|(player, hasher)| {
                (
                    player.as_str().to_owned(),
                    format!(
                        "{:x}",
                        <sha2::Sha256 as sha2::Digest>::finalize(hasher.borrow().clone())
                    ),
                )
            })
            .collect();
        let digest = game_digest.as_ref();
        Some(serde_json::json!({
            "kind": "game",
            "seed": seed,
            "rotation": rotation,
            "state_sha256": digest.map(|d| d.state_sha256.clone()),
            "log_sha256": digest.map(|d| d.log_sha256.clone()),
            "events": digest.map(|d| d.events),
            "choices_sha256": choices,
            "steps_sha256": ti4_mlp::perf::steps_digest(&steps).map_err(|error| error.to_string())?,
            "steps": steps.len(),
            "outcomes": outcomes
                .iter()
                .map(|o| serde_json::json!([o.faction, o.cleared, o.victory_points]))
                .collect::<Vec<_>>(),
        }))
    } else {
        None
    };
    if qualifying.len() != steps.len() {
        return Err(format!(
            "seed {seed} rotation {rotation}: {} self-imitation flags for {} steps",
            qualifying.len(),
            steps.len()
        ));
    }
    Ok((steps, outcomes, wasted, tactical, record, qualifying))
}

/// `--record-best`: where a seat's training records go when its final points reach its faction's
/// bar (`--record-vp`, e.g. `6,hacan=7`: 6 for every faction, 7 for Hacan).
struct Recorder {
    directory: std::path::PathBuf,
    default_bar: i64,
    bars: BTreeMap<String, i64>,
}

impl Recorder {
    fn parse(directory: String, bars: &str) -> Result<Self, String> {
        let mut default_bar = None;
        let mut by_faction = BTreeMap::new();
        for part in bars.split(',').map(str::trim) {
            match part.split_once('=') {
                Some((faction, points)) => {
                    ti4_mlp::FactionRow::of(faction).map_err(|error| error.to_string())?;
                    let points = points
                        .parse()
                        .map_err(|_| format!("--record-vp {part}: not a number of points"))?;
                    by_faction.insert(faction.to_owned(), points);
                }
                None => {
                    default_bar = Some(
                        part.parse()
                            .map_err(|_| format!("--record-vp {part}: not a number of points"))?,
                    );
                }
            }
        }
        Ok(Self {
            directory: directory.into(),
            default_bar: default_bar.ok_or("--record-vp needs a default bar, e.g. 6,hacan=7")?,
            bars: by_faction,
        })
    }

    fn bar(&self, faction: &str) -> i64 {
        self.bars.get(faction).copied().unwrap_or(self.default_bar)
    }

    /// `<dir>/<faction>/u<update>-s<seed>-r<rotation>-<seat>-vp<points>.ti4ppo.zst`, read back by
    /// `ti4_mlp::perf::read_capture`.
    #[expect(
        clippy::too_many_arguments,
        reason = "the record's name is these facts"
    )]
    fn write(
        &self,
        steps: &[Step],
        faction: &str,
        update: usize,
        seed: u64,
        rotation: usize,
        player: &PlayerId,
        points: i64,
    ) -> std::io::Result<()> {
        let directory = self.directory.join(faction);
        std::fs::create_dir_all(&directory)?;
        let name = format!("u{update:04}-s{seed}-r{rotation}-{player}-vp{points}.ti4ppo.zst");
        ti4_mlp::perf::write_capture_zstd(steps, &directory.join(name))
    }
}

/// One played game: the decisions it contributed and what each seat ended with.
type Played = (
    Vec<Step>,
    Vec<SeatOutcome>,
    usize,
    usize,
    Option<serde_json::Value>,
    // Per step: the seat number when that seat beat its table (see `play_one`), else None.
    Vec<Option<usize>>,
);

/// What one seat's game produced, beyond the decisions it contributed to the batch.
///
/// Taken from the training games themselves rather than a separate evaluation pass: those games are
/// already being played, already sampled from the current policy, and 96 games an update is 9,600
/// per hundred-update report. A dedicated eval would cost compute and see fewer games.
#[derive(Clone)]
struct SeatOutcome {
    faction: String,
    cleared: bool,
    victory_points: i64,
}

/// Faction-level totals accumulated across a reporting window.
#[derive(Clone, Copy, Default)]
struct FactionTally {
    games: usize,
    cleared: usize,
    victory_points: i64,
}

impl FactionTally {
    #[expect(
        clippy::cast_precision_loss,
        reason = "game counts are exact in f64 far beyond any run length"
    )]
    fn clearance(self) -> f64 {
        if self.games == 0 {
            return 0.0;
        }
        self.cleared as f64 / self.games as f64 * 100.0
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "game counts and points are exact in f64"
    )]
    fn mean_points(self) -> f64 {
        if self.games == 0 {
            return 0.0;
        }
        self.victory_points as f64 / self.games as f64
    }

    fn add(&mut self, outcome: &SeatOutcome) {
        self.games += 1;
        self.cleared += usize::from(outcome.cleared);
        self.victory_points += outcome.victory_points;
    }
}

/// Print a window's stage-one clearance and victory points per faction, against the window before.
///
/// The deltas are what make this readable as progress rather than as a snapshot; the first report
/// has nothing to compare against and says so instead of printing a zero, which would read as "no
/// movement" rather than "no baseline".
fn report(
    update: usize,
    window: &BTreeMap<String, FactionTally>,
    previous: Option<&BTreeMap<String, FactionTally>>,
    reported_at: usize,
) {
    let first = reported_at + 1;
    let seats: usize = window.values().map(|tally| tally.games).sum();
    println!(
        "\n  ===== report after update {update} (updates {first}-{update}, {seats} seat-games) ====="
    );
    println!("  faction      games   stage-1 clearance        mean VP");

    let mut table = FactionTally::default();
    let mut previous_table = FactionTally::default();
    for (faction, tally) in window {
        table.games += tally.games;
        table.cleared += tally.cleared;
        table.victory_points += tally.victory_points;
        let before = previous.and_then(|earlier| earlier.get(faction)).copied();
        if let Some(before) = before {
            previous_table.games += before.games;
            previous_table.cleared += before.cleared;
            previous_table.victory_points += before.victory_points;
        }
        print_row(faction, *tally, before);
    }
    println!("  {:-<58}", "");
    print_row(
        "table",
        table,
        (previous_table.games > 0).then_some(previous_table),
    );
}

fn print_row(name: &str, tally: FactionTally, previous: Option<FactionTally>) {
    let clearance = tally.clearance();
    let points = tally.mean_points();
    match previous {
        Some(before) => println!(
            "  {:<10} {:>6}   {:>6.2}%  ({:+.2})   {:>6.3}  ({:+.3})",
            name,
            tally.games,
            clearance,
            clearance - before.clearance(),
            points,
            points - before.mean_points(),
        ),
        None => println!(
            "  {:<10} {:>6}   {:>6.2}%      (--)   {:>6.3}      (--)",
            name, tally.games, clearance, points
        ),
    }
}

/// Write a checkpoint and verify it loads back to the weights that were trained.
///
/// A multi-day run with no resume (M10-035 is not built) would otherwise keep every update's work
/// in one process's memory. Publishing at each report bounds what a crash costs to one window.
fn publish(
    actor: &ti4_mlp::Actor,
    destination: &std::path::Path,
    slots_text: &str,
    critic_mode: ti4_mlp::bundle::CriticMode,
    provenance: &ti4_mlp::bundle::Provenance,
    expected: &[u32],
) {
    let cpu = actor.inference_copy().to_device(ti4_tensor::Device::Cpu);
    let bundle = ti4_mlp::bundle::write(destination, &cpu, slots_text, critic_mode, provenance)
        .unwrap_or_else(|error| refuse(&format!("writing the checkpoint: {error}")));
    let reloaded = ti4_mlp::bundle::read(&bundle.directory)
        .unwrap_or_else(|error| refuse(&format!("the checkpoint does not load: {error}")));
    let fingerprint = ti4_mlp::ppo::parameter_fingerprint(&reloaded.actor, reloaded.critic_mode)
        .unwrap_or_else(|error| refuse(&format!("fingerprinting the reload: {error}")));
    if fingerprint != expected {
        refuse("the reloaded checkpoint does not match the weights that were trained");
    }
    println!(
        "  checkpoint  {} (reloaded, identical)",
        bundle.directory.display()
    );
}

/// How often to report, allowing the cadence to loosen as a run gets longer.
///
/// Written `50:500,500` — every 50 updates until update 500, every 500 after that. Early windows
/// are where a run either starts moving or does not, and that is worth watching closely; ten hours
/// later the same cadence would be 700 reports nobody reads.
///
/// Each report resets the window, so a delta always compares consecutive windows. When the cadence
/// changes, the first long window is compared against the last short one — rates and means stay
/// comparable, the sample sizes do not, which is why every report prints its own span and seat-game
/// count rather than leaving the reader to assume they match.
struct Cadence {
    /// `(every, until)`, in order. The last segment's `until` is open.
    segments: Vec<(usize, Option<usize>)>,
}

impl Cadence {
    fn parse(text: &str) -> Result<Self, String> {
        let mut segments = Vec::new();
        for (position, piece) in text.split(',').enumerate() {
            let piece = piece.trim();
            let (every, until) = match piece.split_once(':') {
                Some((every, until)) => (
                    every.trim(),
                    Some(until.trim().parse::<usize>().map_err(|_| {
                        format!("segment {position} of --report-every: '{until}' is not a number")
                    })?),
                ),
                None => (piece, None),
            };
            let every: usize = every.parse().map_err(|_| {
                format!("segment {position} of --report-every: '{every}' is not a number")
            })?;
            if every == 0 {
                return Err("--report-every cannot report every 0 updates".to_owned());
            }
            segments.push((every, until));
        }
        if segments.is_empty() {
            return Err("--report-every is empty".to_owned());
        }
        // A bounded segment after an unbounded one can never be reached.
        if let Some(position) = segments
            .iter()
            .position(|(_, until)| until.is_none())
            .filter(|position| *position + 1 < segments.len())
        {
            return Err(format!(
                "--report-every segment {position} is unbounded, so the segments after it are dead"
            ));
        }
        Ok(Self { segments })
    }

    /// The interval in force at `done`, and whether a report falls on it.
    fn interval(&self, done: usize) -> usize {
        self.segments
            .iter()
            .find(|(_, until)| until.is_none_or(|until| done <= until))
            .or_else(|| self.segments.last())
            .map_or(1, |(every, _)| *every)
    }

    fn due(&self, done: usize) -> bool {
        done.is_multiple_of(self.interval(done))
    }
}

#[cfg(test)]
mod cadence_tests {
    use super::Cadence;

    #[test]
    fn a_loosening_cadence_reports_where_it_says_it_will() {
        let cadence = Cadence::parse("50:500,500").expect("a valid cadence");
        let due: Vec<usize> = (1..=2_000).filter(|done| cadence.due(*done)).collect();
        let mut expected: Vec<usize> = (1..=10).map(|n| n * 50).collect();
        expected.extend([1_000, 1_500, 2_000]);
        assert_eq!(due, expected);
    }

    #[test]
    fn a_bare_interval_still_means_every_n() {
        let cadence = Cadence::parse("100").expect("a valid cadence");
        assert_eq!(
            (1..=350)
                .filter(|done| cadence.due(*done))
                .collect::<Vec<_>>(),
            vec![100, 200, 300]
        );
    }

    #[test]
    fn a_cadence_that_could_never_fire_is_refused() {
        // Zero would divide by zero; a segment after an unbounded one is unreachable. Both are
        // operator typos that would otherwise show up only as silence hours into a run.
        assert!(Cadence::parse("0").is_err());
        assert!(Cadence::parse("500,50:100").is_err());
        assert!(Cadence::parse("50:x,500").is_err());
    }
}

/// One finished game of the async rollout: job index, policy version it was played with, result.
type Finished = (usize, usize, Result<Played, String>);

/// Shared state of `--async-batch-games`: the rollout workers, which play games continuously, and
/// the optimiser, which trains on the batches they cut.
#[derive(Default)]
struct AsyncState {
    next_job: usize,
    /// Batches the optimiser has taken off the queue.
    consumed: usize,
    pending: Vec<Finished>,
    ready: std::collections::VecDeque<Vec<Finished>>,
    stop: bool,
    failure: Option<String>,
    warned: bool,
    // Since the optimiser last took a batch, summed over workers.
    blocked: Duration,
    finished: usize,
}

struct AsyncShared {
    state: std::sync::Mutex<AsyncState>,
    changed: std::sync::Condvar,
    /// The policy version (optimiser steps done) the packed weights are from, and the weights.
    weights: std::sync::RwLock<(usize, Vec<Arc<ti4_mlp::infer::InferModel>>)>,
    focus: std::sync::RwLock<Option<Arc<BTreeMap<&'static str, u32>>>>,
    batch_games: usize,
    queue_batches: usize,
}

fn flag_count(name: &str) -> Option<usize> {
    argument(name).map(|value| {
        value
            .parse()
            .ok()
            .filter(|count| *count > 0)
            .unwrap_or_else(|| refuse(&format!("{name} expects a positive integer")))
    })
}

fn main() {
    let process_started = Instant::now();
    check_flags();
    // Diagnostics (plans/TRAINING_PERFORMANCE_HANDOFF_2026-09-11.md), all off by default: with none
    // of these flags the run is exactly the run it always was.
    let diag_path = argument("--diag");
    let capture_path = argument("--capture-batch");
    let no_checkpoint = std::env::args().any(|a| a == "--no-checkpoint");
    // Before any optimizer step, rescore every recorded step with the weights that played it: the
    // recorded behaviour probability must come back, for fleet decisions as for engine prompts.
    let check_likelihood = std::env::args().any(|a| a == "--check-likelihood");
    let hash_games = std::env::args().any(|a| a == "--hash-games");
    let diag_sync = std::env::args().any(|a| a == "--diag-sync");
    let diplomacy = std::env::args().any(|a| a == "--diplomacy");
    // Which factions a game seats (BF-22). `six` is the historical FACTIONS rotation; `wide` draws
    // six distinct factions per seed from the whole implemented roster, and the bundle's pinned
    // faction rows (ti4_mlp::FACTION_ROSTER) condition every one of them.
    let roster = argument("--roster")
        .map_or(ti4_engine::seating::FactionRoster::InScope, |value| {
            ti4_training::rollout::parse_roster(&value).unwrap_or_else(|error| refuse(&error))
        });
    if diag_path.is_some() {
        ti4_mlp::perf::enable(diag_sync);
    } else if hash_games || diag_sync {
        refuse("--hash-games and --diag-sync need --diag <file>");
    }
    let mut diag = diag_path.as_ref().map(|path| {
        std::fs::File::options()
            .create(true)
            .append(true)
            .open(path)
            .unwrap_or_else(|error| refuse(&format!("opening {path}: {error}")))
    });
    let updates: usize = argument("--updates")
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let optimizer_device = match argument("--device").as_deref() {
        None | Some("cpu") => ti4_tensor::OptimizerDevice::Cpu,
        Some("cuda") => ti4_tensor::OptimizerDevice::Cuda,
        Some(other) => refuse(&format!("--device {other}: expected cpu or cuda")),
    };
    let device = optimizer_device
        .resolve()
        .unwrap_or_else(|error| refuse(&format!("--device cuda: {error}")));
    let rollout_backend = match argument("--rollout-backend").as_deref().unwrap_or("cpu") {
        "cpu" => RolloutBackend::Cpu,
        "infer" => RolloutBackend::Infer(ti4_infer::Backend::fastest()),
        "infer-int8" => {
            if !ti4_infer::Backend::Avx2Vnni.is_available() {
                refuse("--rollout-backend infer-int8 needs a CPU with AVX2, FMA and AVX-VNNI");
            }
            RolloutBackend::Infer(ti4_infer::Backend::Avx2Vnni)
        }
        "gpu-batched" => {
            if !matches!(device, ti4_tensor::Device::Cuda(_)) {
                refuse("--rollout-backend gpu-batched requires --device cuda");
            }
            let batch_size = argument("--gpu-batch")
                .map_or(Ok(32usize), |value| value.parse::<usize>())
                .unwrap_or_else(|_| refuse("--gpu-batch expects an unsigned integer"));
            let flush_ms = argument("--gpu-flush-ms")
                .map_or(Ok(2u64), |value| value.parse::<u64>())
                .unwrap_or_else(|_| refuse("--gpu-flush-ms expects an unsigned integer"));
            RolloutBackend::GpuBatched {
                batch_size,
                flush: Duration::from_millis(flush_ms),
            }
        }
        other => refuse(&format!(
            "--rollout-backend {other}: expected cpu, infer, infer-int8 or gpu-batched"
        )),
    };
    if !matches!(rollout_backend, RolloutBackend::GpuBatched { .. })
        && (argument("--gpu-batch").is_some() || argument("--gpu-flush-ms").is_some())
    {
        refuse("--gpu-batch and --gpu-flush-ms require --rollout-backend gpu-batched");
    }

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();

    let bundle_path = argument("--bundle").unwrap_or_else(|| {
        ti4_mlp::bundle::latest_complete(std::path::Path::new("out/checkpoints/mlp-critic"))
            .unwrap_or_else(|error| refuse(&format!("scanning for a bundle: {error}")))
            .map_or_else(
                || refuse("no complete bundle under out/checkpoints/mlp-critic"),
                |path| path.display().to_string(),
            )
    });
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let mut actor = loaded.actor;
    if diplomacy && !actor.head_names().contains(&"diplomacy") {
        refuse("--diplomacy requires a schema-9/10 bundle with the diplomacy head");
    }
    let critic_mode = loaded.critic_mode;
    // With `--opponent`, one seat per game learns and the rest play a frozen benchmark. Only the
    // learner's decisions enter PPO: frozen trajectories are off-policy for this batch.
    let opponent_actor = argument("--opponent").map(|path| {
        let frozen = ti4_mlp::bundle::read(std::path::Path::new(&path))
            .unwrap_or_else(|error| refuse(&format!("reading {path}: {error}")));
        if frozen.actor.head_names() != actor.head_names() {
            refuse("--opponent has a different head layout than --bundle");
        }
        if diplomacy && !frozen.actor.head_names().contains(&"diplomacy") {
            refuse("--diplomacy requires an --opponent bundle with the diplomacy head");
        }
        (path, frozen.actor)
    });
    let replay_actor = Rc::new(actor.inference_copy());
    let demo_per_update: usize = argument("--demo-per-update").map_or(0, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--demo-per-update expects an unsigned integer"))
    });
    let demo_corpus: BTreeMap<String, Vec<Trajectory>> =
        argument("--demo-corpus").map_or_else(BTreeMap::new, |directory| {
            let mut all = BTreeMap::new();
            for faction in *FACTIONS {
                let path = std::path::Path::new(&directory).join(format!("{faction}.corpus"));
                let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
                    refuse(&format!("reading {}: {error}", path.display()))
                });
                let parsed = read_all(&text).unwrap_or_else(|error| {
                    refuse(&format!("parsing {}: {error}", path.display()))
                });
                all.extend(parsed);
            }
            all
        });
    if demo_per_update > 0 && demo_corpus.is_empty() {
        refuse("--demo-per-update requires --demo-corpus");
    }
    // Filtered self-imitation: imitate this run's own decisions from seats that beat their table.
    let self_imitation: f64 = argument("--self-imitation").map_or(0.0, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--self-imitation expects a number"))
    });
    if !self_imitation.is_finite() || !(0.0..=1.0).contains(&self_imitation) {
        refuse("--self-imitation must be a fraction in (0, 1] (0 or absent disables it)");
    }
    let sil_margin: f64 = argument("--sil-margin").map_or(1.0, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--sil-margin expects a number of victory points"))
    });
    if !sil_margin.is_finite() {
        refuse("--sil-margin must be finite");
    }
    // How hard the wide draw leans toward factions whose recent mean VP trails the table: a
    // faction's weight is 1 + focus x (table mean - its mean), clamped to [0.25, 4]. 0 is off.
    let faction_focus: f64 = argument("--faction-focus").map_or(0.0, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--faction-focus expects a number"))
    });
    if !faction_focus.is_finite() || faction_focus < 0.0 {
        refuse("--faction-focus must be a finite number >= 0");
    }
    // GAE: lambda < 1 replaces each decision's return target with the lambda-return built from the
    // recorded critic values, so the frozen advantage becomes GAE(lambda). 1 is today's G - V.
    let gae_lambda: f64 = argument("--gae-lambda").map_or(1.0, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--gae-lambda expects a number in [0, 1]"))
    });
    if !gae_lambda.is_finite() || !(0.0..=1.0).contains(&gae_lambda) {
        refuse("--gae-lambda must be in [0, 1]");
    }
    let sil_keep: usize = argument("--sil-keep").map_or(4, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--sil-keep expects an unsigned integer"))
    });
    if self_imitation > 0.0 && demo_per_update > 0 {
        refuse("--self-imitation and --demo-per-update are both auxiliary objectives; use one");
    }
    if self_imitation > 0.0 && sil_keep == 0 {
        refuse("--sil-keep must be at least 1 with --self-imitation");
    }
    let mut sil_buffer: std::collections::VecDeque<Vec<ti4_mlp::positive_corpus::AdvantageDemo>> =
        std::collections::VecDeque::new();

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

    let out = argument("--out").unwrap_or_else(|| "out/checkpoints/mlp-ppo".to_owned());
    let cadence = Cadence::parse(&argument("--report-every").unwrap_or_else(|| "100".to_owned()))
        .unwrap_or_else(|error| refuse(&error));
    // The entropy schedule. Coefficients are scaled from 1.0 at the first update down to this
    // multiplier at the last, linearly.
    //
    // A constant bonus is right early, when the policy does not yet know which move is best, and
    // is pure cost once it does: paying to keep probability mass off the chosen move puts a floor
    // under the error rate, and an opening needs about four consecutive correct decisions. The
    // champion trained at a constant bonus ranks 3.5 points better than it samples, which is that
    // floor measured.
    //
    // Defaults to 1.0, which is no schedule at all.
    let entropy_final: f64 = argument("--entropy-final").map_or(1.0, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--entropy-final expects a number"))
    });
    // Where the schedule starts. 1.0 by default; a resume passes the multiplier its predecessor
    // ended on, so the entropy bonus carries on from there instead of jumping back to full strength.
    // Every 50-update resume that restarted at 1.0 lost about 0.13 VP of greedy play before it began
    // to recover (paired greedy evals, 2026-10-02).
    let entropy_start: f64 = argument("--entropy-start").map_or(1.0, |value| {
        value
            .parse()
            .ok()
            .filter(|start: &f64| start.is_finite() && *start >= 0.0)
            .unwrap_or_else(|| refuse("--entropy-start expects a non-negative number"))
    });
    let mut settings = Settings::default();
    // Train only the per-faction rows of the factions outside the six (their readout residual,
    // bias and embedding); everything shared stays where it is (BF-22: populate untrained rows).
    // A restricted optimiser needs weight decay 0, or decay would still shrink the frozen weights.
    let faction_rows_only = std::env::args().any(|a| a == "--faction-rows-only");
    if faction_rows_only {
        settings.weight_decay = 0.0;
    }
    settings.movement_entropy = argument("--movement-entropy").map_or(settings.entropy, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--movement-entropy expects a number"))
    });
    // Exposed for the temperature sweep, which needs it as a control rather than as a tuning knob.
    // Dividing the logits by `T` before the softmax also divides the gradient with respect to those
    // logits by `T`: `d/ds log softmax(s / T)_a = (1/T) (e_a - p)`. So a temperature change is
    // silently also an effective-learning-rate change, 4x at 0.25 and 0.4x at 2.5, and a sweep that
    // does not hold that fixed cannot say which of the two it measured.
    settings.learning_rate = argument("--learning-rate").map_or(settings.learning_rate, |value| {
        value
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite() && *parsed > 0.0)
            .unwrap_or_else(|| refuse("--learning-rate expects a positive number"))
    });
    // Subtracted from the return-to-go of every activation that did nothing. Zero restores the
    // reward exactly as it was, so a run can be compared against the history.
    let waste_penalty: f64 = argument("--waste-penalty").map_or(0.0, |value| {
        value
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite() && *parsed >= 0.0)
            .unwrap_or_else(|| refuse("--waste-penalty expects a non-negative number"))
    });
    // A penalty per faction, in the fixed FACTIONS order, overriding the scalar.
    //
    // One number for all six was hiding a large gain inside an average. At penalty 8 Xxcha went
    // 90.47% to 99.22% while Letnev fell 96.39% to 91.67%, and the table moved 0.65 -- so a single
    // value is being asked to serve two opposite needs. Xxcha starts with four infantry and a
    // single carrier (its two cruisers carry nothing, which is why the bar's composition clause was
    // written to bind on it), so a wasted activation costs it a far larger share of its capacity
    // than it costs a faction with spare hulls.
    if roster != ti4_engine::seating::FactionRoster::InScope
        && argument("--waste-penalties").is_some()
    {
        refuse(
            "--waste-penalties is per faction of the six; with --roster wide or new use --waste-penalty",
        );
    }
    if faction_focus > 0.0 && roster != ti4_engine::seating::FactionRoster::Wide {
        refuse("--faction-focus reweights the wide draw; it needs --roster wide");
    }
    if gae_lambda < 1.0 && critic_mode == ti4_mlp::bundle::CriticMode::BatchMean {
        refuse("--gae-lambda below 1 needs a critic; this bundle is batch-mean");
    }
    let waste_penalties: Vec<f64> = argument("--waste-penalties").map_or_else(
        || vec![waste_penalty; FACTIONS.len()],
        |text| {
            let parsed: Vec<f64> = text
                .split(',')
                .map(|piece| {
                    piece
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite() && *value >= 0.0)
                        .unwrap_or_else(|| refuse("--waste-penalties expects non-negative numbers"))
                })
                .collect();
            if parsed.len() != FACTIONS.len() {
                refuse(&format!(
                    "--waste-penalties needs {} values in the order {}",
                    FACTIONS.len(),
                    FACTIONS.join(",")
                ));
            }
            parsed
        },
    );
    // How many games an update plays: `--seeds-per-update` seeds, each at `--rotations` rotations.
    // The default is §6.3's 16 x 6. Rotations exist so that, against a frozen opponent, each
    // faction is the learner once per seed; in self-play every seat trains in every game, so one
    // rotation per seed buys a fresh map and seating for each game instead (operator, 2026-09-22).
    let seeds_per_update: u64 = argument("--seeds-per-update").map_or(SEEDS_PER_UPDATE, |value| {
        value
            .parse()
            .ok()
            .filter(|seeds| *seeds > 0)
            .unwrap_or_else(|| refuse("--seeds-per-update expects a positive integer"))
    });
    let rotations: usize = argument("--rotations").map_or(FACTIONS.len(), |value| {
        value
            .parse()
            .ok()
            .filter(|rotations| (1..=FACTIONS.len()).contains(rotations))
            .unwrap_or_else(|| refuse(&format!("--rotations expects 1..={}", FACTIONS.len())))
    });
    if rotations != FACTIONS.len() && argument("--opponent").is_some() {
        refuse(
            "--rotations below the faction count leaves factions without a learner game against --opponent",
        );
    }
    let seed_base: u64 = argument("--seed-base").map_or(SEED_BASE, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base expects an unsigned integer"))
    });
    // NEGLECTED_SCORING_PLAN_2026-09-09 Stage 2 screen, isolated worktree only: an explicit,
    // pre-classified seed sequence read one `u64` per line, `updates * SEEDS_PER_UPDATE` lines
    // long. When present it replaces the sequential `seed_base + offset` range below with exactly
    // these seeds, in file order -- everything else (rotation assignment, reward, observation,
    // action set) is untouched, so this changes only which games get played, never the rules
    // available inside one. Absent, behaviour is bit-identical to the unmodified trainer.
    let curriculum_seeds: Option<Vec<u64>> = argument("--curriculum-seeds").map(|path| {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| refuse(&format!("reading {path}: {error}")));
        let seeds: Vec<u64> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| {
                line.parse()
                    .unwrap_or_else(|_| refuse(&format!("{path}: {line:?} is not a u64")))
            })
            .collect();
        let want = updates * seeds_per_update as usize;
        if seeds.len() != want {
            refuse(&format!(
                "{path}: {} seeds, expected updates({updates}) * seeds per update({seeds_per_update}) = {want}",
                seeds.len()
            ));
        }
        eprintln!("  curriculum  {path} ({} seeds)", seeds.len());
        seeds
    });
    // Sampling temperature for self-play. One knob for both acting and the recorded behaviour
    // probabilities -- `MlpBot` has a single `probabilities()` call -- so the PPO importance ratio
    // is computed against the same distribution the action was drawn from. Setting one and not the
    // other would silently corrupt every ratio in the batch.
    let temperature: f64 = argument("--temperature").map_or(1.0, |value| {
        value
            .parse()
            .ok()
            .filter(|t: &f64| t.is_finite() && *t > 0.0)
            .unwrap_or_else(|| refuse("--temperature must be a positive number"))
    });

    let stage = match argument("--stage").as_deref() {
        None | Some("2") => ti4_training::reward::Stage::Two,
        Some("1") => ti4_training::reward::Stage::One,
        Some(other) => refuse(&format!("--stage {other}: expected 1 or 2")),
    };
    let rounds = argument("--rounds").map_or_else(
        || rounds_for(stage),
        |value| {
            value
                .parse()
                .unwrap_or_else(|_| refuse("--rounds expects a positive integer"))
        },
    );
    if rounds == 0 {
        refuse("--rounds 0 plays no game");
    }
    // The Stage-1 potential's coefficients, overridable so an experiment on them is recorded in
    // the command line and the log rather than as an edit to a default nobody can see afterwards.
    //
    // They are pre-registered values: changing one changes what the policy is being paid for, and
    // the run that results is not comparable to runs at the registered settings.
    let weight = |name: &str, registered: f64| -> f64 {
        argument(name).map_or(registered, |value| {
            value
                .parse()
                .unwrap_or_else(|_| refuse(&format!("{name} expects a number")))
        })
    };
    let mut reward = ti4_training::reward::Reward::for_stage(stage);
    reward.expansion_weight = weight("--expansion-weight", reward.expansion_weight);
    reward.unit_weight = weight("--unit-weight", reward.unit_weight);
    reward.conjunctive_weight = weight("--conjunctive-weight", reward.conjunctive_weight);
    reward.clear_bonus = weight("--clear-bonus", reward.clear_bonus);
    // The Stage-2 coefficients that price the opening. Same pre-registration argument as above:
    // a run at other values is a different experiment and the command line has to say so.
    //
    // `r1_bonus` reaches only round-one decisions, by construction -- it is credited at the last
    // round-one slot precisely so a round-three decision is not paid for something it could not
    // affect. `clearance_weight` is the one that reaches every decision, because it is credited at
    // the final slot and every return is a suffix sum. Raising the first alone sharpens round one
    // and leaves the rest of the game indifferent to whether the opening held.
    // What a point is worth against what standing next to one is worth. `validate` enforces
    // objective_weight < vp_weight and secret_weight < vp_weight, so raising the price of a point
    // is the safe direction and lowering it can be refused.
    reward.vp_weight = weight("--vp-weight", reward.vp_weight);
    reward.objective_weight = weight("--objective-weight", reward.objective_weight);
    reward.secret_weight = weight("--secret-weight", reward.secret_weight);
    reward.r1_bonus = weight("--r1-bonus", reward.r1_bonus);
    reward.r1_shaping = weight("--r1-shaping", reward.r1_shaping);
    reward.r1_expansion_weight = weight("--r1-expansion-weight", reward.r1_expansion_weight);
    reward.sftt_weight = weight("--sftt-weight", reward.sftt_weight);
    reward.clearance_weight = weight("--clearance-weight", reward.clearance_weight);
    reward.high_vp_bonus = weight("--high-vp-bonus", reward.high_vp_bonus);
    // The three shapings the profile trainer already exposed. They live on the shared `Reward` and
    // default to zero, so until now an MLP run silently trained the reference reward no matter what
    // the surrounding experiment thought it was configuring.
    reward.fleet_weight = weight("--fleet-weight", reward.fleet_weight);
    // Power-projection shaping. Measuring it costs a movement search per recorded decision, so the
    // progress measure only computes it when a run actually rewards it.
    reward.projection_weight = weight("--projection-weight", reward.projection_weight);
    // Experiment: trade goods every seat gains as each status phase begins (engine switch).
    if let Some(income) = argument("--status-income-tg") {
        let income: i32 = income
            .parse()
            .unwrap_or_else(|_| refuse("--status-income-tg expects a whole number"));
        ti4_engine::game::STATUS_INCOME_TRADE_GOODS
            .store(income, std::sync::atomic::Ordering::Relaxed);
        println!(
            "  experiment  every seat gains {income} trade goods at the start of each status phase"
        );
    }
    if reward.projection_weight != 0.0 {
        ti4_policy::progress::MEASURE_PROJECTION.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    reward.tech_weight = weight("--tech-weight", reward.tech_weight);
    reward.home_loss_weight = weight("--home-loss-weight", reward.home_loss_weight);
    reward.keep_terminal_projection = weight("--keep-terminal-projection", 0.0) != 0.0;
    reward.projection_rank_weight =
        weight("--projection-rank-weight", reward.projection_rank_weight);
    if reward.projection_rank_weight != 0.0 {
        ti4_policy::progress::MEASURE_PROJECTION.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    reward.negotiation_step_penalty = weight(
        "--negotiation-step-penalty",
        reward.negotiation_step_penalty,
    );
    // Nonzero turns it on, so the PowerShell launcher's name/value pairs can carry it.
    reward.zero_terminal_objectives = weight("--zero-terminal-objectives", 0.0) != 0.0;
    reward.strategy_diversity_weight = weight(
        "--strategy-diversity-weight",
        reward.strategy_diversity_weight,
    );
    // Command tokens, trade goods and Styx. Off by default like every term above: a run that
    // prices them says so on its command line.
    reward.fleet_hoard_penalty = weight("--fleet-hoard-penalty", reward.fleet_hoard_penalty);
    reward.zero_fleet_penalty = weight("--zero-fleet-penalty", reward.zero_fleet_penalty);
    reward.trade_goods_hoard_weight = weight(
        "--trade-goods-hoard-weight",
        reward.trade_goods_hoard_weight,
    );
    reward.styx_bonus = weight("--styx-bonus", reward.styx_bonus);
    reward.overpay_penalty = weight("--overpay-penalty", reward.overpay_penalty);
    reward.unused_resources_weight = weight(
        "--unused-resources-weight",
        reward.unused_resources_weight,
    );
    reward.build_weight = weight("--build-weight", reward.build_weight);
    // The first entry into the Fracture. `Reward` now defaults it to zero and says an experiment
    // must select it explicitly, but no flag existed, so the term could not be selected at all:
    // runs built before that change had it silently on at 0.1, and runs built after it silently
    // off.
    reward.fracture_entry_bonus = weight("--fracture-entry-bonus", reward.fracture_entry_bonus);
    // Each Fracture planet held at the end of the horizon; Styx also earns `--styx-bonus`.
    reward.fracture_planet_bonus = weight("--fracture-planet-bonus", reward.fracture_planet_bonus);
    // `returns` gates both terminal bonuses on `> 0.0`, so a negative value here would be read as
    // "off" and the run would silently not be the experiment its command line describes.
    for (name, value) in [
        ("--fracture-entry-bonus", reward.fracture_entry_bonus),
        ("--fracture-planet-bonus", reward.fracture_planet_bonus),
        ("--fleet-hoard-penalty", reward.fleet_hoard_penalty),
        ("--zero-fleet-penalty", reward.zero_fleet_penalty),
        (
            "--trade-goods-hoard-weight",
            reward.trade_goods_hoard_weight,
        ),
        ("--styx-bonus", reward.styx_bonus),
        ("--overpay-penalty", reward.overpay_penalty),
        ("--unused-resources-weight", reward.unused_resources_weight),
        ("--build-weight", reward.build_weight),
        ("--clearance-weight", reward.clearance_weight),
        ("--high-vp-bonus", reward.high_vp_bonus),
        // `returns` gates the monoculture penalty on `> 0.0` as well, so the same trap applies:
        // a negative weight would read as off rather than as a reversed penalty.
        (
            "--strategy-diversity-weight",
            reward.strategy_diversity_weight,
        ),
    ] {
        if value < 0.0 {
            refuse(&format!(
                "{name} {value} is negative; the reward reads any value at or below zero as off"
            ));
        }
    }
    reward
        .validate()
        .unwrap_or_else(|error| refuse(&format!("the reward is not self-consistent: {error}")));

    if opponent_actor.is_some() && matches!(rollout_backend, RolloutBackend::GpuBatched { .. }) {
        refuse("--opponent needs a CPU rollout backend: the GPU service owns a single actor");
    }
    // The model of ti4-infer has the value readout of a shared critic only: a separate critic has
    // its own trunk, which it does not carry.
    if matches!(rollout_backend, RolloutBackend::Infer(_)) && critic_mode == CriticMode::Separate {
        refuse(
            "--rollout-backend infer needs a shared critic or batch_mean, not a separate critic",
        );
    }
    let infer_backend = match rollout_backend {
        RolloutBackend::Infer(backend) => backend,
        _ => ti4_infer::Backend::fastest(),
    };

    println!("M10-034 PPO update");
    println!("  bundle      {bundle_path}");
    if let Some((path, _)) = &opponent_actor {
        println!("  opponent    {path} (frozen, every seat but the learner's)");
        println!(
            "  learner     one rotating seat per game; only its decisions enter PPO, benchmark at temperature {OPPONENT_TEMPERATURE}"
        );
    }
    println!("  seeds       {seed_base}.. ({seeds_per_update} per update)");
    println!("  roster      {roster:?}");
    if faction_focus > 0.0 {
        println!("  focus       {faction_focus} (wide draw weighted toward trailing factions)");
    }
    if gae_lambda < 1.0 {
        println!("  gae         lambda {gae_lambda}");
    }
    println!("  critic mode {critic_mode:?}");
    println!(
        "  trunk       width {} | residual blocks {}",
        actor.width(),
        actor.residual_blocks()
    );
    if matches!(stage, ti4_training::reward::Stage::One) {
        println!(
            "  potential   planets+systems {} | capacity+infantry {} | conjunctive {} | clear bonus {}",
            reward.expansion_weight,
            reward.unit_weight,
            reward.conjunctive_weight,
            reward.clear_bonus
        );
    }
    if matches!(stage, ti4_training::reward::Stage::Two) {
        println!(
            "  opening     r1 bonus {} | r1 shaping {} | r1 expansion {} | clearance {} | high-VP {}",
            reward.r1_bonus,
            reward.r1_shaping,
            reward.r1_expansion_weight,
            reward.clearance_weight,
            reward.high_vp_bonus
        );
        println!(
            "  points      vp {} (Support for the Throne x{}) | objective {} | secret {} | zero at horizon {} | home lost {}",
            reward.vp_weight,
            reward.sftt_weight,
            reward.objective_weight,
            reward.secret_weight,
            reward.zero_terminal_objectives,
            reward.home_loss_weight
        );
        println!(
            "  shaping     fleet {} | projection {} (kept at horizon {}, rank per round {}) | negotiation step {} | tech {} | strategy diversity {}",
            reward.fleet_weight,
            reward.projection_weight,
            reward.keep_terminal_projection,
            reward.projection_rank_weight,
            -reward.negotiation_step_penalty,
            reward.tech_weight,
            reward.strategy_diversity_weight
        );
        println!(
            "  holdings    fleet pool >= {} at end {} | empty fleet pool {} | trade goods > {} {} per excess good per round | Styx at end {} | first Fracture entry {} | per Fracture planet at end {}",
            ti4_training::reward::FLEET_HOARD_AT,
            reward.fleet_hoard_penalty,
            reward.zero_fleet_penalty,
            ti4_training::reward::TRADE_GOODS_FREE,
            reward.trade_goods_hoard_weight,
            reward.styx_bonus,
            reward.fracture_entry_bonus,
            reward.fracture_planet_bonus
        );
        println!(
            "  production  overpay penalty {} per resource lost | unused ready planets {} per resource per round | build {} per resource built (all units at cost, upgraded ships x1.3)",
            reward.overpay_penalty, reward.unused_resources_weight, reward.build_weight
        );
    }
    println!(
        "  reward      {stage:?} ({}) | {rounds} round(s) per game",
        match stage {
            ti4_training::reward::Stage::One => "the opening bar",
            ti4_training::reward::Stage::Two => "victory points",
        }
    );
    println!("  optimiser   {device:?}");
    match &rollout_backend {
        RolloutBackend::Cpu => println!("  rollout     CPU inference on Rayon workers"),
        RolloutBackend::Infer(_) => println!(
            "  rollout     ti4-infer ({}) on Rayon workers, one model per update",
            infer_backend.name()
        ),
        RolloutBackend::GpuBatched { batch_size, flush } => println!(
            "  rollout     experimental CUDA service, batch {batch_size}, partial flush {:.1} ms",
            flush.as_secs_f64() * 1_000.0
        ),
    }
    println!(
        "  ppo         clip {} | {} epochs | minibatch {} | value {} | entropy {}/{} (movement {}), x{entropy_start} at the start, x{entropy_final} by the end",
        settings.clip_epsilon,
        settings.epochs,
        settings.minibatch,
        settings.value_coefficient,
        settings.entropy,
        settings.strategy_entropy,
        settings.movement_entropy
    );
    if demo_per_update > 0 {
        println!(
            "  auxiliary   {demo_per_update} clean trajectories/update, advantage weighted, 10% gradient cap"
        );
    }
    if self_imitation > 0.0 {
        println!(
            "  self-imit   seats >= table mean + {sil_margin} VP, advantage weighted, faction balanced, last {sil_keep} updates, {:.0}% gradient cap",
            self_imitation * 100.0
        );
    }
    // Recorded in the header because run-030's log did not name it, and the temperature is the
    // whole difference between that run and the one before it. A log that cannot say what it was
    // run at cannot be compared against another.
    println!("  sampling    temperature {temperature} (acting and recorded behaviour)");
    println!(
        "  diplomacy   {}",
        if diplomacy { "enabled" } else { "disabled" }
    );
    println!("  adam        learning rate {}", settings.learning_rate);
    println!(
        "  waste       penalty per faction {}",
        FACTIONS
            .iter()
            .zip(&waste_penalties)
            .map(|(faction, penalty)| format!("{faction} {penalty}"))
            .collect::<Vec<_>>()
            .join("  ")
    );
    println!("  update      {seeds_per_update} seeds x {rotations} rotations\n",);

    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let replay_factions: Vec<FactionId> = FACTIONS
        .iter()
        .map(|faction| FactionId::new(*faction))
        .collect();

    // F-M10-034-D3: **once**, for the whole run. Constructing this inside the loop discarded the
    // moments and the step counter on every update after the first, which turns Adam into a
    // sequence of first steps — and Adam's bias correction is a function of `t`, so the first step
    // is the one that behaves least like Adam. Nothing in the telemetry would have shown it.
    actor = actor.to_device(device);
    let mut optimizer = ti4_mlp::ppo::Adam::new(&mut actor, critic_mode, settings)
        .unwrap_or_else(|error| refuse(&format!("optimiser: {error}")));
    // Optimiser state sits beside a checkpoint as `<checkpoint>.adam`; older ones have none.
    let adam_file = format!("{}.adam", bundle_path.trim_end_matches(['/', '\\']));
    match optimizer.load_state(std::path::Path::new(&adam_file)) {
        Ok(()) => println!("  optimiser   resumed from {adam_file} at step {}", optimizer.steps()),
        Err(error) => println!("  optimiser   fresh ({error})"),
    }
    if faction_rows_only {
        if critic_mode == ti4_mlp::bundle::CriticMode::Separate {
            refuse("--faction-rows-only covers the shared and batch-mean critics");
        }
        let rows: Vec<ti4_mlp::FactionRow> = ti4_mlp::FACTION_ROSTER
            .iter()
            .filter(|alias| !FACTIONS.contains(alias))
            .map(|alias| ti4_mlp::FactionRow::of(alias).expect("a roster row"))
            .collect();
        let masks =
            actor.faction_rows_masks(&rows, critic_mode == ti4_mlp::bundle::CriticMode::Shared);
        optimizer
            .restrict(masks)
            .unwrap_or_else(|error| refuse(&format!("restricting the optimiser: {error}")));
        println!(
            "  training    faction rows only: {} factions outside the six; trunk, input table, shared readout and value frozen",
            rows.len()
        );
    }

    // --collect-only: play and record, never optimise or checkpoint (gathering data from a fixed model).
    let collect_only = std::env::args().any(|a| a == "--collect-only");
    let require_faction = argument("--require-faction");
    if let Some(faction) = &require_faction {
        ti4_mlp::FactionRow::of(faction).unwrap_or_else(|error| refuse(&error.to_string()));
        if faction_focus > 0.0 {
            refuse("--require-faction filters the unweighted draw; use --faction-focus 0");
        }
    }
    let recorder = argument("--record-best").map(|directory| {
        let bars =
            argument("--record-vp").unwrap_or_else(|| refuse("--record-best needs --record-vp"));
        let recorder = Recorder::parse(directory, &bars).unwrap_or_else(|error| refuse(&error));
        println!(
            "  record      seats at >= {} VP ({}) -> {}",
            recorder.default_bar,
            recorder
                .bars
                .iter()
                .map(|(faction, points)| format!("{faction} >= {points}"))
                .collect::<Vec<_>>()
                .join(", "),
            recorder.directory.display()
        );
        recorder
    });

    // --separate-models: one network and optimiser per faction of the six, each from the same
    // bundle and trained only on its own seats' decisions. Otherwise one shared model.
    let separate_models = std::env::args().any(|a| a == "--separate-models");
    if separate_models {
        if roster != ti4_engine::seating::FactionRoster::InScope {
            refuse("--separate-models needs the six factions (no --roster wide)");
        }
        if !matches!(rollout_backend, RolloutBackend::Infer(_)) {
            refuse("--separate-models needs --rollout-backend infer");
        }
        if opponent_actor.is_some()
            || demo_per_update > 0
            || faction_rows_only
            || check_likelihood
            || hash_games
            || capture_path.is_some()
        {
            refuse(
                "--separate-models does not combine with --opponent, demos, \
                 --faction-rows-only, --check-likelihood, --hash-games or --capture-batch",
            );
        }
    }
    let mut models: Vec<(ti4_mlp::Actor, ti4_mlp::ppo::Adam)> = vec![(actor, optimizer)];
    if separate_models {
        // Each faction starts from its own model when --bundle is a checkpoint of a separate-models
        // run that trained it, else from --bundle itself (a faction new to the set).
        let previous = ti4_mlp::bundle::separate_model_bundles(std::path::Path::new(&bundle_path))
            .unwrap_or_else(|error| refuse(&error))
            .unwrap_or_default();
        models.clear();
        for faction in FACTIONS.iter() {
            let start = previous
                .get(*faction)
                .map_or_else(|| bundle_path.clone(), |path| path.display().to_string());
            println!("  model       {faction} starts from {start}");
            let mut actor = ti4_mlp::bundle::read(std::path::Path::new(&start))
                .unwrap_or_else(|error| refuse(&format!("reading {start}: {error}")))
                .actor
                .to_device(device);
            let adam = ti4_mlp::ppo::Adam::new(&mut actor, critic_mode, settings)
                .unwrap_or_else(|error| refuse(&format!("optimiser: {error}")));
            models.push((actor, adam));
        }
        println!(
            "  models      separate: one per faction ({})",
            FACTIONS.join(", ")
        );
        if self_imitation > 0.0 {
            println!(
                "  cross-imit  every model imitates the self-imitation picks of all six factions as its own"
            );
        }
    }

    // F-M10-034-D6. Loss telemetry is not evidence that an update happened: a broken optimiser
    // still produces a full, plausible table of losses, and the vacuous tests this milestone kept
    // producing failed in exactly that way. Parameters and Adam state are fingerprinted before and
    // after, and the run refuses if either stayed put.
    let fingerprints = |models: &[(ti4_mlp::Actor, ti4_mlp::ppo::Adam)]| {
        models
            .iter()
            .map(|(actor, optimizer)| {
                (
                    ti4_mlp::ppo::parameter_fingerprint(actor, critic_mode).unwrap_or_else(
                        |error| refuse(&format!("fingerprinting parameters: {error}")),
                    ),
                    optimizer
                        .state_fingerprint()
                        .unwrap_or_else(|error| refuse(&format!("fingerprinting Adam: {error}"))),
                )
            })
            .collect::<Vec<_>>()
    };
    let before = fingerprints(&models);

    // §4.4: weights are stored on CPU, so a checkpoint from a CUDA run loads on a CPU-only machine.
    // Read once here rather than per publish: it is 1.1 MB of JSON and does not change.
    let slots_text = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
    // The frozen benchmark on ti4-infer, packed once: its weights never change.
    let frozen_infer = match (&rollout_backend, &opponent_actor) {
        (RolloutBackend::Infer(_), Some((path, frozen))) => Some(Arc::new(
            ti4_mlp::infer::InferModel::from_actor(frozen, &slots_text, infer_backend)
                .unwrap_or_else(|error| refuse(&format!("packing {path} for ti4-infer: {error}"))),
        )),
        _ => None,
    };

    // Stage-one clearance and victory points accumulate across a reporting window and are compared
    // against the window before it. Per update the numbers are noise -- 96 games, six seats -- but a
    // hundred updates is 9,600 seat-games, which is enough to read a trend from.
    let mut window: BTreeMap<String, FactionTally> = BTreeMap::new();
    let mut previous: Option<BTreeMap<String, FactionTally>> = None;
    let mut reported_at = 0usize;
    // --faction-focus: smoothed per-faction mean VP, and the wide roster it weights.
    let mut smoothed_vp: BTreeMap<String, f64> = BTreeMap::new();
    let wide_entries = ti4_engine::seating::wide_roster(content, DEFAULT);

    // The current weights packed once per model for ti4-infer, shared by every worker.
    let pack_models = |models: &[(ti4_mlp::Actor, ti4_mlp::ppo::Adam)]| -> Vec<Arc<ti4_mlp::infer::InferModel>> {
        models
            .iter()
            .map(|(actor, _)| {
                Arc::new(
                    ti4_mlp::infer::InferModel::from_actor(actor, &slots_text, infer_backend)
                        .unwrap_or_else(|error| {
                            refuse(&format!("packing the actor for ti4-infer: {error}"))
                        }),
                )
            })
            .collect()
    };

    // ---- --async-batch-games: rollouts and optimisation run concurrently ----
    //
    // `--rollout-workers` threads (default: one per core) play games back to back; each starts its
    // next game the moment the last ends, with no barrier at a batch boundary. Every
    // `--async-batch-games` finished games are cut into a batch for the optimiser, which trains one
    // update per batch and, every `--async-publish-games` games it has processed, publishes new
    // weights; workers pick them up at their next game start, never mid-game. PPO already records
    // behaviour log-probs per step, so slightly stale batches are handled by the importance ratio
    // and clipping. Staleness is bounded: a worker may start game j only while
    // j < (batches taken + queue depth + 1) x batch, so when the optimiser falls behind the queue
    // fills and workers block (their blocked time is reported, never games dropped).
    let async_batch = flag_count("--async-batch-games");
    let async_rt: Option<Arc<AsyncShared>> = async_batch.map(|batch_games| {
        if !matches!(rollout_backend, RolloutBackend::Infer(_)) {
            refuse("--async-batch-games needs --rollout-backend infer");
        }
        if collect_only || require_faction.is_some() || curriculum_seeds.is_some() {
            refuse(
                "--async-batch-games does not combine with --collect-only, --require-faction or --curriculum-seeds",
            );
        }
        if batch_games != seeds_per_update as usize * rotations {
            refuse("--async-batch-games must equal --seeds-per-update x --rotations");
        }
        let publish = flag_count("--async-publish-games").unwrap_or(2 * batch_games);
        if publish % batch_games != 0 {
            refuse("--async-publish-games must be a multiple of --async-batch-games");
        }
        Arc::new(AsyncShared {
            state: std::sync::Mutex::new(AsyncState::default()),
            changed: std::sync::Condvar::new(),
            weights: std::sync::RwLock::new((0, pack_models(&models))),
            focus: std::sync::RwLock::new((faction_focus > 0.0).then(|| {
                Arc::new(self::focus_weights(
                    &wide_entries,
                    &BTreeMap::new(),
                    faction_focus,
                ))
            })),
            batch_games,
            queue_batches: flag_count("--async-queue-batches").unwrap_or(2),
        })
    });
    if async_rt.is_none()
        && (argument("--async-publish-games").is_some()
            || argument("--async-queue-batches").is_some()
            || argument("--rollout-workers").is_some())
    {
        refuse("--async-publish-games, --async-queue-batches and --rollout-workers need --async-batch-games");
    }
    let async_publish_updates = async_rt.as_ref().map_or(0, |shared| {
        flag_count("--async-publish-games").unwrap_or(2 * shared.batch_games) / shared.batch_games
    });
    let async_workers = flag_count("--rollout-workers")
        .unwrap_or_else(|| rayon::current_num_threads().max(1));
    if let Some(shared) = &async_rt {
        println!(
            "  async       {async_workers} rollout workers, batch {} games, publish every {} games, queue {} batches; optimiser on {}",
            shared.batch_games,
            async_publish_updates * shared.batch_games,
            shared.queue_batches,
            if matches!(models[0].0.device(), ti4_tensor::Device::Cuda(_)) { "the GPU (1 CPU thread)" } else { "the CPU" },
        );
    }
    let mut async_last_take = Instant::now();

    std::thread::scope(|scope| {
    if let Some(shared) = &async_rt {
        for _ in 0..async_workers {
            scope.spawn(|| {
                loop {
                    // Claim the next game, waiting while the queue is full.
                    let waiting = Instant::now();
                    let job = {
                        let mut state = shared.state.lock().expect("async state");
                        loop {
                            if state.stop || state.failure.is_some() {
                                return;
                            }
                            if state.next_job
                                < (state.consumed + shared.queue_batches + 1) * shared.batch_games
                            {
                                break;
                            }
                            if !state.warned {
                                state.warned = true;
                                eprintln!(
                                    "ROLLOUT QUEUE FULL: workers are waiting for the optimiser (reported per update as blocked)"
                                );
                            }
                            state = shared.changed.wait(state).expect("async state");
                        }
                        let job = state.next_job;
                        state.next_job += 1;
                        state.blocked += waiting.elapsed();
                        job
                    };
                    let (version, models) = {
                        let weights = shared.weights.read().expect("async weights");
                        (weights.0, weights.1.clone())
                    };
                    let focus = shared.focus.read().expect("async focus").clone();
                    let local: Vec<WorkerScopedInference> =
                        models.into_iter().map(WorkerScopedInference::Infer).collect();
                    let frozen = frozen_infer
                        .as_ref()
                        .map(|model| FrozenInference::Infer(Arc::clone(model)));
                    let (seed, rotation) = (seed_base + (job / rotations) as u64, job % rotations);
                    let played = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        play_one(
                            &local,
                            frozen.as_ref(),
                            rotation,
                            content,
                            &players,
                            &vocabulary,
                            &pool,
                            &reward,
                            critic_mode,
                            rounds,
                            seed,
                            rotation,
                            temperature,
                            &waste_penalties,
                            waste_penalty,
                            roster,
                            diplomacy,
                            hash_games,
                            sil_margin,
                            focus.as_deref(),
                            gae_lambda,
                            recorder.as_ref(),
                            job / shared.batch_games,
                        )
                    }));
                    let mut state = shared.state.lock().expect("async state");
                    match played {
                        Ok(result) => {
                            state.finished += 1;
                            state.pending.push((job, version, result));
                            if state.pending.len() == shared.batch_games {
                                let mut batch = std::mem::take(&mut state.pending);
                                batch.sort_by_key(|game| game.0);
                                state.ready.push_back(batch);
                            }
                        }
                        Err(panic) => {
                            let what = panic
                                .downcast_ref::<String>()
                                .cloned()
                                .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_owned()))
                                .unwrap_or_default();
                            state.failure = Some(format!("rollout worker panicked on game {job}: {what}"));
                        }
                    }
                    shared.changed.notify_all();
                }
            });
        }
    }

    for update in 0..updates {
        let focus_weights = (faction_focus > 0.0)
            .then(|| focus_weights(&wide_entries, &smoothed_vp, faction_focus));
        let mut update_vp: BTreeMap<String, (i64, i64)> = BTreeMap::new();
        // ---- rollout ----
        //
        // The optimiser's leaves never act. Every backend takes a frozen inference copy before
        // workers start, then the service is stopped before Adam can mutate the training actor.
        let update_started = Instant::now();
        let mut steps: Vec<Step> = Vec::new();
        let mut seated_decisions = 0usize;
        let mut games = 0usize;
        let mut wasted_activations = 0usize;
        let mut tactical_actions = 0usize;
        let mut game_lines = Vec::new();
        let mut async_report = None;
        let (
            inference_copy_time,
            rolled,
            rollout_lock,
            chunk_copy_time,
            harvest_time,
            merge_started,
            workers_json,
            by_job,
            jobs_len,
            gpu_stats,
            service_shutdown_time,
        ) = if let Some(shared) = &async_rt {
            // Wait for the next batch: this wait is the optimiser's idle time.
            let rolled = Instant::now();
            let (batch, queued, blocked, finished) = {
                let mut state = shared.state.lock().expect("async state");
                let batch = loop {
                    if let Some(failure) = state.failure.clone() {
                        drop(state);
                        refuse(&failure);
                    }
                    if let Some(batch) = state.ready.pop_front() {
                        state.consumed += 1;
                        break batch;
                    }
                    state = shared.changed.wait(state).expect("async state");
                };
                let taken = (
                    batch,
                    state.ready.len(),
                    std::mem::take(&mut state.blocked),
                    std::mem::take(&mut state.finished),
                );
                shared.changed.notify_all();
                taken
            };
            let idle = rolled.elapsed();
            let interval = async_last_take.elapsed();
            async_last_take = Instant::now();
            let lags = batch.iter().map(|game| update.saturating_sub(game.1)).collect::<Vec<_>>();
            async_report = Some((
                idle,
                interval,
                blocked,
                finished,
                queued,
                lags.iter().copied().max().unwrap_or(0),
                lags.iter().sum::<usize>() as f64 / lags.len().max(1) as f64,
            ));
            let by_job: Vec<Option<Result<Played, String>>> =
                batch.into_iter().map(|game| Some(game.2)).collect();
            let merge_started = Instant::now();
            let jobs_len = by_job.len();
            (
                Duration::ZERO,
                rolled,
                None,
                Duration::ZERO,
                Duration::ZERO,
                merge_started,
                Vec::new(),
                by_job,
                jobs_len,
                None,
                Duration::ZERO,
            )
        } else {
        let mut infer_model = None;
        let (cpu_inference, mut gpu_service) = match &rollout_backend {
            RolloutBackend::Cpu => (
                Some(
                    models[0]
                        .0
                        .inference_copy()
                        .to_device(ti4_tensor::Device::Cpu),
                ),
                None,
            ),
            RolloutBackend::Infer(_) => {
                // The weights of this update, packed once per model and shared by every worker.
                infer_model = Some(
                    models
                        .iter()
                        .map(|(actor, _)| {
                            Arc::new(
                                ti4_mlp::infer::InferModel::from_actor(
                                    actor,
                                    &slots_text,
                                    infer_backend,
                                )
                                .unwrap_or_else(|error| {
                                    refuse(&format!("packing the actor for ti4-infer: {error}"))
                                }),
                            )
                        })
                        .collect::<Vec<_>>(),
                );
                (None, None)
            }
            RolloutBackend::GpuBatched { batch_size, flush } => (
                None,
                Some(
                    ti4_mlp::gpu_batch::GpuInferenceService::spawn(
                        models[0]
                            .0
                            .inference_copy()
                            .to_device(ti4_tensor::Device::Cuda(0)),
                        *batch_size,
                        *flush,
                    )
                    .unwrap_or_else(|error| {
                        refuse(&format!("starting GPU inference service: {error}"))
                    }),
                ),
            ),
        };
        let inference_copy_time = update_started.elapsed();
        let rollout_lock = phase_lock("rollout");
        let rolled = Instant::now();

        // §6.3's unit of work as a job list. Self-play is embarrassingly parallel — every game is
        // an independent seed — and once the optimizer stopped being launch-bound it was 87% of an
        // update's wall time.
        //
        // Work is split into one chunk per rayon thread rather than one job per thread, because
        // each chunk carries an owned `Actor` copy: `tch::Tensor` is `Send` but not `Sync`, so the
        // actor cannot be borrowed across threads. Per-job copies would allocate 96 actors instead
        // of one per core.
        let update_seeds: Vec<u64> = curriculum_seeds.as_ref().map_or_else(
            || {
                let base = seed_base + seeds_per_update * update as u64;
                (base..base + seeds_per_update).collect()
            },
            |all| {
                let start = update * seeds_per_update as usize;
                all[start..start + seeds_per_update as usize].to_vec()
            },
        );
        // --require-faction: only the seeds whose faction draw seats that faction.
        let update_seeds: Vec<u64> = match &require_faction {
            None => update_seeds,
            Some(wanted) => update_seeds
                .into_iter()
                .filter(|seed| {
                    ti4_training::rollout::game_factions(
                        content, roster, &*FACTIONS, &players, DEFAULT, *seed,
                    )
                    .is_ok_and(|drawn| drawn.iter().any(|faction| faction.as_str() == wanted))
                })
                .collect(),
        };
        if update_seeds.is_empty() {
            println!("  update {update:>3}  no seed seats the required faction");
            continue;
        }
        let jobs: Vec<(u64, usize)> = update_seeds
            .into_iter()
            .flat_map(|seed| (0..rotations).map(move |rotation| (seed, rotation)))
            .collect();
        let workers = rayon::current_num_threads().max(1).min(jobs.len());
        let locals: Vec<(Vec<WorkerInference>, Option<ti4_mlp::Actor>)> = match &rollout_backend {
            RolloutBackend::Cpu => {
                let inference = cpu_inference
                    .as_ref()
                    .unwrap_or_else(|| refuse("CPU rollout has no inference snapshot"));
                (0..workers)
                    .map(|_| {
                        (
                            vec![WorkerInference::Cpu(inference.inference_copy())],
                            opponent_actor.as_ref().map(|(_, frozen)| {
                                frozen.inference_copy().to_device(ti4_tensor::Device::Cpu)
                            }),
                        )
                    })
                    .collect()
            }
            RolloutBackend::Infer(_) => {
                let packed = infer_model
                    .as_ref()
                    .unwrap_or_else(|| refuse("ti4-infer rollout has no model"));
                (0..workers)
                    .map(|_| {
                        (
                            packed
                                .iter()
                                .map(|model| WorkerInference::Infer(Arc::clone(model)))
                                .collect(),
                            None,
                        )
                    })
                    .collect()
            }
            RolloutBackend::GpuBatched { .. } => {
                let client = gpu_service
                    .as_ref()
                    .map(ti4_mlp::gpu_batch::GpuInferenceService::client)
                    .unwrap_or_else(|| refuse("GPU rollout has no inference service"));
                (0..workers)
                    .map(|_| (vec![WorkerInference::Gpu(client.clone())], None))
                    .collect()
            }
        };
        let chunk_copy_time = rolled.elapsed();
        let harvest_started = Instant::now();

        // Games are claimed one at a time from a shared cursor, so a worker that drew short games
        // takes more of them instead of idling while the slowest fixed share finishes. With fixed
        // shares of three the slowest of 32 workers measured 1.21x the mean, about 2.3 s of every
        // update spent waiting (plans/TRAINING_PERFORMANCE_HANDOFF_2026-09-11.md, A1).
        //
        // Each result keeps its job index and the batch is reassembled in job order below, so the
        // batch a given update sees still does not depend on which worker finished first.
        // Determinism here is not decoration: §6.3's shuffle is seeded, and a batch assembled in
        // scheduling order would make every downstream fingerprint irreproducible.
        let cursor = std::sync::atomic::AtomicUsize::new(0);
        type WorkerResult = (
            Vec<(usize, Result<Played, String>)>,
            ti4_mlp::perf::StageTotals,
            f64,
        );
        let harvest: Vec<WorkerResult> = locals
            .into_par_iter()
            .map(|(local, frozen)| {
                let worker_started = Instant::now();
                let _ = ti4_mlp::perf::take_stages();
                // One CPU actor or one GPU-service client per worker, shared by every game it plays.
                let local: Vec<WorkerScopedInference> = local
                    .into_iter()
                    .map(|local| match local {
                        WorkerInference::Cpu(actor) => WorkerScopedInference::Cpu(Rc::new(actor)),
                        WorkerInference::Infer(model) => WorkerScopedInference::Infer(model),
                        WorkerInference::Gpu(client) => WorkerScopedInference::Gpu(client),
                    })
                    .collect();
                let frozen = match (frozen, &frozen_infer) {
                    (Some(actor), _) => Some(FrozenInference::Cpu(Rc::new(actor))),
                    (None, Some(model)) => Some(FrozenInference::Infer(Arc::clone(model))),
                    (None, None) => None,
                };
                let mut played = Vec::new();
                loop {
                    let job = cursor.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((seed, rotation)) = jobs.get(job) else {
                        break;
                    };
                    played.push((
                        job,
                        play_one(
                            &local,
                            frozen.as_ref(),
                            // One faction learns per rotation, so a seed block trains each faction
                            // exactly once. Choosing by seat index instead left the learner's
                            // faction to the seeded permutation, which skewed a smoke update to 22
                            // hacan games against 10 letnev.
                            *rotation,
                            content,
                            &players,
                            &vocabulary,
                            &pool,
                            &reward,
                            critic_mode,
                            rounds,
                            *seed,
                            *rotation,
                            temperature,
                            &waste_penalties,
                            waste_penalty,
                            roster,
                            diplomacy,
                            hash_games,
                            sil_margin,
                            focus_weights.as_ref(),
                            gae_lambda,
                            recorder.as_ref(),
                            update,
                        ),
                    ));
                }
                (
                    played,
                    ti4_mlp::perf::take_stages(),
                    worker_started.elapsed().as_secs_f64(),
                )
            })
            .collect();

        let service_shutdown_started = Instant::now();
        let gpu_stats = gpu_service.take().map(|service| {
            service
                .shutdown()
                .unwrap_or_else(|error| refuse(&format!("stopping GPU inference service: {error}")))
        });
        let service_shutdown_time = service_shutdown_started.elapsed();

        let merge_started = Instant::now();
        let harvest_time = merge_started.duration_since(harvest_started);
        let mut workers_json = Vec::new();
        let mut by_job: Vec<Option<Result<Played, String>>> =
            (0..jobs.len()).map(|_| None).collect();
        for (played, stages, worker_wall) in harvest {
            if diag.is_some() {
                let stage_s: serde_json::Map<String, serde_json::Value> = ti4_mlp::perf::STAGES
                    .iter()
                    .zip(stages.nanos)
                    .map(|(name, nanos)| {
                        ((*name).to_owned(), serde_json::json!(nanos as f64 / 1e9))
                    })
                    .collect();
                workers_json.push(serde_json::json!({
                    "wall_s": worker_wall,
                    "decisions": stages.decisions,
                    "stage_s": stage_s,
                }));
            }
            for (job, result) in played {
                by_job[job] = Some(result);
            }
        }
        (
            inference_copy_time,
            rolled,
            rollout_lock,
            chunk_copy_time,
            harvest_time,
            merge_started,
            workers_json,
            by_job,
            jobs.len(),
            gpu_stats,
            service_shutdown_time,
        )
        };
        // Job order, whatever order the games finished in.
        // This update's self-imitation picks, in job order (deterministic), and the seats they
        // came from. Built before `Batch::freeze` consumes the steps.
        let mut sil_selected: Vec<ti4_mlp::positive_corpus::AdvantageDemo> = Vec::new();
        let mut sil_seats: std::collections::BTreeSet<(usize, usize)> =
            std::collections::BTreeSet::new();
        let mut stalled_games = 0usize;
        for (job, result) in by_job.into_iter().enumerate() {
            let played = result.unwrap_or_else(|| refuse("a rollout job was never played"));
            // An engine no-progress guard (a rare decision loop) drops that one game, logged, so a
            // long run survives it; anything else, or more than a tenth of the update, still refuses.
            let (game, outcomes, wasted, tactical, record, qualifying) = match played {
                Err(error)
                    if error.contains("without progress") && stalled_games < jobs_len / 10 =>
                {
                    stalled_games += 1;
                    eprintln!("SKIPPED (no-progress guard, update {update}): {error}");
                    continue;
                }
                played => played.unwrap_or_else(|error| refuse(&error)),
            };
            if self_imitation > 0.0 {
                for (step, seat) in game.iter().zip(&qualifying) {
                    let (Some(seat), Some(critic)) = (seat, &step.critic) else {
                        continue;
                    };
                    sil_seats.insert((job, *seat));
                    sil_selected.push(ti4_mlp::positive_corpus::AdvantageDemo {
                        demo: ti4_mlp::positive_corpus::Demo {
                            row: step.row,
                            head: step.head,
                            options: step.options.clone(),
                            chosen: step.chosen,
                            weight: 1.0,
                        },
                        return_to_go: step.return_to_go,
                        critic: critic.clone(),
                    });
                }
            }
            if let Some(record) = record {
                game_lines.push(record);
            }
            games += 1;
            wasted_activations += wasted;
            tactical_actions += tactical;
            seated_decisions += game.len();
            steps.extend(game);
            for outcome in &outcomes {
                window
                    .entry(outcome.faction.clone())
                    .or_default()
                    .add(outcome);
                let tally = update_vp.entry(outcome.faction.clone()).or_default();
                tally.0 += outcome.victory_points;
                tally.1 += 1;
            }
        }
        // --faction-focus: fold this update's per-faction mean VP into the smoothed means. Integer
        // sums in BTreeMap order, so the next update's weights are the same on every run.
        for (faction, (points, games)) in &update_vp {
            #[expect(
                clippy::cast_precision_loss,
                reason = "victory points and game counts are tiny"
            )]
            let mean = *points as f64 / *games as f64;
            smoothed_vp
                .entry(faction.clone())
                .and_modify(|value| *value = FOCUS_SMOOTHING.mul_add(mean - *value, *value))
                .or_insert(mean);
        }
        if let Some(shared) = &async_rt {
            *shared.focus.write().expect("async focus") = (faction_focus > 0.0)
                .then(|| Arc::new(self::focus_weights(&wide_entries, &smoothed_vp, faction_focus)));
        }
        // Each faction row's picks share one unit of weight, so a strong faction that qualifies
        // more often does not dominate the imitation.
        if self_imitation > 0.0 {
            let mut count: BTreeMap<usize, f64> = BTreeMap::new();
            for demo in &sil_selected {
                *count.entry(demo.demo.row.index()).or_default() += 1.0;
            }
            for demo in &mut sil_selected {
                if let Some(total) = count.get(&demo.demo.row.index()) {
                    demo.demo.weight = 1.0 / *total;
                }
            }
        }
        let sil_selected_count = sil_selected.len();
        let sil_seat_count = sil_seats.len();
        if self_imitation > 0.0 {
            if !sil_selected.is_empty() {
                sil_buffer.push_back(sil_selected);
            }
            // An update with no qualifying seat still ages the buffer.
            else if !sil_buffer.is_empty() {
                sil_buffer.pop_front();
            }
            while sil_buffer.len() > sil_keep {
                sil_buffer.pop_front();
            }
        }
        let sil_demos: Vec<ti4_mlp::positive_corpus::AdvantageDemo> =
            sil_buffer.iter().flatten().cloned().collect();
        let merge_time = merge_started.elapsed();
        let rollout_time = rolled.elapsed();
        drop(rollout_lock);
        if steps.is_empty() {
            refuse("self-play recorded no decisions");
        }
        // F-M10-034-D4, the global half. Each seat's returns were already checked against its own
        // decisions; this checks that every seat's decisions reached the batch. A seat lost between
        // the two — by a filter, a drain, a shadowed accumulator — shrinks the batch toward
        // whichever seats survived, and every number downstream stays plausible.
        if steps.len() != seated_decisions {
            refuse(&format!(
                "{seated_decisions} decisions were recorded across seats but {} reached the batch",
                steps.len()
            ));
        }
        // --collect-only: the games were played (and recorded); nothing is trained.
        if collect_only {
            println!(
                "  update {update:>3}  games {games}  decisions {seated_decisions:>7}  rollout {rollout_time:>6.1?}  (collect only, no update)"
            );
            continue;
        }

        // ---- optimise ----
        // Reconstruct a small, faction-balanced clean slice under the frozen source policy. The
        // stored corpus contains action ids, not stale feature dumps; replay is what gives the
        // auxiliary its exact state, return, and critic input.
        let mut demonstrations = Vec::new();
        if demo_per_update > 0 {
            let buckets = FACTIONS.len() * DEMO_TEMPERATURES_MILLI.len();
            if !demo_per_update.is_multiple_of(buckets) {
                refuse(&format!(
                    "--demo-per-update must be divisible by {buckets} to balance six factions and four temperatures"
                ));
            }
            let each = demo_per_update / buckets;
            for faction in *FACTIONS {
                let rows = demo_corpus
                    .get(faction)
                    .unwrap_or_else(|| refuse(&format!("demo corpus has no {faction} rows")));
                for temperature in DEMO_TEMPERATURES_MILLI {
                    let bucket: Vec<&Trajectory> = rows
                        .iter()
                        .filter(|row| row.temperature_milli == temperature)
                        .collect();
                    if bucket.len() < each {
                        refuse(&format!(
                            "demo corpus has {} {faction} trajectories at T={:.3}, needs {each}",
                            bucket.len(),
                            temperature as f64 / 1_000.0
                        ));
                    }
                    for offset in 0..each {
                        let stride = (bucket.len() / each).max(1);
                        let index = (update.wrapping_mul(7_919) + offset * stride) % bucket.len();
                        match replay_advantage(
                            &replay_actor,
                            bucket[index],
                            content,
                            &replay_factions,
                            &vocabulary,
                            &pool,
                            &reward,
                        ) {
                            Ok(mut rows) => demonstrations.append(&mut rows),
                            Err(error) => refuse(&format!(
                                "replaying clean {faction} T={:.3} demo: {error}",
                                temperature as f64 / 1_000.0
                            )),
                        }
                    }
                }
            }
            // One trajectory already carries one total unit of mass. Equalise the six faction
            // masses after replay so a longer Hacan line cannot outweigh a shorter Letnev line.
            let mut mass: BTreeMap<usize, f64> = BTreeMap::new();
            for demo in &demonstrations {
                *mass.entry(demo.demo.row.index()).or_default() += demo.demo.weight;
            }
            for demo in &mut demonstrations {
                if let Some(total) = mass.get(&demo.demo.row.index())
                    && *total > 0.0
                {
                    demo.demo.weight /= *total;
                }
            }
            if demonstrations.is_empty() {
                refuse("the requested clean demonstration slice replayed empty");
            }
        }
        if check_likelihood && update == 0 {
            // Rollouts score on a CPU inference copy; rescore on the same kind of copy, so the check
            // compares the recorded numbers with the same arithmetic rather than with the device's.
            let actor = &models[0].0;
            let scorer = actor.inference_copy().to_device(ti4_tensor::Device::Cpu);
            let mut worst = 0.0f64;
            // On ti4-infer the recorded probabilities are those of its kernels, whose logits are
            // within `logit_bound` · max(1, |logit|) of libtorch's (1e-4 in f32, the int8 bound of
            // the plan on infer-int8): the log-probability within twice that of the largest logit,
            // over the temperature. On libtorch they are the same arithmetic.
            let mut worst_excess = 0.0f64;
            let mut worst_value = 0.0f64;
            let mut by_head: BTreeMap<&str, (usize, f64)> = BTreeMap::new();
            for step in &steps {
                let name = ti4_mlp::all_heads()
                    .iter()
                    .copied()
                    .find(|name| actor.layout_head_index(name).ok() == Some(step.head))
                    .unwrap_or_else(|| refuse(&format!("no head at index {}", step.head)));
                let p = scorer
                    .probabilities(&step.options, name, step.row, step.temperature)
                    .unwrap_or_else(|error| refuse(&format!("rescoring: {error}")));
                let gap = (p[step.chosen].ln() - step.behaviour_log_prob).abs();
                worst = worst.max(gap);
                let tolerance = if let RolloutBackend::Infer(backend) = rollout_backend {
                    let logits = scorer
                        .logits(&step.options, name, step.row)
                        .and_then(|logits| Ok(ti4_tensor::to_vec(&logits)?))
                        .unwrap_or_else(|error| refuse(&format!("rescoring: {error}")));
                    let largest = logits.iter().fold(1.0f64, |m, l| m.max(f64::from(l.abs())));
                    2.0 * ti4_mlp::infer::logit_bound(backend) * largest / step.temperature
                } else {
                    1e-6
                };
                worst_excess = worst_excess.max(gap - tolerance);
                let entry = by_head.entry(name).or_default();
                entry.0 += 1;
                entry.1 = entry.1.max(gap);
                if let (Some(critic), Some(recorded)) = (&step.critic, step.behaviour_value) {
                    let value = scorer
                        .value(critic, step.row)
                        .unwrap_or_else(|error| refuse(&format!("rescoring value: {error}")));
                    worst_value = worst_value.max((value - recorded).abs() / value.abs().max(1.0));
                }
            }
            println!("  value check: max |V - recorded| / max(1, |V|) {worst_value:.3e}");
            println!(
                "  likelihood check: {} steps, max |log p - recorded| {worst:.3e}",
                steps.len()
            );
            println!("  steps by head (count, max gap): {by_head:?}");
            if worst_excess > 0.0 {
                refuse("recorded behaviour probabilities do not reproduce");
            }
        }
        let freeze_started = Instant::now();
        // One batch per model: everything for the shared model, else each faction's own seats
        // (a step's faction row names the faction that took it).
        let grouped: Vec<Vec<Step>> = if models.len() == 1 {
            vec![steps]
        } else {
            let rows: Vec<ti4_mlp::FactionRow> = FACTIONS
                .iter()
                .map(|faction| ti4_mlp::FactionRow::of(faction).expect("a roster row"))
                .collect();
            let mut grouped: Vec<Vec<Step>> = rows.iter().map(|_| Vec::new()).collect();
            for step in steps {
                let model = rows
                    .iter()
                    .position(|row| *row == step.row)
                    .unwrap_or_else(|| refuse("a decision from a faction outside the six"));
                grouped[model].push(step);
            }
            grouped
        };
        let batches: Vec<Batch> = grouped
            .into_iter()
            .map(|steps| {
                Batch::freeze(steps, critic_mode)
                    .unwrap_or_else(|error| refuse(&format!("freezing: {error}")))
            })
            .collect();
        let batch = &batches[0];
        let freeze_time = freeze_started.elapsed();
        // With --hash-games: the whole frozen batch, content and order, as one digest -- the same
        // encoding `ppo_replay` reports for a captured batch, so the two compare directly.
        let batch_digest = hash_games.then(|| {
            ti4_mlp::perf::steps_digest(batch.steps())
                .unwrap_or_else(|error| refuse(&format!("digesting the batch: {error}")))
        });
        let all_steps = || batches.iter().flat_map(Batch::steps);
        let decisions: usize = batches.iter().map(Batch::len).sum();
        let raw_options: usize = all_steps().map(|step| step.options.len()).sum();
        let mut head_counts: BTreeMap<String, usize> = BTreeMap::new();
        for step in all_steps() {
            let name = ti4_mlp::all_heads()
                .get(step.head)
                .copied()
                .unwrap_or("unknown");
            *head_counts.entry(name.to_owned()).or_default() += 1;
        }
        if update == 0
            && let Some(path) = &capture_path
        {
            ti4_mlp::perf::write_capture(batch.steps(), std::path::Path::new(path))
                .unwrap_or_else(|error| refuse(&format!("capturing the batch to {path}: {error}")));
            println!("  captured    {} decisions to {path}", batch.len());
        }
        if decisions != seated_decisions {
            refuse(&format!(
                "freezing changed the decision count from {seated_decisions} to {decisions}"
            ));
        }
        let optimise_lock = phase_lock("optimise");
        let optimised = Instant::now();
        // Linear in the update index, so the last update trains at `entropy_final` times the
        // configured bonus. Applied to every head at once: the three coefficients express a
        // considered ratio between heads, and annealing them apart would change that ratio as a
        // side effect of a schedule that is not about it.
        #[expect(clippy::cast_precision_loss, reason = "update counts are exact in f64")]
        let progress = if updates > 1 {
            update as f64 / (updates - 1) as f64
        } else {
            1.0
        };
        let scale = entropy_start + (entropy_final - entropy_start) * progress;
        let settings = Settings {
            entropy: settings.entropy * scale,
            strategy_entropy: settings.strategy_entropy * scale,
            movement_entropy: settings.movement_entropy * scale,
            ..settings
        };

        // The auxiliary set and its gradient cap: the stored-corpus demonstrations at 10%, else the
        // run's own recent good decisions at `--self-imitation`.
        let (auxiliary, auxiliary_cap): (&[ti4_mlp::positive_corpus::AdvantageDemo], f64) =
            if demonstrations.is_empty() {
                (&sil_demos, self_imitation)
            } else {
                (&demonstrations, 0.10)
            };
        // One update per model, each on its own batch with its own optimiser.
        let mut stats_by_model = Vec::with_capacity(models.len());
        let separate = models.len() > 1;
        for (index, ((actor, optimizer), batch)) in models.iter_mut().zip(&batches).enumerate() {
            // Separate models learn from every faction's good decisions, taken as their own: the
            // demonstration's row becomes this model's faction, so its critic judges the advantage.
            let own_row =
                separate.then(|| ti4_mlp::FactionRow::of(FACTIONS[index]).expect("a roster row"));
            let stats = if auxiliary.is_empty() {
                ti4_mlp::ppo::update(
                    actor,
                    batch,
                    critic_mode,
                    settings,
                    seed_base ^ update as u64,
                    optimizer,
                )
            } else {
                let batches = batch.len().div_ceil(settings.minibatch);
                ti4_mlp::ppo::update_with_auxiliary(
                    actor,
                    batch,
                    critic_mode,
                    settings,
                    seed_base ^ update as u64,
                    optimizer,
                    auxiliary_cap,
                    |current, epoch, minibatch| {
                        let width = 128usize;
                        let start = ((epoch * batches + minibatch) * width) % auxiliary.len();
                        let selected: Vec<_> = (0..width.min(auxiliary.len()))
                            .map(|offset| {
                                let mut demo =
                                    auxiliary[(start + offset) % auxiliary.len()].clone();
                                if let Some(row) = own_row {
                                    demo.demo.row = row;
                                }
                                demo
                            })
                            .collect();
                        ti4_mlp::positive_corpus::advantage_weighted_clone_loss(current, &selected)
                    },
                )
            }
            .unwrap_or_else(|error| refuse(&format!("update: {error}")));
            stats_by_model.push(stats);
        }
        // Every update uploads a batch of a different size, and libtorch keeps freed blocks
        // reserved; without a release the reserve outgrew the card and spilled into shared system
        // memory, where the optimise step ran 5-8x slower (2026-09-22). The update's tensors are
        // gone by now, so this frees only what nothing uses.
        let gpu_memory = if matches!(models[0].0.device(), ti4_tensor::Device::Cuda(_)) {
            let held = ti4_tensor::cuda_cache::memory(0);
            ti4_tensor::cuda_cache::release_cached();
            held.zip(ti4_tensor::cuda_cache::memory(0))
        } else {
            None
        };
        let optimise_time = optimised.elapsed();
        drop(optimise_lock);
        let phases = ti4_mlp::perf::take_phases();
        // Every `--async-publish-games` games processed, hand the rollout workers new weights.
        let mut published = None;
        if let Some(shared) = &async_rt
            && (update + 1) % async_publish_updates == 0
        {
            let packing = Instant::now();
            let packed = pack_models(&models);
            *shared.weights.write().expect("async weights") = (update + 1, packed);
            published = Some((update + 1, packing.elapsed()));
        }

        if let Some((idle, interval, blocked, finished, queued, lag_max, lag_mean)) = async_report {
            let workers = async_workers as f64;
            let lost = blocked.as_secs_f64() / workers;
            println!(
                "  update {update:>3}  games {games}  decisions {decisions:>7}  optimiser idle {idle:>6.1?}  merge {merge_time:>5.1?}  freeze {freeze_time:>5.1?}  optimise {optimise_time:>6.1?}  lag max {lag_max} mean {lag_mean:.2}  queued {queued}",
            );
            println!(
                "              rollout {:.2} games/s  workers blocked {:.1}% ({lost:.1} s/update of {:.1} s)",
                finished as f64 / interval.as_secs_f64().max(1e-9),
                100.0 * lost / interval.as_secs_f64().max(1e-9),
                interval.as_secs_f64()
            );
            if lost > 0.05 * interval.as_secs_f64() {
                println!(
                    "              OPTIMISER IS THE BOTTLENECK: workers lost {lost:.1} s/update; try --async-queue-batches 3 or a larger --async-publish-games"
                );
            }
            if let Some((version, time)) = published {
                println!("              published weights v{version} (packed in {time:.1?})");
            }
        } else {
            println!(
                "  update {:>3}  games {games}  decisions {:>7}  rollout {:>6.1?}  optimise {:>6.1?}  total {:>6.1?}",
                update,
                decisions,
                rollout_time,
                optimise_time,
                rollout_time + optimise_time
            );
        }
        if let Some((before, after)) = gpu_memory {
            #[expect(clippy::cast_precision_loss, reason = "bytes shown in GB")]
            let gb = |bytes: i64| bytes as f64 / f64::from(1_u32 << 30);
            println!(
                "              gpu reserved {:.2} GB -> {:.2} GB after release  allocated {:.2} GB",
                gb(before.reserved),
                gb(after.reserved),
                gb(after.allocated)
            );
        }
        for (index, stats) in stats_by_model.iter().enumerate() {
            let last = stats.last().unwrap_or_else(|| refuse("no epoch ran"));
            let name = if models.len() == 1 {
                String::new()
            } else {
                format!("  [{}]", FACTIONS[index])
            };
            println!(
                "              actor loss {:>9.5}  critic {:>9.5}  |log r| {:>7.5}  clipped {:>6.2}%{name}",
                last.actor_loss,
                last.critic_loss,
                last.kl,
                last.clipped_fraction * 100.0
            );
            let worst = last
                .entropy
                .iter()
                .min_by(|left, right| left.1.total_cmp(right.1));
            if let Some((head, entropy)) = worst {
                println!("              lowest-entropy head {head} at {entropy:.4}{name}");
            }
            // Non-vacuity: an update that moved nothing is not an update, however plausible its
            // telemetry.
            if matches!(critic_mode, CriticMode::BatchMean) && last.critic_loss != 0.0 {
                refuse("batch_mean mode reported a critic loss");
            }
        }
        // Wasted activations per seat-game. This is the quantity the penalty exists to move, so it
        // is reported every update whether or not a penalty is charged -- a run with the penalty at
        // zero still measures it, which is what makes the comparison possible.
        {
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let seats = (games * FACTIONS.len()).max(1) as f64;
            let per_seat = wasted_activations as f64 / seats;
            let tactical_per_seat = tactical_actions as f64 / seats;
            let per_tactical = wasted_activations as f64 / tactical_actions.max(1) as f64;
            println!(
                "              tactical/seat {tactical_per_seat:.3}  waste/seat {per_seat:.3}  waste/tactical {per_tactical:.3}"
            );
            let [overpaid, unused, built, fleet] = std::array::from_fn(|i| {
                PRODUCTION_TELEMETRY[i].swap(0, std::sync::atomic::Ordering::Relaxed) as f64
            });
            println!(
                "              overpay/seat {:.3}  unused/seat {:.3}  built/seat {:.3} (resources, all units full, upgraded x1.3)",
                overpaid / seats,
                unused / seats,
                built / 1000.0 / seats
            );
            println!("              fleet/seat {:.1}", fleet / 1000.0 / seats);
            if self_imitation > 0.0 {
                println!(
                    "              self-imitation  {sil_selected_count} decisions from {sil_seat_count} seats this update, buffer {}",
                    sil_demos.len()
                );
            }
        }

        let report_started = Instant::now();
        // ---- the periodic report ----
        let done = update + 1;
        if cadence.due(done) || done == updates {
            report(done, &window, previous.as_ref(), reported_at);
            if !demonstrations.is_empty() {
                let (reference_kl, greedy_flip) = reference_drift(
                    &replay_actor,
                    &models[0]
                        .0
                        .inference_copy()
                        .to_device(ti4_tensor::Device::Cpu),
                    &demonstrations,
                )
                .unwrap_or_else(|error| refuse(&format!("measuring reference drift: {error}")));
                println!(
                    "  reference   KL {reference_kl:.6}  greedy-action flips {:.2}%",
                    greedy_flip * 100.0
                );
            }
            reported_at = done;
            for (index, (actor, optimizer)) in models.iter().enumerate() {
                if no_checkpoint {
                    break;
                }
                let fingerprint = ti4_mlp::ppo::parameter_fingerprint(actor, critic_mode)
                    .unwrap_or_else(|error| refuse(&format!("fingerprinting parameters: {error}")));
                // Separate models publish under one directory per faction.
                let (directory, model) = if models.len() == 1 {
                    (std::path::PathBuf::from(&out), String::new())
                } else {
                    (
                        std::path::Path::new(&out).join(FACTIONS[index]),
                        format!(", separate model {}", FACTIONS[index]),
                    )
                };
                publish(
                    actor,
                    &directory.join(format!("checkpoint-{}", optimizer.steps())),
                    &slots_text,
                    critic_mode,
                    &ti4_mlp::bundle::Provenance {
                        source: format!(
                            "M10-034 PPO, {done} update(s) from {bundle_path}, roster {roster:?}{model}{}",
                            if self_imitation > 0.0 {
                                format!(
                                    ", self-imitation {self_imitation} margin {sil_margin} keep {sil_keep}"
                                )
                            } else {
                                String::new()
                            }
                        ),
                        git_commit: std::env::var("GIT_COMMIT")
                            .unwrap_or_else(|_| "unrecorded".to_owned()),
                        update: u64::try_from(optimizer.steps()).unwrap_or(0),
                    },
                    &fingerprint,
                );
                if models.len() == 1 {
                    let file = directory.join(format!("checkpoint-{}.adam", optimizer.steps()));
                    if let Err(error) = optimizer.save_state(&file) {
                        println!("  optimiser   state not saved: {error}");
                    }
                }
            }
            previous = Some(std::mem::take(&mut window));
        }
        if let Some(file) = diag.as_mut() {
            let report_time = report_started.elapsed();
            let phase_s: serde_json::Map<String, serde_json::Value> = ti4_mlp::perf::PHASES
                .iter()
                .zip(phases.nanos)
                .map(|(name, nanos)| ((*name).to_owned(), serde_json::json!(nanos as f64 / 1e9)))
                .collect();
            let gpu_service = gpu_stats.map(|stats| {
                serde_json::json!({
                    "requests": stats.requests,
                    "batches": stats.batches,
                    "options": stats.options,
                    "critics": stats.critics,
                    "max_batch": stats.max_batch,
                    "mean_batch": if stats.batches == 0 { 0.0 } else { stats.requests as f64 / stats.batches as f64 },
                    "mean_queue_ms": if stats.requests == 0 { 0.0 } else { stats.queue_nanos as f64 / stats.requests as f64 / 1e6 },
                    "max_queue_ms": stats.max_queue_nanos as f64 / 1e6,
                    "pack_s": stats.pack_nanos as f64 / 1e9,
                    "score_s": stats.score_nanos as f64 / 1e9,
                    "shutdown_s": service_shutdown_time.as_secs_f64(),
                })
            });
            let line = serde_json::json!({
                "kind": "update",
                "update": update,
                "rollout_backend": match &rollout_backend { RolloutBackend::Cpu => "cpu", RolloutBackend::Infer(backend) if backend.is_int8() => "infer-int8", RolloutBackend::Infer(_) => "infer", RolloutBackend::GpuBatched { .. } => "gpu-batched" },
                "games": games,
                "decisions": decisions,
                "raw_options": raw_options,
                "head_counts": head_counts,
                "batch_steps_sha256": batch_digest,
                "wall_s": update_started.elapsed().as_secs_f64(),
                "driver_process_s": process_started.elapsed().as_secs_f64(),
                "inference_copy_s": inference_copy_time.as_secs_f64(),
                "chunk_copy_s": chunk_copy_time.as_secs_f64(),
                "harvest_s": harvest_time.as_secs_f64(),
                "merge_s": merge_time.as_secs_f64(),
                "rollout_s": rollout_time.as_secs_f64(),
                "freeze_s": freeze_time.as_secs_f64(),
                "optimise_s": optimise_time.as_secs_f64(),
                "report_s": report_time.as_secs_f64(),
                "ppo_phase_s": phase_s,
                "ppo_minibatches": phases.minibatches,
                "ppo_options": phases.options,
                "ppo_cells": phases.cells,
                "ppo_widest_max": phases.widest_max,
                "workers": workers_json,
                "gpu_service": gpu_service,
            });
            writeln!(file, "{line}")
                .unwrap_or_else(|error| refuse(&format!("writing --diag: {error}")));
            for game in &game_lines {
                writeln!(file, "{game}")
                    .unwrap_or_else(|error| refuse(&format!("writing --diag: {error}")));
            }
        }
    }

    if let Some(shared) = &async_rt {
        shared.state.lock().expect("async state").stop = true;
        shared.changed.notify_all();
    }
    });

    for ((after_parameters, after_state), (before_parameters, before_state)) in fingerprints(&models)
        .into_iter()
        .zip(&before)
        .filter(|_| !collect_only)
    {
        if after_parameters == *before_parameters {
            refuse(
                "the run moved no parameter; the losses above describe an update that never applied",
            );
        }
        if after_state == *before_state {
            refuse("Adam's moments and step cursor did not advance");
        }
    }
    println!("\n  parameters  moved");
    println!("  adam state  advanced, {} steps", models[0].1.steps());

    println!(
        "\n  done. Rollouts are CPU inference across rayon workers; the optimiser honoured --device."
    );
}

/// `(seats below - seats above) / (seats - 1)`: +1 alone at the top, -1 alone at the bottom, 0 for
/// a lone seat or a full tie. `table` includes the seat itself.
fn projection_rank_bonus(mine: i64, table: impl Iterator<Item = i64>) -> f64 {
    let (mut below, mut above, mut seats) = (0i32, 0i32, 0i32);
    for other in table {
        seats += 1;
        below += i32::from(other < mine);
        above += i32::from(other > mine);
    }
    if seats < 2 {
        return 0.0;
    }
    f64::from(below - above) / f64::from(seats - 1)
}

/// For each decision, the discounted sum of `penalty` over every flagged decision at or after it.
fn discounted_charges(flags: &[bool], penalty: f64, gamma: f64) -> Vec<f64> {
    let mut carried = 0.0;
    let mut out: Vec<f64> = flags
        .iter()
        .rev()
        .map(|flagged| {
            carried = carried * gamma + if *flagged { penalty } else { 0.0 };
            carried
        })
        .collect();
    out.reverse();
    out
}

#[cfg(test)]
mod discounted_charges_tests {
    #[test]
    fn each_flag_charges_itself_and_earlier_decisions_discounted() {
        let charges = super::discounted_charges(&[false, true, false, true], 1.0, 0.5);
        assert_eq!(charges, vec![0.5 + 0.125, 1.0 + 0.25, 0.5, 1.0]);
    }
}

#[cfg(test)]
mod projection_rank_tests {
    #[test]
    fn rank_bonus_spans_minus_one_to_one() {
        let table = [10, 20, 30];
        assert!((super::projection_rank_bonus(30, table.into_iter()) - 1.0).abs() < 1e-12);
        assert!((super::projection_rank_bonus(10, table.into_iter()) + 1.0).abs() < 1e-12);
        assert!(super::projection_rank_bonus(20, table.into_iter()).abs() < 1e-12);
        assert!(super::projection_rank_bonus(5, [5, 5].into_iter()).abs() < 1e-12);
    }
}

/// With `--phase-lock <dir>`, hold `<dir>/ppo-<phase>.lock` for the phase: trainers sharing the
/// directory take turns, one rolling out on the CPU while another optimises on the GPU, instead of
/// fighting over both at the same moments. Waiting happens before the phase timer starts.
fn phase_lock(phase: &str) -> Option<std::fs::File> {
    let dir = argument("--phase-lock")?;
    let path = std::path::Path::new(&dir).join(format!("ppo-{phase}.lock"));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .unwrap_or_else(|error| refuse(&format!("--phase-lock {}: {error}", path.display())));
    file.lock()
        .unwrap_or_else(|error| refuse(&format!("--phase-lock {}: {error}", path.display())));
    Some(file)
}
