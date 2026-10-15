//! The rollouts of training on `ti4-infer` (`web2/plans/2026-10-15-MLP_INFERENCE_PLAN.md`,
//! section D2), on `examples/reviewer/checkpoint-473312` (a shared critic):
//!
//! - the model packed in memory from the actor (`ti4_mlp::infer::InferModel::from_actor`): how long
//!   it takes, printed;
//! - a short seeded PPO run on each rollout backend, libtorch, `ti4-infer` and, on a CPU with
//!   AVX-VNNI, `ti4-infer` with int8 weights (`infer-int8`): self-play games that record PPO steps,
//!   the update on libtorch, and a fresh model of the updated actor for the next update. On
//!   `ti4-infer` every recorded behaviour probability and value is checked against the actor's
//!   within the bound of the backend (`ti4_mlp::infer::logit_bound`);
//! - the bench: a decision and a whole game on each backend, printed.
//!
//! Needs libtorch: `cargo t-mlp-parity` (the command is in `.cargo/config.toml`), which builds
//! `ti4-infer` optimized; the engine, the features and the bot are a debug build.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_mlp::bot::{MlpBot, PpoRecord};
use ti4_mlp::bundle::CriticMode;
use ti4_mlp::infer::InferModel;
use ti4_mlp::ppo::{Batch, Settings, Step};
use ti4_mlp::{Actor, FactionRow};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

fn reviewer() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/reviewer/checkpoint-473312")
}

/// The model behind the seats of a game.
#[derive(Clone)]
enum Backend {
    Tch(Rc<Actor>),
    Infer(Arc<InferModel>),
}

impl Backend {
    fn name(&self) -> String {
        match self {
            Self::Tch(_) => "libtorch".to_owned(),
            Self::Infer(model) => format!("ti4-infer ({})", model.model().backend().name()),
        }
    }
}

/// What one game gave.
struct Game {
    steps: Vec<Step>,
    decisions: usize,
    wall: Duration,
    stages: ti4_mlp::perf::StageTotals,
}

/// One seeded game of six seats on `backend`, every seat recording PPO steps with `mode`, the
/// returns of the stage-one reward attached.
fn play(
    backend: &Backend,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    seed: u64,
    rounds: u32,
    temperature: f64,
    mode: CriticMode,
) -> Game {
    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let factions = ti4_training::rollout::game_factions(
        content,
        ti4_engine::seating::FactionRoster::InScope,
        &FACTIONS,
        &players,
        DEFAULT,
        seed,
    )
    .expect("factions");
    let seated: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            (
                player.clone(),
                ti4_training::rollout::seated_faction(&factions, seed, 0, index),
            )
        })
        .collect();
    let mut handles: BTreeMap<PlayerId, Rc<std::cell::RefCell<Vec<PpoRecord>>>> = BTreeMap::new();
    let mut statuses = Vec::new();
    let _ = ti4_mlp::perf::take_stages();
    let started = Instant::now();
    let (rollout, _) = ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
        content,
        &players,
        &seated,
        DEFAULT,
        seed,
        ti4_training::rollout::Horizon {
            rounds,
            steps: 4_000,
        },
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        &ti4_training::rollout::OpeningMap::RustVaried,
        ti4_training::rollout::SimulationCapabilities { diplomacy: false },
        false,
        |baselines| {
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let row = FactionRow::of(seated[player].as_str()).expect("a roster row");
                let stream = seed * 31 + u64::try_from(index).unwrap_or(0);
                let bot = match backend {
                    Backend::Tch(actor) => MlpBot::sharing(actor, vocabulary.clone(), row, stream),
                    Backend::Infer(model) => {
                        MlpBot::on_infer(model, vocabulary.clone(), row, stream)
                    }
                }
                .at_temperature(temperature)
                .from_setup(baselines[player])
                .recording_ppo(mode);
                handles.insert(player.clone(), bot.ppo_records());
                let (bot, status) = bot.seat();
                statuses.push(status);
                deciders.insert(player.clone(), bot);
            }
            Ok(deciders)
        },
    );
    let wall = started.elapsed();
    let stages = ti4_mlp::perf::take_stages();
    assert!(rollout.error.is_none(), "seed {seed}: {:?}", rollout.error);
    let decisions = statuses
        .into_iter()
        .map(|status| status.into_result().expect("every decision answered"))
        .sum();
    let reward = ti4_training::reward::Reward::for_stage(ti4_training::reward::Stage::One);
    let mut steps = Vec::new();
    for seat in &rollout.seats {
        let mut recorded = handles[&seat.player].borrow_mut();
        let episode = ti4_training::reward::Episode {
            steps: recorded.iter().map(|record| record.progress).collect(),
            final_progress: seat.episode.final_progress,
            cleared: seat.episode.cleared,
            shortfall: seat.episode.shortfall,
            traded_goods: seat.episode.traded_goods,
            strategy_card_plays: seat.episode.strategy_card_plays.clone(),
        };
        let returns = ti4_training::reward::returns(&episode, &reward);
        assert_eq!(returns.len(), recorded.len());
        for (record, value) in recorded.iter_mut().zip(returns) {
            record.step.return_to_go = value;
        }
        steps.extend(recorded.drain(..).map(|record| record.step));
    }
    Game {
        steps,
        decisions,
        wall,
        stages,
    }
}

