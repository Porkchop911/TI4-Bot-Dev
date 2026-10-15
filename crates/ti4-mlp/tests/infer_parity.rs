//! The MLP of training on libtorch against `ti4-infer`, decision by decision, on seeded games.
//!
//! Every seat is an `MlpBot` on the libtorch `Actor` (recording PPO steps), shadowed by the shared
//! decision pipeline of `ti4_policy::mlp` with the same seed and temperature. For every decision
//! the shadow scores the same columns on the actor and on a `ti4_infer::Model` packed in memory
//! from the same bundle, and checks:
//!
//! - every logit: `|tch − infer| ≤ 1e-4 · max(1, |tch|)`;
//! - the option drawn at temperature 0.001 from either logits is the same (a pair of options whose
//!   logits are within that bound is a tie, counted and printed, not a failure);
//! - the option the bot answers is the option the shadow draws from the actor's logits, and the
//!   features, head, option and behaviour probability of each PPO step the bot records are the
//!   shadow's, bit for bit;
//! - the time of the forward pass of a decision on libtorch and on `ti4-infer`, printed: in the
//!   game (the caches as the game left them), and again at once (warm).
//!
//! The games are of `examples/reviewer/checkpoint-473312` (schema 7), once as it is and once with
//! a small synthetic battle predictor of fact version 8, so that the battle facts and the fleet
//! decisions run too, and of the first schema-12 bundle under `out/champions/` when the machine
//! has one. Needs libtorch: `cargo t-mlp-parity` (the command is in `.cargo/config.toml`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_mlp::bot::{MlpBot, PpoRecord};
use ti4_mlp::{Actor, FactionRow, SparseOption};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::battle::{
    BattlePredictor, GROUND_INPUT_WIDTH, GROUND_OUTPUT_WIDTH, input_width, output_width,
};
use ti4_policy::mlp::{
    Columns, Features, HeadLayout, MlpError, ModelInfo, Picked, Pipeline, Prepared, Scorer,
};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

/// The temperature of the games: the evaluation temperature of a bot.
const TEMPERATURE: f64 = 0.25;

fn reviewer() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/reviewer/checkpoint-473312")
}

/// The first bundle of schema 12 under `out/champions/`, if the machine has one.
fn champion() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/champions");
    let mut found: Vec<PathBuf> = walk(&root, 3)
        .into_iter()
        .filter(|dir| {
            std::fs::read_to_string(dir.join("manifest.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .is_some_and(|manifest| manifest["schema"] == 12)
        })
        .collect();
    found.sort();
    found.into_iter().next()
}

/// The directories with a `manifest.json` under `dir`, `depth` levels down.
fn walk(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.join("manifest.json").is_file() {
                dirs.push(path.clone());
            }
            if depth > 0 {
                dirs.extend(walk(&path, depth - 1));
            }
        }
    }
    dirs
}

/// A small predictor of fact version 8 (fleet decisions from version 7), with weights that give
/// different predictions for different fleets.
fn synthetic_predictor() -> BattlePredictor {
    let version = 8;
    let (input, output) = (input_width(version), output_width(version));
    let space = vec![
        (input, 2, vec![0.01; input * 2], vec![0.0, 0.5]),
        (2, output, vec![0.2; 2 * output], vec![0.0; output]),
    ];
    let ground = vec![
        (
            GROUND_INPUT_WIDTH,
            1,
            vec![0.3; GROUND_INPUT_WIDTH],
            vec![0.0],
        ),
        (
            1,
            GROUND_OUTPUT_WIDTH,
            vec![1.0; GROUND_OUTPUT_WIDTH],
            vec![0.0; GROUND_OUTPUT_WIDTH],
        ),
    ];
    BattlePredictor::assemble(version, "parity", space, ground).expect("a predictor")
}

/// `ti4_infer::Model` as the model of the pipeline, as `ti4-host` wraps it.
struct Infer {
    model: ti4_infer::Model,
    layout: HeadLayout,
    battle: Option<Arc<BattlePredictor>>,
    scratch: RefCell<ti4_infer::Scratch>,
}

impl ModelInfo for Infer {
    fn layout(&self) -> HeadLayout {
        self.layout
    }

