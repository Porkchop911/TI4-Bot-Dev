//! CPU policy attribution on real games. Diagnostic copies preserve arithmetic and can be
//! checked against the production decider with `--reference` and `--verify`.
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_mlp::{Actor, FactionRow, SparseOption};
use ti4_model::{
    content_types::DEFAULT,
    id::{FactionId, PlayerId},
};
use ti4_tensor::{Kind, Tensor};
#[path = "inference_cost_support/gather.rs"]
mod gather;
#[path = "inference_cost_support/micro.rs"]
mod micro;
#[path = "inference_cost_support/bot.rs"]
mod profile_bot;
#[path = "inference_cost_support/projection.rs"]
mod projection;
const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

#[derive(Clone)]
struct Sample {
    options: Vec<SparseOption>,
    head: String,
    row: FactionRow,
}
#[derive(Default, Clone)]
struct Profile {
    times: BTreeMap<String, Duration>,
    features: usize,
    options: usize,
    calls: usize,
    capture: bool,
    samples: Vec<Sample>,
}
impl Profile {
    fn add(&mut self, key: &str, d: Duration) {
        *self.times.entry(key.to_owned()).or_default() += d;
    }
}

fn profile_probabilities(
    actor: &Actor,
    options: &[SparseOption],
    head: &str,
    row: FactionRow,
    temperature: f64,
    profile: &Rc<RefCell<Profile>>,
    verify: bool,
) -> Result<Vec<f64>, ti4_mlp::ActorError> {
    {
        let mut p = profile.borrow_mut();
        p.calls += 1;
        if p.capture && p.calls % 97 == 1 {
            p.samples.push(Sample {
                options: options.to_vec(),
                head: head.to_owned(),
                row,
            });
        }
    }
    let started = Instant::now();
    let batch: Vec<(&[i64], &[f32])> = options
        .iter()
        .map(|o| (o.columns.as_slice(), o.values.as_slice()))
        .collect();
    profile.borrow_mut().add("parts", started.elapsed());
    let x = gather::gather_profiled(profile, actor.input(), &batch)?;
    let started = Instant::now();
    let first = (x + actor.identity_row(row) + actor.b1()).relu();
    profile.borrow_mut().add("identity_relu", started.elapsed());
    let started = Instant::now();
    let second = (first.matmul(&actor.hidden().tr()) + actor.b2()).relu();
    profile.borrow_mut().add("hidden_matmul", started.elapsed());
    let started = Instant::now();
    let head_i = i64::try_from(Actor::head_index(head)?).expect("head fits");
    let seat_i = i64::try_from(row.index()).expect("seat fits");
    let w = actor.shared_readout().get(head_i) + actor.delta().get(seat_i).get(head_i);
    let b = actor.b_shared().get(head_i) + actor.b_delta().get(seat_i).get(head_i);
    let scores = (second.matmul(&w) + b) / temperature;
    profile.borrow_mut().add("readout", started.elapsed());
    let started = Instant::now();
    let probabilities = ti4_mlp::stable_softmax(&scores)?;
    profile.borrow_mut().add("softmax_copy", started.elapsed());
    if verify {
        let expected = actor.probabilities(options, head, row, temperature)?;
        assert_eq!(
            probabilities
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            "diagnostic arithmetic changed probabilities"
        );
    }
    Ok(probabilities)
}
#[derive(Default)]
struct Boundary {
    wall: Duration,
    calls: usize,
    digest: Sha256,
}
struct Timed {
    inner: Box<dyn Decider>,
    boundary: Rc<RefCell<Boundary>>,
}
impl Decider for Timed {
    fn choose(&mut self, c: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(c)
    }
    fn choose_seeing(
        &mut self,
        c: &Choice,
        s: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let start = Instant::now();
        let answer = self.inner.choose_seeing(c, s);
        let elapsed = start.elapsed();
        let mut b = self.boundary.borrow_mut();
        b.wall += elapsed;
        b.calls += 1;
        b.digest.update(format!("{c:?}{answer:?}"));
        answer
    }
}
struct GameResult {
    seed: u64,
    rotation: usize,
    wall: Duration,
    policy: Duration,
    calls: usize,
    profile: Profile,
    hashes: [String; 3],
    steps: Vec<ti4_mlp::ppo::Step>,
}
#[derive(Clone)]
struct Config {
    rounds: u32,
    temperature: f64,
    reference: bool,
    verify: bool,
    record: bool,
    capture: bool,
    cache_columns: bool,
}
fn play(
    actor: &Rc<Actor>,
    vocab: &ti4_policy::vocabulary::Vocabulary,
    pool: &Arc<ti4_sim::MapPool>,
    seed: u64,
    rotation: usize,
    cfg: &Config,
) -> Result<GameResult, String> {
    let players: Vec<_> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();
    let assignments: BTreeMap<_, _> = players
        .iter()
        .enumerate()
        .map(|(i, p)| (p.clone(), FactionId::new(FACTIONS[(i + rotation) % 6])))
        .collect();
    let boundary = Rc::new(RefCell::new(Boundary::default()));
    let profile = Rc::new(RefCell::new(Profile {
        capture: cfg.capture,
        ..Default::default()
    }));
    let mut original_status = Vec::new();
    let mut custom_status = Vec::new();
    let mut original_records = Vec::new();
    let mut custom_records = Vec::new();
    let start = Instant::now();
    let mut game = ti4_training::rollout::setup_game_with_decider_factory(
        ContentStore::embedded(),
        &players,
        &assignments,
        DEFAULT,
        seed,
        &ti4_training::rollout::OpeningMap::PythonPool {
            pool: Arc::clone(pool),
            tile_seed_offset: 0,
        },
        |baselines| {
            let mut bots: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (i, p) in players.iter().enumerate() {
                let row = FactionRow::of(assignments[p].as_str()).map_err(|e| e.to_string())?;
                let stream = seed.wrapping_mul(1_000_003).wrapping_add(i as u64);
                let inner = if cfg.reference {
                    let mut bot = ti4_mlp::bot::MlpBot::sharing(actor, vocab.clone(), row, stream)
                        .at_temperature(cfg.temperature)
                        .from_setup(baselines[p]);
                    if cfg.record {
                        bot = bot.recording_ppo(ti4_mlp::bundle::CriticMode::Shared);
                    }
                    original_records.push(bot.ppo_records());
                    let (d, status) = bot.seat();
                    original_status.push(status);
                    d
                } else {
                    let mut bot = profile_bot::MlpBot::sharing(actor, vocab.clone(), row, stream)
                        .at_temperature(cfg.temperature)
                        .from_setup(baselines[p]);
                    bot.profile = Rc::clone(&profile);
                    bot.verify = cfg.verify;
                    bot.cache_columns = cfg.cache_columns;
                    if cfg.record {
                        bot = bot.recording_ppo(ti4_mlp::bundle::CriticMode::Shared);
                    }
                    custom_records.push(bot.ppo_records());
                    let (d, status) = bot.seat();
                    custom_status.push(status);
                    d
                };
                bots.insert(
                    p.clone(),
                    Box::new(Timed {
                        inner,
                        boundary: Rc::clone(&boundary),
                    }),
                );
            }
            Ok(bots)
        },
    )?;
    let target = game.state.round + cfg.rounds;
    let mut steps_count = 0;
    while !game.state.finished && game.state.round < target && steps_count < 400_000 {
        let result = game.step();
        if let Some(e) = result.error {
            return Err(format!("{seed}/{rotation}: {e:?}"));
        }
        steps_count += 1;
    }
    if !game.state.finished && game.state.round < target {
        return Err("step limit".to_owned());
    }
    let wall = start.elapsed();
    for s in original_status {
        s.into_result().map_err(|e| e.to_string())?;
    }
    for s in custom_status {
        s.into_result().map_err(|e| e.to_string())?;
    }
    let hashes = [
        format!("{:x}", boundary.borrow().digest.clone().finalize()),
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&game.events).unwrap())
        ),
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&game.state).unwrap())
        ),
    ];
    let mut steps = Vec::new();
    for h in original_records {
        steps.extend(h.borrow_mut().drain(..).map(|r| r.step));
    }
    for h in custom_records {
        steps.extend(h.borrow_mut().drain(..).map(|r| r.step));
    }
    let p = profile.borrow().clone();
    let b = boundary.borrow();
    Ok(GameResult {
        seed,
        rotation,
        wall,
        policy: b.wall,
        calls: b.calls,
        profile: p,
        hashes,
        steps,
    })
}
fn argument(name: &str) -> Option<String> {
    let a: Vec<_> = std::env::args().collect();
    a.windows(2).find(|p| p[0] == name).map(|p| p[1].clone())
}
fn number<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |v| {
        v.parse().unwrap_or_else(|_| panic!("invalid {name}"))
    })
}
fn flag(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}
fn main() {
    ti4_tensor::configure_deterministic(20_260_826).unwrap();
    let bundle = argument("--bundle")
        .unwrap_or_else(|| "out/checkpoints/stage2-mlp-shaped/checkpoint-473312".to_owned());
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle)).unwrap();
    if let Some(path) = argument("--micro") {
        micro::run(&loaded.actor, &path);
        return;
    }
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            ti4_sim::artifacts::read_and_verify_pool_role(
                std::path::Path::new("out/pools/full_np8_12_holdout.json"),
                &[ti4_sim::artifacts::ArtifactRole::Validation],
            )
            .unwrap(),
        ))
        .unwrap(),
    );
    let workers: usize = number("--threads", 1);
    let seeds: u64 = number("--seeds", 6);
    let base: u64 = number("--seed-base", 900_000_100);
    let rotations: usize = number("--rotations", 6);
    assert!(workers > 0 && workers <= 32 && rotations > 0 && rotations <= 6 && seeds > 0);
    let cfg = Config {
        rounds: number("--rounds", 4),
        temperature: number("--temperature", 0.001),
        reference: flag("--reference"),
        verify: flag("--verify"),
        record: flag("--record"),
        capture: flag("--capture"),
        cache_columns: flag("--cache-columns"),
    };
    let jobs: Vec<_> = (base..base + seeds)
        .flat_map(|s| (0..rotations).map(move |r| (s, r)))
        .collect();
    let team = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    println!("CACHE_COLUMNS {}", cfg.cache_columns);
    let copies = Instant::now();
    let chunks: Vec<_> = jobs
        .chunks(jobs.len().div_ceil(workers))
        .map(|j| (loaded.actor.inference_copy(), j.to_vec()))
        .collect();
    println!(
        "CONFIG threads={workers} jobs={} temperature={} record={} reference={} width={} capacity={} copy_count={} copy_seconds={:.9}",
        jobs.len(),
        cfg.temperature,
        cfg.record,
        cfg.reference,
        loaded.actor.width(),
        loaded.actor.capacity(),
        chunks.len(),
        copies.elapsed().as_secs_f64()
    );
    let start = Instant::now();
    let results: Vec<_> = team.install(|| {
        chunks
            .into_par_iter()
            .map(|(actor, chunk)| {
                let actor = Rc::new(actor);
                chunk
                    .into_iter()
                    .map(|(s, r)| play(&actor, &loaded.vocabulary, &pool, s, r, &cfg))
                    .collect::<Vec<_>>()
            })
            .collect()
    });
    let rollout = start.elapsed();
    let mut total = Profile::default();
    let mut policy = Duration::ZERO;
    let mut game_wall = Duration::ZERO;
    let mut calls = 0;
    let mut steps = Vec::new();
    let mut samples = Vec::new();
    for chunk in results {
        for result in chunk {
            let r = result.unwrap();
            println!(
                "HASH {}/{} {} {} {}",
                r.seed, r.rotation, r.hashes[0], r.hashes[1], r.hashes[2]
            );
            policy += r.policy;
            game_wall += r.wall;
            calls += r.calls;
            for (k, t) in r.profile.times {
                total.add(&k, t);
            }
            total.options += r.profile.options;
            total.features += r.profile.features;
            samples.extend(r.profile.samples);
            steps.extend(r.steps);
        }
    }
    println!(
        "TOTAL rollout_seconds={:.9} summed_game_seconds={:.9} summed_policy_seconds={:.9} calls={calls} options={} features={}",
        rollout.as_secs_f64(),
        game_wall.as_secs_f64(),
        policy.as_secs_f64(),
        total.options,
        total.features
    );
    for (k, t) in &total.times {
        println!("STAGE {k} {:.9}", t.as_secs_f64());
    }
    if cfg.record {
        let n = steps.len();
        let start = Instant::now();
        let batch =
            ti4_mlp::ppo::Batch::freeze(steps, ti4_mlp::bundle::CriticMode::Shared).unwrap();
        println!(
            "FREEZE steps={n} seconds={:.9}",
            start.elapsed().as_secs_f64()
        );
        std::hint::black_box(batch);
    }
    if cfg.capture {
        let data:Vec<_>=samples.iter().map(|s|serde_json::json!({"row":s.row.index(),"head":s.head,"options":s.options.iter().map(|o|serde_json::json!([o.columns,o.values])).collect::<Vec<_>>()})).collect();
        let output = argument("--capture-path")
            .unwrap_or_else(|| "out/inference-samples-20260908.json".to_owned());
        std::fs::write(output, serde_json::to_vec(&data).unwrap()).unwrap();
        println!("SAMPLES {}", samples.len());
    }
}
