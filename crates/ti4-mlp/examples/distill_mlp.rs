//! Distil one MLP checkpoint into another, whatever their depths.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example distill_mlp -- \
//!   --teacher out/vponly-main-20260911/checkpoints/checkpoint-236464 \
//!   --student out/blank-shaped-4layers/shaped-r1bonus3-20260912/checkpoint-318956 \
//!   --out out/<run> [--iterations 10] [--seeds 64] [--student-share 0.5] [--device cuda]
//! ```
//!
//! # Why not behaviour cloning
//!
//! Behaviour cloning keeps one sampled action per decision and throws the teacher away. Here the
//! teacher is live: every non-forced decision is labelled with its **whole** distribution at
//! `--teacher-temperature`, and the student minimises `distill::train`'s mean-of-faction-means KL
//! against it. The student learns how sure the teacher is, not only what one draw picked, and a
//! draw from a hot seat never becomes a one-hot target.
//!
//! Only the student's weights move. Teacher and student may differ in depth (residual blocks); they
//! must share one vocabulary, because the student is trained on the columns the teacher was scored
//! on.
//!
//! # Whose games
//!
//! Iteration 0 plays the teacher in every seat. After that each seat is played by the current
//! student with a probability that rises linearly to `--student-share` at the last iteration, and is
//! still labelled by the teacher (DAgger). Teacher-only data teaches the student the teacher's
//! positions; the student's own games teach it the positions its mistakes lead to, which is where a
//! cloned policy otherwise drifts.
//!
//! The decision buffer aggregates across iterations up to `--buffer` decisions, oldest dropped
//! first. Every tenth seed is held out as validation, so validation games are never trained on.
//!
//! # What this does not establish
//!
//! A low KL is imitation, not strength. Every iteration's bundle is kept; judge them with paired
//! greedy `crossplay_eval` runs, not with the in-loop VP tallies, which are sampled at temperature.

#![allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::arc_with_non_send_sync,
    reason = "a driver: the phases read in the order they run"
)]

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use rand::{Rng as _, SeedableRng as _};
use rayon::prelude::*;
use sha2::Digest as _;
use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::{ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_mlp::bundle::{Provenance, read, write};
use ti4_mlp::distill::{Sample, Settings, train, validation_metrics};
use ti4_mlp::{Actor, FactionRow, SparseOption};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::vocabulary::Vocabulary;

/// PPO's training table, in PPO's order, so seating matches the runs both checkpoints came from.
const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
/// PPO's tile offset on the training pool.
const TILE_SEED_OFFSET: u64 = 20_000_000;
/// Away from PPO (650M), the teacher corpus (380M) and offline capture (1.026B).
const SEED_BASE: u64 = 1_500_000_000;
/// A healthy seat-game is two orders of magnitude below this; a game that stops progressing is not.
const MAX_DECISIONS_PER_SEAT: usize = 4_000;
/// One seed in this many is validation.
const VALIDATION_EVERY: u64 = 10;
/// Fraction of games allowed to fail before a whole iteration is refused.
const MAX_FAILED_FRACTION: f64 = 0.01;

const VALUE_FLAGS: &[&str] = &[
    "--batch",
    "--buffer",
    "--device",
    "--epochs",
    "--iterations",
    "--learning-rate",
    "--map-pool",
    "--micro-batch",
    "--out",
    "--patience",
    "--play-temperature",
    "--rounds",
    "--seed-base",
    "--seeds",
    "--student",
    "--student-share",
    "--teacher",
    "--teacher-temperature",
    "--validation-buffer",
    "--workers",
];
const SWITCHES: &[&str] = &["--no-write"];

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn switch(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn parsed<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{name} could not parse {value:?}")))
    })
}

/// A mistyped flag is refused rather than silently falling back to its default.
fn check_flags() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        if VALUE_FLAGS.contains(&flag) {
            index += 2;
        } else if SWITCHES.contains(&flag) {
            index += 1;
        } else {
            refuse(&format!("unknown argument {flag:?}"));
        }
    }
}