/// The behaviour probability and value of every step recorded on `ti4-infer`, against the
/// actor's: the log-probability within twice the logit bound `bound` of the largest logit over the
/// temperature, the value within `value_bound · max(1, |value|)`.
fn check_against_the_actor(
    actor: &Actor,
    steps: &[Step],
    bound: f64,
    value_bound: f64,
) -> (f64, f64) {
    let (mut worst_log_prob, mut worst_value) = (0.0f64, 0.0f64);
    for step in steps {
        let head = actor.head_names()[step.head];
        let logits = ti4_tensor::to_vec(
            &actor
                .logits(&step.options, head, step.row)
                .expect("tch logits"),
        )
        .expect("floats");
        let largest = logits
            .iter()
            .fold(1.0f64, |m, logit| m.max(f64::from(logit.abs())));
        let p = actor
            .probabilities(&step.options, head, step.row, step.temperature)
            .expect("tch probabilities");
        let gap = (p[step.chosen].ln() - step.behaviour_log_prob).abs();
        assert!(
            gap <= 2.0 * bound * largest / step.temperature,
            "head {head}: log p {} on libtorch, {} recorded",
            p[step.chosen].ln(),
            step.behaviour_log_prob
        );
        worst_log_prob = worst_log_prob.max(gap);
        let critic = step
            .critic
            .as_ref()
            .expect("a shared critic records its input");
        let want = actor.value(critic, step.row).expect("tch value");
        let got = step.behaviour_value.expect("a recorded value");
        let error = (want - got).abs();
        assert!(
            error <= value_bound * want.abs().max(1.0),
            "value: libtorch {want}, recorded {got}"
        );
        worst_value = worst_value.max(error / want.abs().max(1.0));
    }
    (worst_log_prob, worst_value)
}

#[test]
fn a_short_ppo_run_completes_on_either_rollout_backend() {
    ti4_tensor::configure_deterministic(20_260_826).expect("a deterministic libtorch");
    let loaded = ti4_mlp::bundle::read(&reviewer()).expect("the reviewer bundle");
    assert_eq!(loaded.critic_mode, CriticMode::Shared);
    let slots = std::fs::read_to_string(reviewer().join("slots.json")).expect("slots.json");
    let settings = Settings {
        epochs: 1,
        minibatch: 512,
        ..Settings::default()
    };
    let mut backends = vec![None, Some(ti4_infer::Backend::fastest())];
    if ti4_infer::Backend::Avx2Vnni.is_available() {
        backends.push(Some(ti4_infer::Backend::Avx2Vnni));
    } else {
        println!("no avx2-vnni on this CPU: the int8 run is skipped");
    }
    for infer in backends {
        let mut actor = loaded.actor.inference_copy();
        let mut optimizer =
            ti4_mlp::ppo::Adam::new(&mut actor, CriticMode::Shared, settings).expect("Adam");
        let before =
            ti4_mlp::ppo::parameter_fingerprint(&actor, CriticMode::Shared).expect("a fingerprint");
        for update in 0..2u64 {
            // The rollout workers' model of this update: a fresh one from the updated actor.
            let backend = if let Some(backend) = infer {
                let started = Instant::now();
                let model =
                    InferModel::from_actor(&actor, &slots, backend).expect("the actor packs");
                println!(
                    "update {update}: the actor packed for ti4-infer in {:.1} ms",
                    started.elapsed().as_secs_f64() * 1e3
                );
                Backend::Infer(Arc::new(model))
            } else {
                Backend::Tch(Rc::new(actor.inference_copy()))
            };
            let game = play(
                &backend,
                &loaded.vocabulary,
                900 + update,
                1,
                1.0,
                CriticMode::Shared,
            );
            assert!(!game.steps.is_empty(), "the game recorded no PPO step");
            if let Some(infer) = infer {
                let bound = ti4_mlp::infer::logit_bound(infer);
                let value_bound = if infer.is_int8() {
                    ti4_mlp::infer::INT8_VALUE_BOUND
                } else {
                    bound
                };
                let (log_prob, value) =
                    check_against_the_actor(&actor, &game.steps, bound, value_bound);
                println!(
                    "update {update}: {} steps on ti4-infer ({}), worst |log p| gap {log_prob:.2e}, worst relative value gap {value:.2e}",
                    game.steps.len(),
                    infer.name()
                );
            }
            let batch = Batch::freeze(game.steps, CriticMode::Shared).expect("a batch");
            let stats = ti4_mlp::ppo::update(
                &mut actor,
                &batch,
                CriticMode::Shared,
                settings,
                update,
                &mut optimizer,
            )
            .expect("the update");
            assert!(!stats.is_empty());
            println!(
                "{}: update {update} on {} steps",
                backend.name(),
                batch.len()
            );
        }
        let after =
            ti4_mlp::ppo::parameter_fingerprint(&actor, CriticMode::Shared).expect("a fingerprint");
        assert_ne!(before, after, "the updates moved the weights");
    }
}