    fn battle_predictor(&self) -> Option<&BattlePredictor> {
        self.battle.as_deref()
    }

    fn temperature(&self) -> f64 {
        f64::from(self.model.temperature())
    }
}

impl Scorer for Infer {
    fn logits(
        &self,
        faction: usize,
        head: usize,
        options: &[Columns],
    ) -> Result<Vec<f32>, MlpError> {
        let mut out = Vec::with_capacity(options.len());
        self.model
            .forward_with(
                &mut self.scratch.borrow_mut(),
                faction,
                head,
                options,
                &mut out,
            )
            .map_err(|error| MlpError::Model(error.to_string()))?;
        Ok(out)
    }
}

/// What the games of one model measured.
#[derive(Default)]
struct Stats {
    decisions: usize,
    fleet_decisions: usize,
    logits: usize,
    worst: f64,
    ties: usize,
    planned: usize,
    records: usize,
    features: Duration,
    columns: usize,
    tch: Duration,
    infer: Duration,
    tch_warm: Duration,
    infer_warm: Duration,
}

/// A PPO step the bot must record for a decision.
struct Expected {
    options: Vec<Columns>,
    head: usize,
    chosen: usize,
    log_prob: f64,
}

/// One seat: the bot, and the shadow pipeline that checks it.
struct Parity {
    bot: Box<dyn Decider>,
    records: Rc<RefCell<Vec<PpoRecord>>>,
    pipeline: Pipeline,
    rng: ChaCha8Rng,
    actor: Rc<Actor>,
    infer: Rc<Infer>,
    stats: Rc<RefCell<Stats>>,
    label: String,
}

impl Parity {
    /// Score one prepared decision on both models, check them, and draw from the actor's logits.
    fn score(&mut self, prepared: &Prepared, expected: &mut Vec<Expected>) -> Picked {
        let started = Instant::now();
        let tch = Scorer::logits(
            &*self.actor,
            prepared.faction,
            prepared.head_index,
            &prepared.options,
        )
        .expect("libtorch logits");
        let between = Instant::now();
        let infer = self
            .infer
            .logits(prepared.faction, prepared.head_index, &prepared.options)
            .expect("ti4-infer logits");
        let done = Instant::now();
        // Again, with the weights of the decision in the caches: the time of a forward pass
        // alone, without the misses that the game between two decisions causes.
        let again = Scorer::logits(
            &*self.actor,
            prepared.faction,
            prepared.head_index,
            &prepared.options,
        )
        .expect("libtorch logits");
        let warm = Instant::now();
        let _ = self
            .infer
            .logits(prepared.faction, prepared.head_index, &prepared.options)
            .expect("ti4-infer logits");
        let warm_done = Instant::now();
        assert_eq!(again, tch, "libtorch gives the same logits twice");
        assert_eq!(tch.len(), infer.len());
        let mut stats = self.stats.borrow_mut();
        stats.tch += between - started;
        stats.infer += done - between;
        stats.tch_warm += warm - done;
        stats.infer_warm += warm_done - warm;
        stats.columns += prepared.options.iter().map(Vec::len).sum::<usize>();
        stats.logits += tch.len();
        if prepared.is_fleet() {
            stats.fleet_decisions += 1;
        } else {
            stats.decisions += 1;
        }
        let bound = |logit: f32| 1e-4 * f64::from(logit.abs()).max(1.0);
        for (index, (a, b)) in tch.iter().zip(&infer).enumerate() {
            let error = f64::from((a - b).abs());
            stats.worst = stats.worst.max(error / f64::from(a.abs()).max(1.0));
            assert!(
                error <= bound(*a),
                "{}: head {} option {index} of {}: libtorch {a}, ti4-infer {b}",
                self.label,
                prepared.head,
                tch.len()
            );
        }
        // The same option at temperature 0.001, from the same draw.
        let seed = u64::try_from(stats.decisions + stats.fleet_decisions).unwrap_or(0);
        let cold = |logits: &[f32]| {
            let p = ti4_policy::mlp::probabilities(logits, 0.001).expect("a distribution");
            ti4_policy::mlp::draw(&p, &mut ChaCha8Rng::seed_from_u64(seed))
        };
        let (a, b) = (cold(&tch), cold(&infer));
        if a != b {
            let gap = f64::from((tch[a] - tch[b]).abs());
            assert!(
                gap <= 2.0 * bound(tch[a]),
                "{}: at temperature 0.001 libtorch draws option {a} ({}) and ti4-infer {b} ({})",
                self.label,
                tch[a],
                tch[b]
            );
            stats.ties += 1;
        }
        drop(stats);
        let picked = self
            .pipeline
            .pick(prepared, &tch, TEMPERATURE, &mut self.rng)
            .expect("a pick");
        if prepared.choices.len() >= 2 {
            expected.push(Expected {
                options: prepared.options.clone(),
                head: prepared.head_index,
                chosen: picked.index,
                log_prob: picked.probabilities[picked.index].ln(),
            });
        }
        picked
    }
}

