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
//! This exists to be measured as much as to run: every estimate of what M10-038's 30,000 updates
//! would cost has so far been an extrapolation, and one real update replaces all of them.

#![allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::arc_with_non_send_sync,
    reason = "a driver: the phases read in the order they run"
)]

use rayon::prelude::*;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_mlp::bundle::CriticMode;
use ti4_mlp::ppo::{Batch, Settings, Step};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;
/// §6.3: "Each update is 16 game seeds × six rotations."
const SEEDS_PER_UPDATE: u64 = 16;
const ROUNDS: u32 = 4;
/// §6.3's pilot seed base, so a run is reproducible from its update number alone.
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

/// One self-play game, recorded as PPO steps with §6.1's shaped per-decision returns.
///
/// Everything a game needs is passed in rather than captured, because this runs on a rayon worker:
/// `tch::Tensor` is `Send` but **not** `Sync`, so the actor cannot be shared by reference across
/// threads and each worker owns its own inference copy.
#[expect(
    clippy::too_many_arguments,
    reason = "a game's inputs; bundling them into a struct would move the list, not shorten it"
)]
fn play_one(
    actor: &ti4_mlp::Actor,
    content: &ContentStore,
    players: &[PlayerId],
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    pool: &Arc<ti4_sim::MapPool>,
    reward: &ti4_training::reward::Reward,
    critic_mode: ti4_mlp::bundle::CriticMode,
    seed: u64,
    rotation: usize,
) -> Result<Played, String> {
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

    // Deciders are built by a factory so each seat gets the **exact** post-deployment baseline the
    // rollout will score its final progress against. Constructing them earlier cannot supply that,
    // and a shaped return measured against a different baseline is not the return §6.1 defines
    // (F-M10-034-D1).
    //
    // The handles are `Rc`, which is exactly why they are created, filled and drained inside this
    // function: nothing thread-local ever crosses back to the caller.
    let mut handles: BTreeMap<PlayerId, _> = BTreeMap::new();
    let rollout = ti4_training::rollout::play_with_decider_factory(
        content,
        players,
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
                // One detached copy per seat, from this worker's actor. Reading the bundle here
                // instead — which the first version did — SHA-256 verifies ~17 MB of tensors and
                // reparses a 1.1 MB slots.json per seat per game: 576 verifications an update, 14
                // minutes measuring nothing but that mistake.
                let bot = ti4_mlp::bot::MlpBot::new(
                    actor.inference_copy(),
                    vocabulary.clone(),
                    row,
                    stream,
                )
                .recording_ppo(critic_mode)
                .from_setup(baseline);
                if handles.insert(player.clone(), bot.ppo_records()).is_some() {
                    return Err(format!("{player} was seated twice"));
                }
                let (decider, _status) = bot.seat();
                deciders.insert(player.clone(), decider);
            }
            Ok(deciders)
        },
    );
    if let Some(error) = &rollout.error {
        return Err(format!("self-play game {seed}/{rotation} failed: {error}"));
    }

    // The returns, matched to the seat that earned them. A missing handle is refused rather than
    // skipped: silently dropping a seat shrinks the batch and nothing downstream would notice
    // (F-M10-034-D4).
    let mut steps: Vec<Step> = Vec::new();
    let mut outcomes: Vec<SeatOutcome> = Vec::new();
    for seat in &rollout.seats {
        outcomes.push(SeatOutcome {
            faction: seat.faction.to_string(),
            cleared: seat.episode.cleared,
            victory_points: seat.episode.final_progress.victory_points,
        });
        let handle = handles.get(&seat.player).ok_or_else(|| {
            format!(
                "seed {seed} rotation {rotation}: {} has no recording handle",
                seat.player
            )
        })?;
        let mut recorded = handle.borrow_mut();
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
        };
        let per_decision = ti4_training::reward::returns(&episode, reward);
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
        steps.extend(recorded.drain(..).map(|record| record.step));
    }
    Ok((steps, outcomes))
}

/// One played game: the decisions it contributed and what each seat ended with.
type Played = (Vec<Step>, Vec<SeatOutcome>);

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
    span: usize,
) {
    let first = update.saturating_sub(span) + 1;
    let seats: usize = window.values().map(|tally| tally.games).sum();
    println!("\n  ===== report after update {update} (updates {first}-{update}, {seats} seat-games) =====");
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
    let bundle =
        ti4_mlp::bundle::write(destination, &cpu, slots_text, critic_mode, provenance)
            .unwrap_or_else(|error| refuse(&format!("writing the checkpoint: {error}")));
    let reloaded = ti4_mlp::bundle::read(&bundle.directory)
        .unwrap_or_else(|error| refuse(&format!("the checkpoint does not load: {error}")));
    let fingerprint = ti4_mlp::ppo::parameter_fingerprint(&reloaded.actor, reloaded.critic_mode)
        .unwrap_or_else(|error| refuse(&format!("fingerprinting the reload: {error}")));
    if fingerprint != expected {
        refuse("the reloaded checkpoint does not match the weights that were trained");
    }
    println!("  checkpoint  {} (reloaded, identical)", bundle.directory.display());
}