#[test]
#[expect(clippy::cast_precision_loss, reason = "durations of a report")]
fn bench_a_decision_and_a_game_on_libtorch_and_on_ti4_infer() {
    ti4_tensor::configure_deterministic(20_260_826).expect("a deterministic libtorch");
    ti4_mlp::perf::enable(false);
    let loaded = ti4_mlp::bundle::read(&reviewer()).expect("the reviewer bundle");
    let slots = std::fs::read_to_string(reviewer().join("slots.json")).expect("slots.json");
    let mut backends = vec![Backend::Tch(Rc::new(loaded.actor.inference_copy()))];
    let mut kinds = vec![ti4_infer::Backend::fastest()];
    if ti4_infer::Backend::Avx2Vnni.is_available() {
        kinds.push(ti4_infer::Backend::Avx2Vnni);
    }
    for kind in kinds {
        let started = Instant::now();
        let model = InferModel::from_actor(&loaded.actor, &slots, kind).expect("the actor packs");
        println!(
            "bench: reviewer 473312 (width {}), the actor packed for ti4-infer ({}) in {:.1} ms",
            loaded.actor.width(),
            kind.name(),
            started.elapsed().as_secs_f64() * 1e3
        );
        backends.push(Backend::Infer(Arc::new(model)));
    }
    let ms = |duration: Duration| duration.as_secs_f64() * 1e3;
    let stage = |game: &Game, index: usize| {
        Duration::from_nanos(u64::try_from(game.stages.nanos[index]).unwrap_or(u64::MAX))
    };
    for backend in &backends {
        // A game of two rounds as the rollouts of stage two record it: temperature 1, a value at
        // every recorded decision. The first game warms the caches and is not counted.
        let _ = play(backend, &loaded.vocabulary, 41, 1, 1.0, CriticMode::Shared);
        let games: Vec<Game> = [42, 43]
            .iter()
            .map(|seed| {
                play(
                    backend,
                    &loaded.vocabulary,
                    *seed,
                    2,
                    1.0,
                    CriticMode::Shared,
                )
            })
            .collect();
        let decisions: usize = games.iter().map(|game| game.decisions).sum();
        let recorded: usize = games.iter().map(|game| game.steps.len()).sum();
        let wall: Duration = games.iter().map(|game| game.wall).sum();
        let forward: Duration = games.iter().map(|game| stage(game, 2)).sum();
        let critic: Duration = games.iter().map(|game| stage(game, 5)).sum();
        let count = games.len() as f64;
        println!(
            "bench {}: per decision forward {:.1} us, per recorded decision critic {:.1} us; \
             per game of two rounds ({:.0} decisions, {:.0} recorded): forward {:.1} ms, critic {:.1} ms, \
             the whole game {:.0} ms (a debug build of the engine and the features)",
            backend.name(),
            forward.as_secs_f64() * 1e6 / decisions as f64,
            critic.as_secs_f64() * 1e6 / recorded.max(1) as f64,
            decisions as f64 / count,
            recorded as f64 / count,
            ms(forward) / count,
            ms(critic) / count,
            ms(wall) / count,
        );
    }
}
