//! Collect every failed opening a policy produces, and price the counterfactual replay that would
//! diagnose them.
//!
//! # Why this exists
//!
//! `opening_failures` says which part of the bar a failure missed. It is serial, it keeps no
//! per-failure record, and it says nothing about what it would cost to investigate one. All three
//! matter now.
//!
//! The plan this serves is a **single-index counterfactual replay**: take a failed line
//! `a_0 .. a_n`, replay `a_0 .. a_{i-1}` exactly, change only `a_i`, let the seat play on, and ask
//! whether that clears. That gives `P(clear | do(a_i = a'))` rather than "some hotter trajectory
//! first differed at `i`" — which is the quantity the previous attempt at this measured, and why it
//! failed: at temperature 2.5 a rescue diverges almost immediately, so "first divergence" collapsed
//! onto the strategy pick and two thirds of its training targets were decision zero (`339f42d`).
//!
//! Before building that, two things have to be known and neither is: how many failures there are
//! per unit of compute, and how many replays one failure costs. This measures both in one pass.
//!
//! # The cost figure
//!
//! Enumerating alternates at index `i` costs `options_i - 1` replays, so a whole failed episode
//! costs `sum over i of (options_i - 1)`. That sum, per failure and per head, is the number this
//! exists to produce. Movement is expected to dominate it, and if it does the enumeration will have
//! to be capped or sampled on that head alone — a decision that should be made against a measured
//! branching factor rather than an assumption about one.
//!
//! # What it records
//!
//! One row per failed seat-game: the seed and rotation that reproduce it exactly, the faction, which
//! parts of the bar were missed, how many planets and distinct systems were reached, and the
//! per-head decision and branching profile of that seat's line. The seed and rotation are what make
//! a row replayable; everything else is what makes it groupable.
//!
//! Failures are collected on the **Train** pool by design. A replay curriculum trains on them, so
//! they must not come from the maps the policy is evaluated on — `clearance_eval` holds the
//! Validation pool for that and nothing here touches it.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example failure_census -- \
//!   --bundle out/checkpoints/sweep-A-250/checkpoint-14476 --seeds 600 --out out/failures/a250.json
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::Decider;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

/// The offset `ppo_update` uses when drawing a map. Failures collected under a different one would
/// not be the failures training sees.
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

/// One decision as the counterfactual replay will meet it.
#[derive(Clone)]
struct Decision {
    head: String,
    /// Legal options offered. `options - 1` is the replay cost of enumerating this index.
    options: usize,
}

/// A decider that answers exactly as its inner one does and writes down what it was asked.
///
/// It must never change an answer: the line recorded here is the line the failure consists of, and
/// a wrapper that perturbed it would be recording a different game from the one it reports.
struct Counting {
    inner: Box<dyn Decider>,
    log: Rc<RefCell<Vec<Decision>>>,
}

impl Counting {
    fn record(&self, choice: &Choice) {
        // The same classifier the policy routes on, so the per-head cost profile is stated in the
        // heads the replay will actually have to enumerate.
        let head = ti4_mlp::Actor::resolve_head(ti4_policy::learned::decision_head(choice));
        self.log.borrow_mut().push(Decision {
            head: head.to_owned(),
            options: choice.options.len(),
        });
    }
}

impl Decider for Counting {
    fn choose(
        &mut self,
        choice: &Choice,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        let chosen = self.inner.choose(choice)?;
        self.record(choice);
        Ok(chosen)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &ti4_engine::choice::SeatObservation<'_>,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        // Both are overridden because the engine calls whichever a site can honestly offer, and
        // recording only one would miss every decision made at the other.
        let chosen = self.inner.choose_seeing(choice, seen)?;
        self.record(choice);
        Ok(chosen)
    }
}

/// One failed seat-game, as a replayable record.
///
/// Every field about the bar comes from `ti4_engine::opening::Opening` and none is recomputed. The
/// bar has four components -- planets gained, distinct systems, capacity ships, infantry -- and the
/// first version of this file rebuilt a three-part approximation of it from `Progress`, which
/// cannot express fleet composition at all. Restating a definition that already exists is how this
/// codebase has produced the same defect eight times. `Opening` is the definition.
struct Failure {
    seed: u64,
    rotation: usize,
    faction: String,
    planets: usize,
    systems: usize,
    /// Shortfalls and the composition verdict, from the engine's own accessors.
    planet_short: usize,
    system_short: usize,
    units_ok: bool,
    decisions: usize,
    /// `sum over decisions of (options - 1)`: the replays a full enumeration of this line costs.
    replay_cost: usize,
    /// Per-head decision count and replay cost.
    by_head: BTreeMap<String, (usize, usize)>,
}

/// Running distribution of a count, kept without storing every sample.
#[derive(Default, Clone)]
struct Spread {
    n: usize,
    total: u64,
    max: usize,
}