impl Decider for Parity {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.bot.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let mut expected = Vec::new();
        let started = Instant::now();
        let features = self
            .pipeline
            .features(&*self.actor, choice, seen)
            .expect("features");
        self.stats.borrow_mut().features += started.elapsed();
        let option = match features {
            Features::Planned(option) => {
                self.stats.borrow_mut().planned += 1;
                option
            }
            Features::Score(prepared) => {
                let picked = self.score(&prepared, &mut expected);
                let actor = Rc::clone(&self.actor);
                if let Some(fleet) = self
                    .pipeline
                    .fleet(&*actor, choice, seen, &picked)
                    .expect("fleet features")
                {
                    self.score(&fleet, &mut expected);
                }
                picked.option
            }
        };
        let _ = self.pipeline.take_trace();
        let before = self.records.borrow().len();
        let answered = self.bot.choose_seeing(choice, seen)?;
        assert_eq!(
            answered.id, option.id,
            "{}: the bot answers {} and the pipeline draws {}",
            self.label, answered.id, option.id
        );
        let records = self.records.borrow();
        let fresh = &records[before..];
        assert_eq!(
            fresh.len(),
            expected.len(),
            "{}: the bot recorded {} steps for {} scored decisions",
            self.label,
            fresh.len(),
            expected.len()
        );
        let bits = |values: &[f32]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        for (record, expected) in fresh.iter().zip(&expected) {
            let step = &record.step;
            assert_eq!(step.options.len(), expected.options.len());
            for (got, want) in step.options.iter().zip(&expected.options) {
                let want = SparseOption::from_columns(want);
                assert_eq!(got.columns, want.columns, "{}: the columns", self.label);
                assert_eq!(
                    bits(&got.values),
                    bits(&want.values),
                    "{}: the values",
                    self.label
                );
            }
            assert_eq!(step.head, expected.head, "{}: the head", self.label);
            assert_eq!(step.chosen, expected.chosen, "{}: the option", self.label);
            assert_eq!(
                step.behaviour_log_prob.to_bits(),
                expected.log_prob.to_bits(),
                "{}: the behaviour probability",
                self.label
            );
        }
        let compared = fresh.len();
        drop(records);
        self.stats.borrow_mut().records += compared;
        Ok(answered)
    }
}