fn file_sha256(path: &Path) -> String {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| refuse(&format!("reading {}: {error}", path.display())));
    format!("{:x}", sha2::Sha256::digest(&bytes))
}

fn git(args: &[&str]) -> String {
    std::process::Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || "unknown".to_owned(),
            |output| String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        )
}

/// One seat: plays either the teacher or the student, and always records the teacher's labels.
struct Labeller {
    teacher: Rc<Actor>,
    student: Rc<Actor>,
    student_plays: bool,
    vocabulary: Vocabulary,
    /// `FeatureKey -> (column, assigned)`, memoised as `MlpBot` does.
    resolved: HashMap<ti4_policy::intern::FeatureKey, (usize, bool)>,
    row: FactionRow,
    baseline: ti4_policy::progress::Baseline,
    teacher_temperature: f64,
    play_temperature: f64,
    rng: rand_chacha::ChaCha8Rng,
    samples: Rc<RefCell<Vec<Sample>>>,
}

impl Labeller {
    fn failed(choice: &Choice, reason: String) -> IllegalChoice {
        IllegalChoice::DeciderFailed {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            reason,
        }
    }

    /// The same column routing as `MlpBot::sparse_from`: an unassigned name goes to its
    /// out-of-vocabulary column rather than being dropped.
    fn sparse(
        &mut self,
        vector: &ti4_policy::features::FeatureVector,
    ) -> Result<SparseOption, String> {
        let mut columns = Vec::with_capacity(vector.len());
        let mut values = Vec::with_capacity(vector.len());
        for (key, value) in vector {
            let vocabulary = &self.vocabulary;
            let (column, _) = *self
                .resolved
                .entry(*key)
                .or_insert_with(|| vocabulary.resolve_key(*key));
            columns.push(
                i64::try_from(column)
                    .map_err(|_| format!("feature column {column} does not fit i64"))?,
            );
            let value = *value as f32;
            if !value.is_finite() {
                return Err("a projected feature is not finite f32".to_owned());
            }
            values.push(value);
        }
        Ok(SparseOption { columns, values })
    }

    fn label(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, String> {
        let held = seen.held_secret_progress();
        let vectors = ti4_policy::projection::mlp_choice_features(
            seen.observed(),
            choice,
            &choice.player,
            &held,
            self.baseline,
        );
        let options: Vec<SparseOption> = vectors
            .iter()
            .map(|vector| self.sparse(vector))
            .collect::<Result<_, _>>()?;
        if options.len() != choice.options.len() {
            return Err(format!(
                "projection produced {} vectors for {} legal options",
                options.len(),
                choice.options.len()
            ));
        }
        let head = Actor::resolve_head(ti4_policy::learned::decision_head(choice));
        let head_index =
            Actor::head_index(head).map_err(|error| format!("head {head}: {error}"))?;

        let teacher = self
            .teacher
            .probabilities(&options, head, self.row, self.teacher_temperature)
            .map_err(|error| format!("teacher on head {head}: {error}"))?;
        let behaviour = if self.student_plays {
            self.student
                .probabilities(&options, head, self.row, self.play_temperature)
                .map_err(|error| format!("student on head {head}: {error}"))?
        } else if (self.play_temperature - self.teacher_temperature).abs() < f64::EPSILON {
            teacher.clone()
        } else {
            self.teacher
                .probabilities(&options, head, self.row, self.play_temperature)
                .map_err(|error| format!("teacher on head {head}: {error}"))?
        };
        for distribution in [&teacher, &behaviour] {
            if distribution.len() != options.len()
                || distribution.iter().any(|p| !p.is_finite() || *p < 0.0)
            {
                return Err(format!("malformed distribution on head {head}"));
            }
        }

        let draw: f64 = self.rng.random_range(0.0..1.0);
        let mut cumulative = 0.0;
        let mut chosen = options.len() - 1;
        for (index, probability) in behaviour.iter().enumerate() {
            cumulative += *probability;
            if draw < cumulative {
                chosen = index;
                break;
            }
        }

        let mut samples = self.samples.borrow_mut();
        if samples.len() >= MAX_DECISIONS_PER_SEAT {
            return Err(format!(
                "a seat exceeded {MAX_DECISIONS_PER_SEAT} decisions; the game is not progressing"
            ));
        }
        samples.push(Sample {
            row: self.row,
            head: head_index,
            options,
            teacher,
        });
        Ok(choice.options[chosen].clone())
    }
}

impl Decider for Labeller {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        Err(Self::failed(
            choice,
            "MLP inference requires a bound seat observation".to_owned(),
        ))
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        match choice.options.len() {
            0 => Err(IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            }),
            // Forced: the teacher's distribution is 1.0 whatever it believes, so the KL term is
            // identically zero and the record would only dilute the per-faction means.
            1 => Ok(choice.options[0].clone()),
            _ => self
                .label(choice, seen)
                .map_err(|reason| Self::failed(choice, reason)),
        }
    }
}

