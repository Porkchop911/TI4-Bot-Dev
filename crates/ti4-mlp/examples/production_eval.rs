//! Production and waste probe for stage-2 checkpoints (a wrapper around the seated MLP bot).
//!
//! Per seat-game, from the decisions the seat takes and the public position it sees:
//! - units built: the `placed` count of every chosen `produce` option, by unit base type.
//! - overpay waste: per production window (first `produce` choice until the seat's next decision
//!   that is not produce/pay/place), resources actually removed from the seat's spendable pool
//!   (exhausted planet faces + trade goods, `available_spend`) minus the summed `cost` of the units
//!   bought. That difference is the credit left over when the window closes, i.e. lost.
//! - ready resources unused: printed resources of the seat's controlled, still-ready planets at
//!   its last Action-phase decision of each round (the status phase readies everything after it),
//!   summed over rounds; also the spendable pool including trade goods.
//!
//! Usage: as `clearance_eval` plus `--out <tsv>`.

use std::collections::BTreeMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_engine::production::{PAY_KIND, PLACE_KIND, PRODUCE_KIND, Spend};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

/// The six trained factions, in the fixed order every rotation is taken against.
/// The six factions a `--roster six` game seats, in rotation order: the historical six, or
/// `--factions a,b,c,d,e,f`.
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

/// The offset `ppo_update` uses when drawing a map from the pool. Evaluation must use the same one
/// or it is scoring a different distribution of maps.
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


#[derive(Default)]
struct Window {
    cost: i64,
    drop: i64,
    valid: bool,
}

#[derive(Default)]
struct Stats {
    units: BTreeMap<String, i64>,
    window: Option<Window>,
    prev_pool: i64,
    prev_round: u32,
    waste: i64,
    production_cost: i64,
    windows: i64,
    negative_windows: i64,
    /// round -> (ready planet resources, pool incl. trade goods) at the last Action-phase decision.
    ready: BTreeMap<u32, (i64, i64)>,
    trace: bool,
    /// round -> board snapshot (last decision of rounds 2 and 4 outside the strategy phase).
    snaps: BTreeMap<u32, BTreeMap<String, f64>>,
}

struct Probe {
    inner: Box<dyn Decider>,
    stats: Rc<RefCell<Stats>>,
    player: PlayerId,
}

impl Probe {
    fn observe(&self, choice: &Choice, seen: &SeatObservation<'_>) {
        let mut s = self.stats.borrow_mut();
        let pool = seen.available_spend(&self.player, Spend::Resources);
        let round = seen.round();
        let action = seen.phase() == ti4_model::state::Phase::Action;
        let related = choice
            .options
            .iter()
            .any(|o| o.kind == PRODUCE_KIND || o.kind == PAY_KIND || o.kind == PLACE_KIND);
        let prev_pool = s.prev_pool;
        let prev_round = s.prev_round;
        if let Some(w) = s.window.as_mut() {
            if !action || round != prev_round {
                w.valid = false;
            }
            w.drop += (prev_pool - pool).max(0);
        }
        if !related {
            if let Some(w) = s.window.take() {
                if w.valid {
                    s.windows += 1;
                    s.production_cost += w.cost;
                    if w.drop < w.cost {
                        s.negative_windows += 1;
                    }
                    s.waste += (w.drop - w.cost).max(0);
                    if s.trace {
                        eprintln!(
                            "  window {}: pool drop {} cost {} -> waste {}",
                            self.player,
                            w.drop,
                            w.cost,
                            (w.drop - w.cost).max(0)
                        );
                    }
                }
            }
        }
        if s.window.is_none() && choice.options.iter().any(|o| o.kind == PRODUCE_KIND) {
            s.window = Some(Window {
                valid: true,
                ..Window::default()
            });
        }
        s.prev_pool = pool;
        s.prev_round = round;
        if action {
            let ready: i64 = seen
                .controlled_planets(&self.player)
                .into_iter()
                .filter(|(_, planet)| seen.planet_is_ready(planet))
                .map(|(_, planet)| {
                    ti4_engine::production::planet_value(
                        seen.content(),
                        seen.sources(),
                        planet,
                        Spend::Resources,
                    )
                })
                .sum();
            s.ready.insert(round, (ready, pool));
        }
        if (round == 2 || round == 4) && seen.phase() != ti4_model::state::Phase::Strategy {
            let snap = snapshot(seen, &self.player);
            s.snaps.insert(round, snap);
        }
    }