impl Spread {
    fn add(&mut self, value: usize) {
        self.n += 1;
        self.total += value as u64;
        self.max = self.max.max(value);
    }
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    fn mean(&self) -> f64 {
        if self.n == 0 {
            return 0.0;
        }
        self.total as f64 / self.n as f64
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one pass: the rollout, the census it produces and the cost it prices are one thing"
)]
fn main() {
    let bundle_path = argument("--bundle")
        .unwrap_or_else(|| refuse("--bundle is required: this censuses a specific policy"));
    // Greedy, because that is how the policy is measured and therefore what its failures are.
    let temperature: f64 = argument("--temperature").map_or(0.001, |value| {
        value
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite() && *parsed > 0.0)
            .unwrap_or_else(|| refuse("--temperature must be a positive number"))
    });
    let seeds: u64 = argument("--seeds").map_or(300, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds must be a number"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(800_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base must be a number"))
    });
    let out_path = argument("--out");

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;

    // Train, and the role guard enforces it: a curriculum built from Validation failures would be
    // training on the maps the policy is scored on.
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

    // The audit path seats the rotation from this list, so it is the roster rather than a
    // pre-built map. Player ids are its business, not this tool's.
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();

    println!("failure census for {bundle_path}");
    println!("  temperature {temperature}");
    println!("  maps        {pool_path} (Train)");
    println!(
        "  seeds       {seed_base}..{} x {} rotations",
        seed_base + seeds,
        FACTIONS.len()
    );
    println!();

    let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers);
    let chunks: Vec<(ti4_mlp::Actor, Vec<(u64, usize)>)> = jobs
        .chunks(per_worker)
        .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
        .collect();

    let started = std::time::Instant::now();
    let harvest: Vec<Result<(usize, Vec<Failure>), String>> = chunks
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let mut seats = 0usize;
            let mut failures = Vec::new();

            for (seed, rotation) in chunk {
                // One log per seat, filled by the wrapper as the game runs. `Rc` never leaves this
                // closure, which is why the logs are drained here rather than returned.
                let mut logs: BTreeMap<PlayerId, Rc<RefCell<Vec<Decision>>>> = BTreeMap::new();

                // The audit path, because it returns the engine's `Opening` per seat. The training
                // rollout keeps only `cleared` and a scalar shortfall, and the per-part breakdown
                // this census exists to produce cannot be recovered from those. It also seats the
                // rotation itself, so the census and `opening_failures` describe the same games.
                let (_events, _setup, assignments, openings, _final_state) =
                    ti4_training::rollout::audit_game_with_deciders(
                        content,
                        &factions,
                        DEFAULT,
                        seed,
                        rotation,
                        ti4_training::rollout::Horizon {
                            rounds: 1,
                            steps: 200_000,
                        },
                        &ti4_training::rollout::OpeningMap::PythonPool {
                            pool: Arc::clone(&pool),
                            tile_seed_offset: TILE_SEED_OFFSET,
                        },
                        |seated, baselines| {
                            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> =
                                BTreeMap::new();
                            for (index, (player, faction)) in seated.iter().enumerate() {
                                let row = ti4_mlp::FactionRow::of(faction.as_str())
                                    .map_err(|error| format!("{player}: {error}"))?;
                                let baseline = baselines
                                    .get(player)
                                    .copied()
                                    .ok_or_else(|| format!("{player} has no setup baseline"))?;
                                let stream = seed
                                    .wrapping_mul(1_000_003)
                                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                                let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
                                    &local,
                                    vocabulary.clone(),
                                    row,
                                    stream,
                                )
                                .at_temperature(temperature)
                                .from_setup(baseline)
                                .seat();
                                let log = Rc::new(RefCell::new(Vec::new()));
                                logs.insert(player.clone(), Rc::clone(&log));
                                deciders.insert(
                                    player.clone(),
                                    Box::new(Counting {
                                        inner: decider,
                                        log,
                                    }),
                                );
                            }
                            Ok(deciders)
                        },
                    )?;

                for (player, opening) in &openings {
                    seats += 1;
                    if opening.cleared() {
                        continue;
                    }
                    let Some(faction) = assignments.get(player) else {
                        return Err(format!("{player} has no faction assignment"));
                    };
                    let log = logs
                        .get(player)
                        .ok_or_else(|| format!("{player} has no decision log"))?
                        .borrow();

                    let mut by_head: BTreeMap<String, (usize, usize)> = BTreeMap::new();
                    let mut replay_cost = 0usize;
                    for decision in log.iter() {
                        let cost = decision.options.saturating_sub(1);
                        replay_cost += cost;
                        let entry = by_head.entry(decision.head.clone()).or_insert((0, 0));
                        entry.0 += 1;
                        entry.1 += cost;
                    }

                    failures.push(Failure {
                        seed,
                        rotation,
                        faction: faction.to_string(),
                        planets: opening.planets_gained,
                        systems: opening.systems,
                        planet_short: opening.planet_shortfall(),
                        system_short: opening.system_shortfall(),
                        units_ok: opening.units_ok(),
                        decisions: log.len(),
                        replay_cost,
                        by_head,
                    });
                }
            }
            Ok((seats, failures))
        })
        .collect();

    let mut seats = 0usize;
    let mut failures: Vec<Failure> = Vec::new();
    for chunk in harvest {
        let (n, mut rows) = chunk.unwrap_or_else(|error| refuse(&error));
        seats += n;
        failures.append(&mut rows);
    }
    if seats == 0 {
        refuse("no seat-games were played");
    }

    // ---- what was found -------------------------------------------------------------------
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let rate = failures.len() as f64 / seats as f64 * 100.0;
    println!(
        "  {seats} seat-games, {} failures ({rate:.2}%), measured in {:.1?}",
        failures.len(),
        started.elapsed()
    );
    println!();

    // The class the whole plan turns on: one part of the bar missed, and it is planets.
    let one_short = failures
        .iter()
        .filter(|f| f.planet_short == 1 && f.system_short == 0 && f.units_ok)
        .count();
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let share = if failures.is_empty() {
        0.0
    } else {
        one_short as f64 / failures.len() as f64 * 100.0
    };
    println!(
        "  one planet short, systems and composition both met: {one_short} ({share:.1}% of failures)"
    );

    // The full picture, because the single class above is only interesting against what surrounds
    // it. Composition is a pass/fail on capacity ships and infantry together, per the requirement.
    let mut shape: BTreeMap<(usize, usize, bool), usize> = BTreeMap::new();
    for failure in &failures {
        *shape
            .entry((failure.planet_short, failure.system_short, failure.units_ok))
            .or_default() += 1;
    }
    let mut shapes: Vec<((usize, usize, bool), usize)> =
        shape.into_iter().map(|(k, v)| (k, v)).collect();
    shapes.sort_by(|a, b| b.1.cmp(&a.1));
    println!();
    println!("  how the failures are shaped (planets short / systems short / composition)");
    println!();
    let mut running = 0usize;
    for ((planets, systems, units), count) in shapes.iter().take(10) {
        running += count;
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let pct = *count as f64 / failures.len() as f64 * 100.0;
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let cum = running as f64 / failures.len() as f64 * 100.0;
        println!(
            "    planets -{planets}  systems -{systems}  composition {}   {count:>4}  {pct:>5.1}%  (cumulative {cum:>5.1}%)",
            if *units { "ok  " } else { "SHORT" }
        );
    }
    println!();

    // ---- the cost of investigating one ------------------------------------------------------
    let mut decisions = Spread::default();
    let mut cost = Spread::default();
    let mut head_decisions: BTreeMap<String, Spread> = BTreeMap::new();
    let mut head_cost: BTreeMap<String, Spread> = BTreeMap::new();
    for failure in &failures {
        decisions.add(failure.decisions);
        cost.add(failure.replay_cost);
        for (head, (count, replays)) in &failure.by_head {
            head_decisions.entry(head.clone()).or_default().add(*count);
            head_cost.entry(head.clone()).or_default().add(*replays);
        }
    }

    println!("  what a single-index counterfactual replay costs, per failed seat-game");
    println!();
    println!(
        "    decisions in the line     mean {:.1}   max {}",
        decisions.mean(),
        decisions.max
    );
    println!(
        "    replays to enumerate      mean {:.1}   max {}",
        cost.mean(),
        cost.max
    );
    println!();
    println!("    head              decisions      replays    mean branching");
    let mut rows: Vec<(&String, &Spread)> = head_cost.iter().collect();
    rows.sort_by(|a, b| {
        b.1.mean()
            .partial_cmp(&a.1.mean())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (head, replays) in rows {
        let count = head_decisions.get(head).cloned().unwrap_or_default();
        let branching = if count.mean() > 0.0 {
            replays.mean() / count.mean() + 1.0
        } else {
            0.0
        };
        println!(
            "    {head:<16}     {:>6.1}      {:>7.1}          {branching:>6.2}",
            count.mean(),
            replays.mean()
        );
    }
    println!();

    // ---- the pool ---------------------------------------------------------------------------
    if let Some(path) = out_path {
        let mut json = String::from("{\"schema\":\"ti4-failure-census-v1\",\"bundle\":\"");
        json.push_str(&bundle_path.replace('\\', "/"));
        json.push_str("\",\"pool\":\"");
        json.push_str(&pool_path.replace('\\', "/"));
        json.push_str(&format!(
            "\",\"temperature\":{temperature},\"seat_games\":{seats},\"failures\":["
        ));
        for (index, f) in failures.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"seed\":{},\"rotation\":{},\"faction\":\"{}\",\"planets\":{},\"systems\":{},\
                 \"planet_short\":{},\"system_short\":{},\"units_ok\":{},\
                 \"decisions\":{},\"replay_cost\":{}}}",
                f.seed,
                f.rotation,
                f.faction,
                f.planets,
                f.systems,
                f.planet_short,
                f.system_short,
                f.units_ok,
                f.decisions,
                f.replay_cost
            ));
        }
        json.push_str("]}");
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|error| refuse(&format!("creating {}: {error}", parent.display())));
        }
        std::fs::write(&path, json)
            .unwrap_or_else(|error| refuse(&format!("writing {path}: {error}")));
        println!("  wrote {} failures to {path}", failures.len());
    }
}