/// Seeded games of one bundle, the shadow checking every decision.
#[expect(
    clippy::cast_precision_loss,
    reason = "counts and durations of a report"
)]
fn play(dir: &Path, name: &str, predictor: Option<BattlePredictor>, games: &[(u64, bool)]) {
    let loaded = ti4_mlp::bundle::read(dir).expect("the bundle loads on libtorch");
    let mut actor = loaded.actor;
    let parts = ti4_infer::bundle::read(dir).expect("the bundle reads for ti4-infer");
    let meta = ti4_infer::Meta {
        name: "parity".to_owned(),
        label: name.to_owned(),
        temperature: 0.25,
    };
    let model =
        ti4_infer::Model::from_bytes(&ti4_infer::format::write(&meta, &parts).expect("packs"))
            .expect("loads");
    let battle = match predictor {
        Some(predictor) => Some(Arc::new(predictor)),
        None => model.battle_json().map(|json| {
            Arc::new(BattlePredictor::from_json(json).expect("the bundle's predictor"))
        }),
    };
    if battle.is_some() {
        actor.set_battle_predictor(battle.clone());
    }
    let layout = HeadLayout::of(model.heads()).expect("a known head layout");
    assert_eq!(layout, ModelInfo::layout(&actor));
    let infer = Rc::new(Infer {
        model,
        layout,
        battle,
        scratch: RefCell::new(ti4_infer::Scratch::new()),
    });
    let actor = Rc::new(actor);
    let vocabulary = Arc::new(loaded.vocabulary);
    let stats = Rc::new(RefCell::new(Stats::default()));
    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    for &(seed, diplomacy) in games {
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
        let (rollout, _) = ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
            content,
            &players,
            &seated,
            DEFAULT,
            seed,
            ti4_training::rollout::Horizon {
                rounds: 2,
                steps: 4_000,
            },
            ti4_engine::opening::DEFAULT_REQUIREMENT,
            &ti4_training::rollout::OpeningMap::RustVaried,
            ti4_training::rollout::SimulationCapabilities { diplomacy },
            false,
            |baselines| {
                let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                for (index, player) in players.iter().enumerate() {
                    let row = FactionRow::of(seated[player].as_str()).expect("a roster row");
                    let stream = seed * 31 + u64::try_from(index).unwrap_or(0);
                    let bot = MlpBot::sharing(&actor, (*vocabulary).clone(), row, stream)
                        .at_temperature(TEMPERATURE)
                        .from_setup(baselines[player])
                        .recording_ppo(ti4_mlp::bundle::CriticMode::BatchMean);
                    let records = bot.ppo_records();
                    let (bot, _status) = bot.seat();
                    let mut pipeline = Pipeline::new(Arc::clone(&vocabulary), row.index());
                    pipeline.set_baseline(baselines[player]);
                    deciders.insert(
                        player.clone(),
                        Box::new(Parity {
                            bot,
                            records,
                            pipeline,
                            rng: ChaCha8Rng::seed_from_u64(stream),
                            actor: Rc::clone(&actor),
                            infer: Rc::clone(&infer),
                            stats: Rc::clone(&stats),
                            label: format!("{name} seed {seed} {player}"),
                        }),
                    );
                }
                Ok(deciders)
            },
        );
        assert!(
            rollout.error.is_none(),
            "{name} seed {seed}: {:?}",
            rollout.error
        );
    }
    let stats = stats.borrow();
    let scored = stats.decisions + stats.fleet_decisions;
    assert!(
        stats.decisions > 100,
        "{name}: only {} decisions",
        stats.decisions
    );
    assert!(stats.records > 0, "{name}: no PPO step was compared");
    let per = |total: Duration| total.as_secs_f64() * 1e6 / scored.max(1) as f64;
    println!(
        "{name}: {} decisions and {} fleet decisions scored ({} planned), {:.1} options of {:.0} columns each, {} PPO steps compared",
        stats.decisions,
        stats.fleet_decisions,
        stats.planned,
        stats.logits as f64 / scored.max(1) as f64,
        stats.columns as f64 / stats.logits.max(1) as f64,
        stats.records
    );
    println!(
        "{name}: {} logits, worst |tch - infer| / max(1, |tch|) = {:.2e}, {} ties at temperature 0.001",
        stats.logits, stats.worst, stats.ties
    );
    println!(
        "{name}: forward per decision in the game: libtorch {:.1} us, ti4-infer ({}) {:.1} us; \
         again at once: libtorch {:.1} us, ti4-infer {:.1} us; features (debug build) {:.1} us",
        per(stats.tch),
        infer.model.backend().name(),
        per(stats.infer),
        per(stats.tch_warm),
        per(stats.infer_warm),
        per(stats.features)
    );
}

#[test]
fn libtorch_and_ti4_infer_play_the_same_decisions() {
    ti4_tensor::configure_deterministic(20_260_826).expect("a deterministic libtorch");
    let games = [(21, false), (22, true)];
    play(&reviewer(), "reviewer 473312", None, &games);
    play(
        &reviewer(),
        "reviewer 473312 with a battle predictor",
        Some(synthetic_predictor()),
        &games,
    );
    match champion() {
        Some(dir) => play(&dir, &format!("champion {}", dir.display()), None, &games),
        None => println!("no schema-12 bundle under out/champions/: its games are skipped"),
    }
}