struct SeatOutcome {
    faction: String,
    student: bool,
    victory_points: i64,
    cleared: bool,
}

struct Played {
    validation: bool,
    samples: Vec<Sample>,
    seats: Vec<SeatOutcome>,
}

struct Game<'a> {
    content: &'a ContentStore,
    players: &'a [PlayerId],
    vocabulary: &'a Vocabulary,
    pool: &'a Arc<ti4_sim::MapPool>,
    rounds: u32,
    student_share: f64,
    teacher_temperature: f64,
    play_temperature: f64,
}

fn play_one(
    game: &Game<'_>,
    teacher: &Rc<Actor>,
    student: &Rc<Actor>,
    seed: u64,
    rotation: usize,
) -> Result<Played, String> {
    let rotation_bits = u64::try_from(rotation).unwrap_or(0);
    let seated: BTreeMap<PlayerId, FactionId> = game
        .players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            (
                player.clone(),
                ti4_training::rollout::seated_faction(
                    &FACTIONS.map(FactionId::new),
                    seed,
                    rotation,
                    index,
                ),
            )
        })
        .collect();
    let mut handles: BTreeMap<PlayerId, (bool, Rc<RefCell<Vec<Sample>>>)> = BTreeMap::new();
    let rollout = ti4_training::rollout::play_with_decider_factory(
        game.content,
        game.players,
        &seated,
        DEFAULT,
        seed,
        ti4_training::rollout::Horizon {
            rounds: game.rounds,
            steps: 10_000,
        },
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        &ti4_training::rollout::OpeningMap::PythonPool {
            pool: Arc::clone(game.pool),
            tile_seed_offset: TILE_SEED_OFFSET,
        },
        |baselines| {
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (index, player) in game.players.iter().enumerate() {
                let row = FactionRow::of(seated[player].as_str())
                    .map_err(|error| format!("{player}: {error}"))?;
                let baseline = baselines
                    .get(player)
                    .copied()
                    .ok_or_else(|| format!("{player} has no setup baseline"))?;
                let seat_bits = seed.wrapping_mul(1_000_003)
                    ^ (rotation_bits << 40)
                    ^ (u64::try_from(index).unwrap_or(0) << 32);
                let coin: f64 = rand_chacha::ChaCha8Rng::seed_from_u64(seat_bits ^ 0x5EA7_C011)
                    .random_range(0.0..1.0);
                let student_plays = coin < game.student_share;
                let samples = Rc::new(RefCell::new(Vec::new()));
                handles.insert(player.clone(), (student_plays, Rc::clone(&samples)));
                deciders.insert(
                    player.clone(),
                    Box::new(Labeller {
                        teacher: Rc::clone(teacher),
                        student: Rc::clone(student),
                        student_plays,
                        vocabulary: game.vocabulary.clone(),
                        resolved: HashMap::new(),
                        row,
                        baseline,
                        teacher_temperature: game.teacher_temperature,
                        play_temperature: game.play_temperature,
                        rng: rand_chacha::ChaCha8Rng::seed_from_u64(seat_bits ^ 0x5A3D_1E00),
                        samples,
                    }),
                );
            }
            Ok(deciders)
        },
    );
    if let Some(error) = rollout.error {
        return Err(format!("game {seed}/{rotation} failed: {error}"));
    }
    let mut samples = Vec::new();
    let mut seats = Vec::new();
    for seat in &rollout.seats {
        let (student, handle) = handles
            .get(&seat.player)
            .ok_or_else(|| format!("game {seed}/{rotation}: {} has no labeller", seat.player))?;
        samples.append(&mut handle.borrow_mut());
        seats.push(SeatOutcome {
            faction: seat.faction.to_string(),
            student: *student,
            victory_points: seat.episode.final_progress.victory_points,
            cleared: seat.episode.cleared,
        });
    }
    Ok(Played {
        validation: seed % VALIDATION_EVERY == 0,
        samples,
        seats,
    })
}