fn main() {
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
    let critic_mode = loaded.critic_mode;

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
    let report_every: usize = argument("--report-every")
        .map_or(100, |value| {
            value
                .parse()
                .unwrap_or_else(|_| refuse("--report-every expects a positive integer"))
        })
        .max(1);
    let settings = Settings::default();
    println!("M10-034 PPO update");
    println!("  bundle      {bundle_path}");
    println!("  critic mode {critic_mode:?}");
    println!("  optimiser   {device:?}   (rollouts always CPU, §7.1)");
    println!(
        "  ppo         clip {} | {} epochs | minibatch {} | value {} | entropy {}/{}",
        settings.clip_epsilon,
        settings.epochs,
        settings.minibatch,
        settings.value_coefficient,
        settings.entropy,
        settings.strategy_entropy
    );
    println!(
        "  update      {SEEDS_PER_UPDATE} seeds x {} rotations\n",
        FACTIONS.len()
    );

    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let reward = ti4_training::reward::Reward::for_stage(ti4_training::reward::Stage::Two);

    // F-M10-034-D3: **once**, for the whole run. Constructing this inside the loop discarded the
    // moments and the step counter on every update after the first, which turns Adam into a
    // sequence of first steps — and Adam's bias correction is a function of `t`, so the first step
    // is the one that behaves least like Adam. Nothing in the telemetry would have shown it.
    actor = actor.to_device(device);
    let mut optimizer = ti4_mlp::ppo::Adam::new(&mut actor, critic_mode, settings)
        .unwrap_or_else(|error| refuse(&format!("optimiser: {error}")));

    // F-M10-034-D6. Loss telemetry is not evidence that an update happened: a broken optimiser
    // still produces a full, plausible table of losses, and the vacuous tests this milestone kept
    // producing failed in exactly that way. Parameters and Adam state are fingerprinted before and
    // after, and the run refuses if either stayed put.
    let before_parameters = ti4_mlp::ppo::parameter_fingerprint(&actor, critic_mode)
        .unwrap_or_else(|error| refuse(&format!("fingerprinting parameters: {error}")));
    let before_state = optimizer
        .state_fingerprint()
        .unwrap_or_else(|error| refuse(&format!("fingerprinting Adam: {error}")));

    // §4.4: weights are stored on CPU, so a checkpoint from a CUDA run loads on a CPU-only machine.
    // Read once here rather than per publish: it is 1.1 MB of JSON and does not change.
    let slots_text = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));

    // Stage-one clearance and victory points accumulate across a reporting window and are compared
    // against the window before it. Per update the numbers are noise -- 96 games, six seats -- but a
    // hundred updates is 9,600 seat-games, which is enough to read a trend from.
    let mut window: BTreeMap<String, FactionTally> = BTreeMap::new();
    let mut previous: Option<BTreeMap<String, FactionTally>> = None;

    for update in 0..updates {
        // ---- rollout, on CPU ----
        //
        // §7.1 pins inference to the CPU, so self-play needs CPU weights. It takes a *copy* rather
        // than moving the training actor: `Adam::new` established the parameters as leaf tensors,
        // and `to_device` on a tensor that requires a gradient returns a non-leaf view of the move.
        // Backward then populates `.grad` on the leaves that were left behind, Adam sees none, and
        // the update silently applies nothing. On CPU the move is a no-op so the bug is invisible;
        // on CUDA it is fatal, which is how it was found.
        //
        // One transfer per update, not one per seat: the per-seat copies are made from this.
        let inference = actor.inference_copy().to_device(ti4_tensor::Device::Cpu);
        let rolled = Instant::now();
        let mut steps: Vec<Step> = Vec::new();
        let mut seated_decisions = 0usize;
        let mut games = 0usize;

        // §6.3's unit of work as a job list. Self-play is embarrassingly parallel — every game is
        // an independent seed — and once the optimizer stopped being launch-bound it was 87% of an
        // update's wall time.
        //
        // Work is split into one chunk per rayon thread rather than one job per thread, because
        // each chunk carries an owned `Actor` copy: `tch::Tensor` is `Send` but not `Sync`, so the
        // actor cannot be borrowed across threads. Per-job copies would allocate 96 actors instead
        // of one per core.
        let base = SEED_BASE + SEEDS_PER_UPDATE * update as u64;
        let jobs: Vec<(u64, usize)> = (base..base + SEEDS_PER_UPDATE)
            .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
            .collect();
        let workers = rayon::current_num_threads().max(1);
        let per_worker = jobs.len().div_ceil(workers);
        let chunks: Vec<(ti4_mlp::Actor, Vec<(u64, usize)>)> = jobs
            .chunks(per_worker)
            .map(|chunk| (inference.inference_copy(), chunk.to_vec()))
            .collect();

        // Collected in chunk order and flattened in job order, so the batch a given update sees does
        // not depend on which worker finished first. Determinism here is not decoration: §6.3's
        // shuffle is seeded, and a batch assembled in scheduling order would make every downstream
        // fingerprint irreproducible.
        let harvest: Vec<Result<Vec<Played>, String>> = chunks
            .into_par_iter()
            .map(|(local, chunk)| {
                chunk
                    .iter()
                    .map(|(seed, rotation)| {
                        play_one(
                            &local,
                            content,
                            &players,
                            &vocabulary,
                            &pool,
                            &reward,
                            critic_mode,
                            *seed,
                            *rotation,
                        )
                    })
                    .collect()
            })
            .collect();

        for chunk in harvest {
            for (game, outcomes) in chunk.unwrap_or_else(|error| refuse(&error)) {
                games += 1;
                seated_decisions += game.len();
                steps.extend(game);
                for outcome in &outcomes {
                    window
                        .entry(outcome.faction.clone())
                        .or_default()
                        .add(outcome);
                }
            }
        }
        let rollout_time = rolled.elapsed();
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

        // ---- optimise ----
        let batch =
            Batch::freeze(steps, critic_mode).unwrap_or_else(|error| refuse(&format!("freezing: {error}")));
        if batch.steps().len() != seated_decisions {
            refuse(&format!(
                "freezing changed the decision count from {seated_decisions} to {}",
                batch.steps().len()
            ));
        }
        let optimised = Instant::now();
        let stats = ti4_mlp::ppo::update(
            &mut actor,
            &batch,
            critic_mode,
            settings,
            SEED_BASE ^ update as u64,
            &mut optimizer,
        )
        .unwrap_or_else(|error| refuse(&format!("update: {error}")));
        let optimise_time = optimised.elapsed();

        let last = stats.last().unwrap_or_else(|| refuse("no epoch ran"));
        println!(
            "  update {:>3}  games {games}  decisions {:>7}  rollout {:>6.1?}  optimise {:>6.1?}  total {:>6.1?}",
            update,
            batch.len(),
            rollout_time,
            optimise_time,
            rollout_time + optimise_time
        );
        println!(
            "              actor loss {:>9.5}  critic {:>9.5}  |log r| {:>7.5}  clipped {:>6.2}%",
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
            println!("              lowest-entropy head {head} at {entropy:.4}");
        }

        // Non-vacuity: an update that moved nothing is not an update, however plausible its
        // telemetry.
        if matches!(critic_mode, CriticMode::BatchMean) && last.critic_loss != 0.0 {
            refuse("batch_mean mode reported a critic loss");
        }

        // ---- the periodic report ----
        let done = update + 1;
        if done % report_every == 0 || done == updates {
            report(done, &window, previous.as_ref(), report_every);
            let fingerprint = ti4_mlp::ppo::parameter_fingerprint(&actor, critic_mode)
                .unwrap_or_else(|error| refuse(&format!("fingerprinting parameters: {error}")));
            publish(
                &actor,
                &std::path::Path::new(&out).join(format!("checkpoint-{}", optimizer.steps())),
                &slots_text,
                critic_mode,
                &ti4_mlp::bundle::Provenance {
                    source: format!("M10-034 PPO, {done} update(s) from {bundle_path}"),
                    git_commit: std::env::var("GIT_COMMIT")
                        .unwrap_or_else(|_| "unrecorded".to_owned()),
                    update: u64::try_from(optimizer.steps()).unwrap_or(0),
                },
                &fingerprint,
            );
            previous = Some(std::mem::take(&mut window));
        }
    }

    let after_parameters = ti4_mlp::ppo::parameter_fingerprint(&actor, critic_mode)
        .unwrap_or_else(|error| refuse(&format!("fingerprinting parameters: {error}")));
    let after_state = optimizer
        .state_fingerprint()
        .unwrap_or_else(|error| refuse(&format!("fingerprinting Adam: {error}")));
    if after_parameters == before_parameters {
        refuse("the run moved no parameter; the losses above describe an update that never applied");
    }
    if after_state == before_state {
        refuse("Adam's moments and step cursor did not advance");
    }
    println!("\n  parameters  moved");
    println!("  adam state  advanced, {} steps", optimizer.steps());

    println!(
        "\n  done. Rollouts are CPU-bound and sequential here; the optimiser honoured --device."
    );
}
