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
//! path. `--device cuda` moves the model for the optimiser phase only, between rollouts, and the
//! actor returns to CPU before the next game.
//!
//! This exists to be measured as much as to run: every estimate of what M10-038's 30,000 updates
//! would cost has so far been an extrapolation, and one real update replaces all of them.

#![allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::arc_with_non_send_sync,
    reason = "a driver: the phases read in the order they run"
)]

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

    for update in 0..updates {
        // ---- rollout, on CPU ----
        actor = actor.to_device(ti4_tensor::Device::Cpu);
        let rolled = Instant::now();
        let mut steps: Vec<Step> = Vec::new();
        let mut seated_decisions = 0usize;
        let mut games = 0usize;

        let base = SEED_BASE + SEEDS_PER_UPDATE * update as u64;
        for seed in base..base + SEEDS_PER_UPDATE {
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

                // Deciders are built by a factory so each seat gets the **exact** post-deployment
                // baseline the rollout will score its final progress against. Constructing them
                // earlier cannot supply that, and a shaped return measured against a different
                // baseline is not the return §6.1 defines (F-M10-034-D1).
                let mut handles: BTreeMap<PlayerId, _> = BTreeMap::new();
                let rollout = ti4_training::rollout::play_with_decider_factory(
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
                        pool: Arc::clone(&pool),
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
                            // One detached copy per seat. Reading the bundle here instead — which
                            // the first version did — SHA-256 verifies ~17 MB of tensors and
                            // reparses a 1.1 MB slots.json per seat per game: 576 verifications an
                            // update, 14 minutes measuring nothing but that mistake.
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
                    refuse(&format!("self-play game {seed}/{rotation} failed: {error}"));
                }
                games += 1;

                // The returns, matched to the seat that earned them. A missing handle is refused
                // rather than skipped: silently dropping a seat shrinks the batch and nothing
                // downstream would notice (F-M10-034-D4).
                for seat in &rollout.seats {
                    let handle = handles.get(&seat.player).unwrap_or_else(|| {
                        refuse(&format!(
                            "seed {seed} rotation {rotation}: {} has no recording handle",
                            seat.player
                        ))
                    });
                    let mut recorded = handle.borrow_mut();
                    // §6.1's shaped per-decision return. Each recorded decision carries the
                    // progress measured **at** that decision against the seat's own setup
                    // baseline, so `returns` can telescope them into a return-to-go per decision.
                    //
                    // The first version built a one-step episode from the final progress and gave
                    // every decision in the game the same number. The advantage is `return − V(s)`,
                    // so with a constant return the only thing separating decisions was the critic,
                    // and the within-game credit assignment §6.1's shaping exists for was gone. It
                    // trained; the objective was wrong.
                    let episode = ti4_training::reward::Episode {
                        steps: recorded.iter().map(|record| record.progress).collect(),
                        final_progress: seat.episode.final_progress,
                        cleared: seat.episode.cleared,
                        shortfall: seat.episode.shortfall,
                        traded_goods: seat.episode.traded_goods,
                    };
                    let per_decision = ti4_training::reward::returns(&episode, &reward);
                    if per_decision.len() != recorded.len() {
                        refuse(&format!(
                            "seed {seed} rotation {rotation} {}: {} returns for {} decisions",
                            seat.player,
                            per_decision.len(),
                            recorded.len()
                        ));
                    }
                    for (record, value) in recorded.iter_mut().zip(&per_decision) {
                        record.step.return_to_go = *value;
                    }
                    seated_decisions += recorded.len();
                    steps.extend(recorded.drain(..).map(|record| record.step));
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
        actor = actor.to_device(device);
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

    // §4.4: weights are stored on CPU, so a checkpoint from a CUDA run loads on a CPU-only machine.
    let actor = actor.to_device(ti4_tensor::Device::Cpu);
    let destination = std::path::Path::new(&out).join(format!("checkpoint-{}", optimizer.steps()));
    let slots_text = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
    let bundle = ti4_mlp::bundle::write(
        &destination,
        &actor,
        &slots_text,
        critic_mode,
        &ti4_mlp::bundle::Provenance {
            source: format!("M10-034 PPO, {updates} update(s) from {bundle_path}"),
            git_commit: std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unrecorded".to_owned()),
            update: u64::try_from(optimizer.steps()).unwrap_or(0),
        },
    )
    .unwrap_or_else(|error| refuse(&format!("writing the checkpoint: {error}")));
    println!("  checkpoint  {}", bundle.directory.display());

    // Read it back before claiming it exists.
    let reloaded = ti4_mlp::bundle::read(&bundle.directory)
        .unwrap_or_else(|error| refuse(&format!("the checkpoint does not load: {error}")));
    let reloaded_fingerprint =
        ti4_mlp::ppo::parameter_fingerprint(&reloaded.actor, reloaded.critic_mode)
            .unwrap_or_else(|error| refuse(&format!("fingerprinting the reload: {error}")));
    if reloaded_fingerprint != after_parameters {
        refuse("the reloaded checkpoint does not match the weights that were trained");
    }
    println!("  reloaded    parameters identical");

    println!(
        "\n  done. Rollouts are CPU-bound and sequential here; the optimiser honoured --device."
    );
}