fn trim(buffer: &mut VecDeque<Sample>, cap: usize) {
    while buffer.len() > cap {
        buffer.pop_front();
    }
}

fn main() {
    check_flags();
    let teacher_path =
        PathBuf::from(argument("--teacher").unwrap_or_else(|| refuse("--teacher is required")));
    let student_path =
        PathBuf::from(argument("--student").unwrap_or_else(|| refuse("--student is required")));
    let out = argument("--out").map(PathBuf::from);
    if out.is_none() && !switch("--no-write") {
        refuse("--out is required (or --no-write for a smoke run that writes nothing)");
    }
    if let Some(out) = &out
        && out.exists()
    {
        refuse(&format!(
            "{} already exists; runs are never written in place",
            out.display()
        ));
    }

    let iterations: usize = parsed("--iterations", 10);
    let seeds: u64 = parsed("--seeds", 64);
    let seed_base: u64 = parsed("--seed-base", SEED_BASE);
    let rounds: u32 = parsed("--rounds", 4);
    let student_share: f64 = parsed("--student-share", 0.5);
    let teacher_temperature: f64 = parsed("--teacher-temperature", 1.0);
    let play_temperature: f64 = parsed("--play-temperature", 1.0);
    let buffer_cap: usize = parsed("--buffer", 3_000_000);
    let validation_cap: usize = parsed("--validation-buffer", 300_000);
    let settings = Settings {
        learning_rate: parsed("--learning-rate", 1e-4),
        batch: parsed("--batch", 4_096),
        micro_batch: parsed("--micro-batch", 2_048),
        max_epochs: parsed("--epochs", 3),
        patience: parsed("--patience", 1),
        preserve_untrained_rows: true,
        ..Settings::default()
    };
    if iterations == 0 || seeds == 0 || rounds == 0 {
        refuse("--iterations, --seeds and --rounds must be positive");
    }
    if !(0.0..=1.0).contains(&student_share) {
        refuse("--student-share must be within [0, 1]");
    }
    if !(teacher_temperature > 0.0 && play_temperature > 0.0) {
        refuse("temperatures must be positive");
    }

    let backend_seed = i64::try_from(seed_base)
        .unwrap_or_else(|_| refuse("--seed-base must fit a signed 64-bit integer"));
    ti4_tensor::configure_deterministic(backend_seed)
        .unwrap_or_else(|error| refuse(&format!("configuring tensor backend: {error}")));
    let workers: usize = parsed(
        "--workers",
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
    );
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
        .unwrap_or_else(|error| refuse(&format!("configuring {workers} workers: {error}")));
    let device = match argument("--device").as_deref().unwrap_or("cuda") {
        "cuda" => ti4_tensor::OptimizerDevice::Cuda
            .resolve()
            .unwrap_or_else(|error| refuse(&format!("CUDA requested: {error}"))),
        "cpu" => ti4_tensor::Device::Cpu,
        other => refuse(&format!("--device expects cuda or cpu, got {other}")),
    };

    let teacher_slots = std::fs::read_to_string(teacher_path.join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("teacher slots: {error}")));
    let student_slots = std::fs::read_to_string(student_path.join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("student slots: {error}")));
    let slots_sha256 = format!("{:x}", sha2::Sha256::digest(student_slots.as_bytes()));
    if format!("{:x}", sha2::Sha256::digest(teacher_slots.as_bytes())) != slots_sha256 {
        refuse(
            "teacher and student vocabularies differ; the student would be trained on columns the teacher never scored",
        );
    }
    let teacher =
        read(&teacher_path).unwrap_or_else(|error| refuse(&format!("teacher bundle: {error}")));
    let student_loaded =
        read(&student_path).unwrap_or_else(|error| refuse(&format!("student bundle: {error}")));
    let critic_mode = student_loaded.critic_mode;
    let vocabulary = student_loaded.vocabulary;
    let teacher_actor = teacher.actor;
    let mut student = student_loaded.actor;
    println!(
        "teacher {} ({} residual blocks), student {} ({} residual blocks)",
        teacher_path.display(),
        teacher_actor.residual_blocks(),
        student_path.display(),
        student.residual_blocks()
    );

    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool_sha256 = format!("{:x}", sha2::Sha256::digest(&pool_bytes));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );

    let git_commit = git(&["rev-parse", "HEAD"]);
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"]) != "";
    let mut log = out.as_ref().map(|out| {
        std::fs::create_dir(out)
            .unwrap_or_else(|error| refuse(&format!("creating {}: {error}", out.display())));
        let run = serde_json::json!({
            "schema": "ti4-mlp-distill-run-v1",
            "created_utc": chrono::Utc::now().to_rfc3339(),
            "command": std::env::args().collect::<Vec<_>>(),
            "git_commit": git_commit,
            "worktree_dirty": dirty,
            "teacher": teacher_path.display().to_string(),
            "teacher_manifest_sha256": file_sha256(&teacher_path.join("manifest.json")),
            "student": student_path.display().to_string(),
            "student_manifest_sha256": file_sha256(&student_path.join("manifest.json")),
            "slots_sha256": slots_sha256,
            "map_pool": pool_path,
            "map_pool_sha256": pool_sha256,
            "tile_seed_offset": TILE_SEED_OFFSET,
            "factions": FACTIONS,
            "iterations": iterations,
            "seeds_per_iteration": seeds,
            "seed_base": seed_base,
            "rounds": rounds,
            "student_share_final": student_share,
            "teacher_temperature": teacher_temperature,
            "play_temperature": play_temperature,
            "buffer": buffer_cap,
            "validation_buffer": validation_cap,
            "learning_rate": settings.learning_rate,
            "batch": settings.batch,
            "micro_batch": settings.micro_batch,
            "epochs": settings.max_epochs,
            "patience": settings.patience,
            "workers": workers,
        });
        std::fs::write(
            out.join("run.json"),
            serde_json::to_string_pretty(&run).expect("run plan serializes"),
        )
        .unwrap_or_else(|error| refuse(&format!("writing run.json: {error}")));
        std::fs::File::create(out.join("iterations.jsonl"))
            .unwrap_or_else(|error| refuse(&format!("creating iterations.jsonl: {error}")))
    });

    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let mut buffer: VecDeque<Sample> = VecDeque::new();
    let mut validation: VecDeque<Sample> = VecDeque::new();

    for iteration in 0..iterations {
        let started = Instant::now();
        let share = if iterations == 1 {
            0.0
        } else {
            student_share * iteration as f64 / (iterations - 1) as f64
        };
        let base = seed_base + seeds * iteration as u64;
        let jobs: Vec<(u64, usize)> = (base..base + seeds)
            .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
            .collect();
        let game = Game {
            content,
            players: &players,
            vocabulary: &vocabulary,
            pool: &pool,
            rounds,
            student_share: share,
            teacher_temperature,
            play_temperature,
        };
        let copies: Vec<(Actor, Actor)> = (0..workers.min(jobs.len()))
            .map(|_| (teacher_actor.inference_copy(), student.inference_copy()))
            .collect();
        let cursor = AtomicUsize::new(0);
        let finished = AtomicUsize::new(0);
        let report_every = (jobs.len() / 10).max(1);
        let mut results: Vec<(usize, Result<Played, String>)> = copies
            .into_par_iter()
            .map(|(teacher_copy, student_copy)| {
                let teacher_copy = Rc::new(teacher_copy);
                let student_copy = Rc::new(student_copy);
                let mut played = Vec::new();
                loop {
                    let job = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some((seed, rotation)) = jobs.get(job) else {
                        break;
                    };
                    played.push((
                        job,
                        play_one(&game, &teacher_copy, &student_copy, *seed, *rotation),
                    ));
                    let done = finished.fetch_add(1, Ordering::Relaxed) + 1;
                    if done % report_every == 0 {
                        println!(
                            "  iteration {iteration}: {done}/{} games in {:.0}s",
                            jobs.len(),
                            started.elapsed().as_secs_f64()
                        );
                    }
                }
                played
            })
            .flatten()
            .collect();
        results.sort_by_key(|(job, _)| *job);
        let play_seconds = started.elapsed().as_secs_f64();

        let mut failures = Vec::new();
        let mut new_train = 0usize;
        let mut new_validation = 0usize;
        // (behaviour, faction) -> (seats, victory points, cleared)
        let mut tally: BTreeMap<(&'static str, String), (usize, i64, usize)> = BTreeMap::new();
        for (_, result) in results {
            match result {
                Err(error) => failures.push(error),
                Ok(played) => {
                    for seat in played.seats {
                        let who = if seat.student { "student" } else { "teacher" };
                        let entry = tally.entry((who, seat.faction)).or_default();
                        entry.0 += 1;
                        entry.1 += seat.victory_points;
                        entry.2 += usize::from(seat.cleared);
                    }
                    if played.validation {
                        new_validation += played.samples.len();
                        validation.extend(played.samples);
                    } else {
                        new_train += played.samples.len();
                        buffer.extend(played.samples);
                    }
                }
            }
        }
        if failures.len() as f64 > jobs.len() as f64 * MAX_FAILED_FRACTION {
            for error in failures.iter().take(5) {
                eprintln!("  {error}");
            }
            refuse(&format!(
                "iteration {iteration}: {} of {} games failed",
                failures.len(),
                jobs.len()
            ));
        }
        trim(&mut buffer, buffer_cap);
        trim(&mut validation, validation_cap);
        println!(
            "iteration {iteration}: student share {share:.3}, {} games in {play_seconds:.0}s ({} failed), +{new_train} train / +{new_validation} validation decisions, buffer {} / {}",
            jobs.len(),
            failures.len(),
            buffer.len(),
            validation.len()
        );
        let mut outcomes = serde_json::Map::new();
        for ((who, faction), (seats_played, vp, cleared)) in &tally {
            let mean_vp = *vp as f64 / *seats_played as f64;
            let clearance = *cleared as f64 / *seats_played as f64;
            println!(
                "  {who:<8} {faction:<7} seats {seats_played:>5}  VP {mean_vp:.3}  cleared {:.1}%",
                clearance * 100.0
            );
            outcomes.insert(
                format!("{who}/{faction}"),
                serde_json::json!({"seats": seats_played, "mean_vp": mean_vp, "cleared": clearance}),
            );
        }
        if buffer.is_empty() || validation.is_empty() {
            refuse("no train or validation decisions yet; raise --seeds to at least 10");
        }

        let train_slice: &[Sample] = buffer.make_contiguous();
        let validation_slice: &[Sample] = validation.make_contiguous();
        let mut actor = student.to_device(device);
        let before = validation_metrics(&actor, validation_slice)
            .unwrap_or_else(|error| refuse(&format!("validation before training: {error}")));
        println!(
            "  before: validation KL {:.5}, top-1 agreement {:.2}%",
            before.mean_kl,
            before.top1_agreement * 100.0
        );
        let train_started = Instant::now();
        let result = train(
            &mut actor,
            train_slice,
            validation_slice,
            settings,
            |epoch| {
                println!(
                    "  epoch {} train KL {:.5} validation KL {:.5} steps {}",
                    epoch.number, epoch.train_kl, epoch.validation_kl, epoch.steps
                );
                let _ = std::io::stdout().flush();
            },
        )
        .unwrap_or_else(|error| refuse(&format!("iteration {iteration} training: {error}")));
        if result.parameter_movement <= 0.0 {
            refuse("parameters did not move");
        }
        let after = validation_metrics(&actor, validation_slice)
            .unwrap_or_else(|error| refuse(&format!("validation after training: {error}")));
        let train_seconds = train_started.elapsed().as_secs_f64();
        println!(
            "  after:  validation KL {:.5}, top-1 agreement {:.2}% (epoch {} selected, {}; movement {:.4}; {train_seconds:.0}s)",
            after.mean_kl,
            after.top1_agreement * 100.0,
            result.selected,
            result.stopped,
            result.parameter_movement
        );
        for (faction, kl) in &after.per_faction {
            println!("    {faction:<7} KL {kl:.5}");
        }
        student = actor.to_device(ti4_tensor::Device::Cpu);

        if let (Some(out), Some(log)) = (&out, log.as_mut()) {
            let bundle = out.join(format!("iter-{iteration:02}"));
            let steps = result
                .epochs
                .iter()
                .find(|epoch| epoch.number == result.selected)
                .map_or(0, |epoch| epoch.steps);
            write(
                &bundle,
                &student,
                &student_slots,
                critic_mode,
                &Provenance {
                    source: format!(
                        "MLP distillation iteration {iteration}: teacher {} into student {}",
                        teacher_path.display(),
                        student_path.display()
                    ),
                    git_commit: git_commit.clone(),
                    update: steps as u64,
                },
            )
            .unwrap_or_else(|error| refuse(&format!("writing {}: {error}", bundle.display())));
            let line = serde_json::json!({
                "iteration": iteration,
                "bundle": bundle.display().to_string(),
                "student_share": share,
                "seeds": [base, base + seeds],
                "games": jobs.len(),
                "failed_games": failures.len(),
                "new_train_decisions": new_train,
                "new_validation_decisions": new_validation,
                "buffer": buffer.len(),
                "validation_buffer": validation.len(),
                "before": {"kl": before.mean_kl, "top1": before.top1_agreement, "per_faction": before.per_faction},
                "after": {"kl": after.mean_kl, "top1": after.top1_agreement, "per_faction": after.per_faction, "per_head": after.per_head},
                "epochs": result.epochs.iter().map(|epoch| serde_json::json!({
                    "number": epoch.number, "train_kl": epoch.train_kl,
                    "validation_kl": epoch.validation_kl, "steps": epoch.steps,
                })).collect::<Vec<_>>(),
                "selected_epoch": result.selected,
                "parameter_movement": result.parameter_movement,
                "outcomes": outcomes,
                "play_seconds": play_seconds,
                "train_seconds": train_seconds,
            });
            writeln!(log, "{line}")
                .and_then(|()| log.flush())
                .unwrap_or_else(|error| refuse(&format!("writing iterations.jsonl: {error}")));
            println!("  wrote {}", bundle.display());
        }
    }
}