    fn record(&self, answer: &ChoiceOption, seen: Option<&SeatObservation<'_>>) {
        if answer.kind != PRODUCE_KIND {
            return;
        }
        let Some(unit) = answer.payload.get("unit").and_then(|v| v.as_str()) else {
            return;
        };
        let placed = answer
            .payload
            .get("placed")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1);
        let cost = answer
            .payload
            .get("cost")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let base = seen
            .and_then(|seen| {
                ti4_content::units::catalogue(seen.content(), seen.sources())
                    .get(unit)
                    .map(|kind| kind.base_type().to_owned())
            })
            .unwrap_or_else(|| unit.to_owned());
        let mut s = self.stats.borrow_mut();
        *s.units.entry(base).or_insert(0) += placed;
        if let Some(w) = s.window.as_mut() {
            w.cost += cost;
        }
        if s.trace {
            eprintln!("  {} builds {} x{placed} cost {cost} ({})", self.player, unit, answer.id);
        }
    }
}

impl Decider for Probe {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let answer = self.inner.choose(choice)?;
        self.record(&answer, None);
        Ok(answer)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.observe(choice, seen);
        let answer = self.inner.choose_seeing(choice, seen)?;
        self.record(&answer, Some(seen));
        Ok(answer)
    }

    fn stage_scores(&mut self, scores: Vec<(Option<f64>, Option<f64>)>) {
        self.inner.stage_scores(scores);
    }
}


const BASES: [&str; 9] = [
    "carrier", "cruiser", "destroyer", "dreadnought", "warsun", "flagship", "fighter", "infantry",
    "mech",
];

/// The seat's board from the public position: units by base type, systems holding its ships, its
/// costliest ship stack (total printed cost of its ships in one system), carried vs capacity, and
/// ship cost split into warships / carriers / fighters.
fn snapshot(seen: &SeatObservation<'_>, player: &PlayerId) -> BTreeMap<String, f64> {
    let types = ti4_content::units::catalogue(seen.content(), seen.sources());
    let mut m: BTreeMap<String, f64> = BTreeMap::new();
    for b in BASES {
        m.insert(format!("n_{b}"), 0.0);
        m.insert(format!("stack_{b}"), 0.0);
    }
    for k in ["n_structures", "ship_systems", "carried", "capacity", "ship_cost", "cost_warship", "cost_carrier", "cost_fighter", "stack_cost"] {
        m.insert(k.to_owned(), 0.0);
    }
    let mut best: Option<(f64, BTreeMap<String, f64>)> = None;
    for system in seen.board().values() {
        let mut stack: BTreeMap<String, f64> = BTreeMap::new();
        let mut stack_cost = 0.0;
        for unit in system.units.iter().chain(system.planet_units.values().flatten()) {
            if unit.owner != *player {
                continue;
            }
            if let Some(kind) = types.get(unit.type_id.as_str()) {
                *m.entry(format!("n_{}", kind.base_type())).or_insert(0.0) += 1.0;
                if kind.is_structure() {
                    *m.get_mut("n_structures").unwrap() += 1.0;
                }
            }
        }
        for unit in &system.units {
            if unit.owner != *player {
                continue;
            }
            let Some(kind) = types.get(unit.type_id.as_str()) else { continue };
            if kind.consumes_capacity() {
                *m.get_mut("carried").unwrap() += 1.0;
            }
            if !kind.is_ship() {
                continue;
            }
            *m.get_mut("capacity").unwrap() += kind.capacity() as f64;
            let cost = kind.cost();
            let class = match kind.base_type() {
                "fighter" => "cost_fighter",
                "carrier" => "cost_carrier",
                "cruiser" | "destroyer" | "dreadnought" | "warsun" | "flagship" => "cost_warship",
                _ => "",
            };
            if !class.is_empty() {
                *m.get_mut(class).unwrap() += cost;
            }
            *m.get_mut("ship_cost").unwrap() += cost;
            stack_cost += cost;
            *stack.entry(format!("stack_{}", kind.base_type())).or_insert(0.0) += 1.0;
        }
        if !stack.is_empty() {
            *m.get_mut("ship_systems").unwrap() += 1.0;
            if best.as_ref().is_none_or(|(c, _)| stack_cost > *c) {
                stack.insert("stack_cost".to_owned(), stack_cost);
                best = Some((stack_cost, stack));
            }
        }
    }
    if let Some((_, stack)) = best {
        for (k, v) in stack {
            m.insert(k, v);
        }
    }
    m
}

struct Row {
    snap: BTreeMap<String, f64>,
    seed: u64,
    rotation: usize,
    faction: String,
    vp: f64,
    units: BTreeMap<String, i64>,
    waste: i64,
    production_cost: i64,
    windows: i64,
    negative_windows: i64,
    ready_planet: i64,
    ready_pool: i64,
}

#[expect(clippy::cast_precision_loss, reason = "small counts")]
fn mean_se(values: &[f64]) -> (f64, f64) {
    let n = values.len().max(1) as f64;
    let mean = values.iter().sum::<f64>() / n;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    (mean, (var / n).sqrt())
}

#[expect(clippy::too_many_lines, reason = "one measurement")]
#[expect(clippy::cast_precision_loss, reason = "small counts")]
fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let temperature: f64 = argument("--temperature")
        .map_or(0.001, |v| v.parse().unwrap_or_else(|_| refuse("--temperature")));
    let seeds: u64 = argument("--seeds").map_or(50, |v| v.parse().unwrap_or_else(|_| refuse("--seeds")));
    let seed_base: u64 = argument("--seed-base")
        .map_or(900_000_000, |v| v.parse().unwrap_or_else(|_| refuse("--seed-base")));
    let rotations: usize = argument("--rotations").map_or(1, |v| v.parse().unwrap_or_else(|_| refuse("--rotations")));
    let diplomacy = std::env::args().any(|a| a == "--diplomacy");
    let trace = std::env::var("PROBE_TRACE").is_ok();
    let roster = argument("--roster")
        .map_or(ti4_engine::seating::FactionRoster::InScope, |value| {
            ti4_training::rollout::parse_roster(&value).unwrap_or_else(|error| refuse(&error))
        });
    let rounds: u32 = argument("--rounds").map_or(4, |v| v.parse().unwrap_or_else(|_| refuse("--rounds")));
    let out = argument("--out");

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let (vocabulary, actor) = (loaded.vocabulary, loaded.actor);
    // A bundle of a separate-models run plays each faction on its own model (packed once for
    // ti4-infer and shared by every worker); otherwise one actor plays every seat.
    let separate: Option<BTreeMap<String, Arc<ti4_mlp::infer::InferModel>>> =
        ti4_mlp::bundle::separate_model_bundles(std::path::Path::new(&bundle_path))
            .unwrap_or_else(|error| refuse(&error))
            .map(|bundles| {
                let slots = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json"))
                    .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
                bundles
                    .into_iter()
                    .map(|(faction, path)| {
                        let model = ti4_mlp::bundle::read(&path)
                            .unwrap_or_else(|error| refuse(&format!("reading {}: {error}", path.display())));
                        let packed = ti4_mlp::infer::InferModel::from_actor(
                            &model.actor,
                            &slots,
                            ti4_infer::Backend::fastest(),
                        )
                        .unwrap_or_else(|error| refuse(&format!("packing {faction}: {error}")));
                        (faction, Arc::new(packed))
                    })
                    .collect()
            });
    if let Some(models) = &separate {
        println!("separate models: {} factions", models.len());
    }

    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Validation],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    println!("production probe for {bundle_path} (T={temperature}, {seeds} seeds x {rotations} rot)");

    let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| (0..rotations).map(move |rotation| (seed, rotation)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers);
    type Chunk = (ti4_mlp::Actor, Vec<(u64, usize)>);
    let chunks: Vec<Chunk> = jobs
        .chunks(per_worker)
        .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
        .collect();

    let started = std::time::Instant::now();
    let harvest: Vec<Result<Vec<Row>, String>> = chunks
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let mut rows = Vec::new();
            for (seed, rotation) in chunk {
                let factions = ti4_training::rollout::game_factions(
                    content, roster, &*FACTIONS, &players, DEFAULT, seed,
                )?;
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
                let mut handles: BTreeMap<PlayerId, Rc<RefCell<Stats>>> = BTreeMap::new();
                let (rollout, _) =
                    ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
                        content,
                        &players,
                        &seated,
                        DEFAULT,
                        seed,
                        ti4_training::rollout::Horizon { rounds, steps: 10_000 },
                        ti4_engine::opening::DEFAULT_REQUIREMENT,
                        &ti4_training::rollout::OpeningMap::PythonPool {
                            pool: Arc::clone(&pool),
                            tile_seed_offset: TILE_SEED_OFFSET,
                        },
                        ti4_training::rollout::SimulationCapabilities { diplomacy },
                        false,
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
                                let bot = match &separate {
                                    Some(models) => ti4_mlp::bot::MlpBot::on_infer(
                                        models.get(seated[player].as_str()).ok_or_else(|| {
                                            format!("the separate models have no {}", seated[player])
                                        })?,
                                        vocabulary.clone(),
                                        row,
                                        stream,
                                    ),
                                    None => ti4_mlp::bot::MlpBot::sharing(
                                        &local,
                                        vocabulary.clone(),
                                        row,
                                        stream,
                                    ),
                                };
                                let (decider, _status) = bot
                                .at_temperature(temperature)
                                .from_setup(baseline)
                                .seat();
                                let stats = Rc::new(RefCell::new(Stats {
                                    trace,
                                    ..Stats::default()
                                }));
                                handles.insert(player.clone(), Rc::clone(&stats));
                                deciders.insert(
                                    player.clone(),
                                    Box::new(Probe { inner: decider, stats, player: player.clone() }),
                                );
                            }
                            Ok(deciders)
                        },
                    );
                if let Some(error) = &rollout.error {
                    if error.contains("without progress") {
                        eprintln!("SKIPPED: game {seed}/{rotation} failed: {error}");
                        continue;
                    }
                    return Err(format!("game {seed}/{rotation} failed: {error}"));
                }
                for seat in &rollout.seats {
                    let s = handles[&seat.player].borrow();
                    let mut snap = BTreeMap::new();
                    for (r, m) in &s.snaps {
                        for (k, v) in m {
                            snap.insert(format!("r{r}_{k}"), *v);
                        }
                    }
                    rows.push(Row {
                        snap,
                        seed,
                        rotation,
                        faction: seat.faction.to_string(),
                        #[expect(clippy::cast_precision_loss, reason = "single digits")]
                        vp: seat.episode.final_progress.victory_points as f64,
                        units: s.units.clone(),
                        waste: s.waste,
                        production_cost: s.production_cost,
                        windows: s.windows,
                        negative_windows: s.negative_windows,
                        ready_planet: s.ready.values().map(|v| v.0).sum(),
                        ready_pool: s.ready.values().map(|v| v.1).sum(),
                    });
                }
            }
            Ok(rows)
        })
        .collect();
    let mut rows: Vec<Row> = Vec::new();
    for chunk in harvest {
        rows.extend(chunk.unwrap_or_else(|error| refuse(&error)));
    }
    rows.sort_by(|a, b| (a.seed, a.rotation, &a.faction).cmp(&(b.seed, b.rotation, &b.faction)));
    let types: Vec<String> = {
        let mut t: Vec<String> = rows.iter().flat_map(|r| r.units.keys().cloned()).collect();
        t.sort();
        t.dedup();
        t
    };

    if let Some(path) = &out {
        let snap_keys: Vec<String> = {
            let mut k: Vec<String> = rows.iter().flat_map(|r| r.snap.keys().cloned()).collect();
            k.sort();
            k.dedup();
            k
        };
        let mut body = format!("seed\trotation\tfaction\tvp\tunits_total\twaste\tproduction_cost\twindows\tneg_windows\tready_planet_res\tready_pool_res\t{}\n", types.join("\t"));
        for r in &rows {
            let total: i64 = r.units.values().sum();
            let per: Vec<String> = types.iter().map(|t| r.units.get(t).copied().unwrap_or(0).to_string()).collect();
            let snaps: Vec<String> = snap_keys.iter().map(|k| r.snap.get(k).copied().unwrap_or(0.0).to_string()).collect();
            body.push_str(&format!(
                "{}\t{}\t{}\t{}\t{total}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                r.seed, r.rotation, r.faction, r.vp, r.waste, r.production_cost, r.windows,
                r.negative_windows, r.ready_planet, r.ready_pool, per.join("\t")
            ));
        }
        std::fs::write(path, body).unwrap_or_else(|error| refuse(&format!("writing {path}: {error}")));
        println!("  wrote {} seat-games to {path}", rows.len());
    }

    let line = |label: &str, set: &[&Row]| {
        let f = |g: &dyn Fn(&Row) -> f64| {
            let v: Vec<f64> = set.iter().map(|r| g(r)).collect();
            mean_se(&v)
        };
        let (vp, vpe) = f(&|r| r.vp);
        let (un, une) = f(&|r| r.units.values().sum::<i64>() as f64);
        let (w, we) = f(&|r| r.waste as f64);
        let (pc, _) = f(&|r| r.production_cost as f64);
        let (rp, rpe) = f(&|r| r.ready_planet as f64);
        let (rq, _) = f(&|r| r.ready_pool as f64);
        let by: Vec<String> = types
            .iter()
            .map(|t| format!("{t}={:.2}", f(&|r| r.units.get(t).copied().unwrap_or(0) as f64).0))
            .collect();
        println!(
            "  {label:<10} n={:<4} VP {vp:.2}+-{vpe:.2} | units {un:.2}+-{une:.2} | prodcost {pc:.1} | waste {w:.2}+-{we:.2} | ready_planet {rp:.1}+-{rpe:.1} | ready_pool {rq:.1} | {}",
            set.len(), by.join(" ")
        );
    };
    let all: Vec<&Row> = rows.iter().collect();
    let mut factions: Vec<&str> = rows.iter().map(|r| r.faction.as_str()).collect();
    factions.sort_unstable();
    factions.dedup();
    for faction in factions {
        let set: Vec<&Row> = rows.iter().filter(|r| r.faction == faction).collect();
        line(faction, &set);
    }
    line("TABLE", &all);
    let neg: i64 = rows.iter().map(|r| r.negative_windows).sum();
    let win: i64 = rows.iter().map(|r| r.windows).sum();
    println!("  windows {win} (cost>drop in {neg}); measured in {:.1?}", started.elapsed());
}
