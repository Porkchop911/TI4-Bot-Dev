//! Per-seat economy ledger: what generates each asset, what it is spent on, and what sits idle.
//!
//! Plays the same games `clearance_eval` plays (held-out pool, fixed seeds, one temperature) and
//! wraps every seat's bot in a tracker. At each of a seat's own decisions the tracker snapshots that
//! seat's public position and attributes every change since its previous decision to that previous
//! decision's purpose (typed `DecisionContext` source and subtype, or the prompt). Payment steps are
//! charged to the decision they pay for.
//!
//! Output is long format, one row per `(seed, rotation, faction, metric, value)`, for analysis
//! outside Rust. Metric families:
//!
//! - `gain.<asset>[.<category>]` / `spend.<asset>[.<category>]` -- increases and decreases of trade
//!   goods, commodities, the three token pools, action cards, technologies, units and planets.
//! - `face.<R|I|unknown>.<category>` -- printed value of planets newly exhausted, by the side the
//!   purpose pays with (an outstanding constraint, else the category's rules default).
//! - `round.<n>.*` -- planets held and exhausted, their values, and every asset still held at the
//!   seat's last decision of the round (idle).
//! - `obj:<id>`, `final.*`, `support.*` -- objectives scored, end-of-game state, Support for the
//!   Throne held and given.
//!
//! Approximate by construction: a change caused by an opponent between two of a seat's decisions
//! lands on the seat's own last purpose, and changes after the seat's final decision of a round or
//! the game are not seen.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example economy_audit -- \
//!   --bundle <checkpoint> --temperature 0.001 --seeds 500 --diplomacy --rounds 4 --out rows.tsv
//! ```

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_engine::decision_context::ConstraintKind;
use ti4_engine::production::{Spend, planet_value};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlanetId, PlayerId};

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

/// `(seed, rotation, faction, metric, value)`.
type Row = (u64, usize, String, String, i64);

/// The purpose a decision serves, coarse enough to aggregate over.
fn category(purpose: &str) -> &'static str {
    let has = |needle: &str| purpose.contains(needle);
    if has("buy_token") || has("influence for a command token") {
        "tokens_bought"
    } else if has("place a structure") {
        "construction"
    } else if has("produce") || has("orbital_drop_deploy") {
        "production"
    } else if has("research") || has("Technology") {
        "tech"
    } else if has("score_objective") {
        "objectives"
    } else if has("vote") || has("AGENDA") {
        "agenda"
    } else if has("custodians") {
        "custodians"
    } else if has("activate_system") {
        "activation"
    } else if has("redistribute") || has("status") {
        "status_phase"
    } else if has("diplomacy") || has("end_turn") || has("transaction") || has("Trade") {
        "trade"
    } else if has("strategy token") {
        "secondary_other"
    } else if has("StrategyCard") {
        "strategy_card_other"
    } else if has("ActionCard") || has("action card") {
        "action_card"
    } else if has("Content(") || has("explore") || has("Explor") {
        "explore_content"
    } else if has("invasion") || has("ground") || has("combat") || has("move") {
        "military"
    } else {
        "other"
    }
}

/// The side a planet exhausted for this category pays with, when no constraint says.
const fn default_face(category: &str) -> Option<Spend> {
    match category.as_bytes() {
        b"production" | b"tech" => Some(Spend::Resources),
        b"tokens_bought" | b"agenda" | b"custodians" => Some(Spend::Influence),
        _ => None,
    }
}

#[derive(Clone)]
struct Snapshot {
    assets: [i64; 9],
    exhausted: BTreeSet<PlanetId>,
    controlled: BTreeSet<PlanetId>,
    purpose: String,
    planet_kind: Option<Spend>,
}

const ASSETS: [&str; 9] = [
    "tg", "comm", "tactic", "fleet", "strat", "ac", "tech", "units", "planets",
];

struct RoundUse {
    round: u32,
    capacity_resources: i64,
    capacity_influence: i64,
    held: BTreeSet<PlanetId>,
    used: BTreeSet<PlanetId>,
    used_resources: i64,
    used_influence: i64,
    voted: BTreeSet<PlanetId>,
    voted_influence: i64,
    last_assets: [i64; 9],
}

/// What the table has seen, refreshed at every decision any seat makes.
#[derive(Default)]
struct TableView {
    scored: BTreeMap<PlayerId, BTreeSet<String>>,
    victory_points: BTreeMap<PlayerId, i64>,
    support: BTreeMap<PlayerId, PlayerId>,
}

struct Tracked {
    inner: Box<dyn Decider>,
    seed: u64,
    rotation: usize,
    faction: String,
    last: Option<Snapshot>,
    round: Option<RoundUse>,
    metrics: BTreeMap<String, i64>,
    /// Every secret objective seen in this seat's hand, and the hand at its latest decision.
    secrets_held: BTreeSet<String>,
    secrets_now: BTreeSet<String>,
    rows: Rc<RefCell<Vec<Row>>>,
    table: Rc<RefCell<TableView>>,
}

fn purpose_of(choice: &Choice) -> (String, Option<Spend>) {
    let Some(context) = &choice.context else {
        return (format!("prompt:{}", choice.prompt), None);
    };
    let kind = context.outstanding.iter().find_map(|c| match c.kind {
        ConstraintKind::Resources => Some(Spend::Resources),
        ConstraintKind::Influence => Some(Spend::Influence),
        _ => None,
    });
    (format!("{:?}|{}", context.source, context.subtype), kind)
}

impl Tracked {
    fn add(&mut self, metric: String, amount: i64) {
        if amount != 0 {
            *self.metrics.entry(metric).or_default() += amount;
        }
    }

    fn flush_round(&mut self) {
        let Some(done) = self.round.take() else {
            return;
        };
        let r = done.round;
        let held = i64::try_from(done.held.len()).unwrap_or(0);
        let used = i64::try_from(done.used.len()).unwrap_or(0);
        // Idle: held all round and never seen exhausted.
        let idle = done.held.difference(&done.used).count();
        let idle_planets = i64::try_from(idle).unwrap_or(0);
        let idle_resources = done.capacity_resources - done.used_resources;
        let idle_influence = done.capacity_influence - done.used_influence;
        for (name, value) in [
            ("cap.R", done.capacity_resources),
            ("cap.I", done.capacity_influence),
            ("used.R", done.used_resources),
            ("used.I", done.used_influence),
            ("held.planets", held),
            ("used.planets", used),
            ("voted.I", done.voted_influence),
            ("idle.planets", idle_planets),
            ("idle.R", idle_resources),
            ("idle.I", idle_influence),
        ] {
            self.add(format!("round.{r}.{name}"), value);
        }
        for (index, asset) in ASSETS.iter().enumerate() {
            self.add(format!("round.{r}.end.{asset}"), done.last_assets[index]);
        }
        self.add(format!("round.{r}.seen"), 1);
    }

    fn observe(&mut self, choice: &Choice, seen: &SeatObservation<'_>) {
        {
            let mut table = self.table.borrow_mut();
            table.support.clone_from(seen.support_holders());
            for other in seen.players() {
                table.scored.insert(
                    other.clone(),
                    seen.scored_by(other)
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                );
                if let Some(seat) = seen.seat(other) {
                    table
                        .victory_points
                        .insert(other.clone(), i64::from(seat.victory_points));
                }
            }
        }
        let player = &choice.player;
        let Some(seat) = seen.seat(player) else {
            return;
        };
        self.secrets_now = seen
            .held_secrets()
            .iter()
            .map(ToString::to_string)
            .collect();
        self.secrets_held.extend(self.secrets_now.iter().cloned());
        let content = seen.content();
        let sources = seen.sources();
        let controlled: BTreeSet<PlanetId> = seen
            .controlled_planets(player)
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect();
        let exhausted: BTreeSet<PlanetId> = controlled
            .iter()
            .filter(|planet| !seen.planet_is_ready(planet))
            .cloned()
            .collect();
        let count = |n: usize| i64::try_from(n).unwrap_or(0);
        let assets = [
            i64::from(seat.trade_goods),
            i64::from(seat.commodities),
            i64::from(seat.tactic_tokens),
            i64::from(seat.fleet_tokens),
            i64::from(seat.strategic_tokens),
            count(seat.action_cards_held),
            count(seat.technologies.len()),
            count(seen.units_held(player)),
            count(controlled.len()),
        ];
        let value = |planet: &PlanetId, kind: Spend| planet_value(content, sources, planet, kind);

        // Round accounting: capacity at the first decision, use at any, idle at the last.
        let round = seen.round();
        if self.round.as_ref().is_some_and(|open| open.round != round) {
            self.flush_round();
        }
        if self.round.is_none() {
            self.round = Some(RoundUse {
                round,
                capacity_resources: controlled.iter().map(|p| value(p, Spend::Resources)).sum(),
                capacity_influence: controlled.iter().map(|p| value(p, Spend::Influence)).sum(),
                held: controlled.clone(),
                used: BTreeSet::new(),
                used_resources: 0,
                used_influence: 0,
                voted: BTreeSet::new(),
                voted_influence: 0,
                last_assets: assets,
            });
        }
        // Votes exhaust planets after the status phase readies them, and the agenda phase readies
        // them again (LRR 8): that influence never competes with the round's own spending, so an
        // agenda-phase reading is not counted as the round using a planet.
        let voting = seen.phase() == ti4_model::state::Phase::Agenda;
        if let Some(open) = self.round.as_mut().filter(|_| !voting) {
            for planet in &exhausted {
                if open.held.contains(planet) && open.used.insert(planet.clone()) {
                    open.used_resources += value(planet, Spend::Resources);
                    open.used_influence += value(planet, Spend::Influence);
                }
            }
            // Assets left over: read at the seat's last action-phase decision of the round, before
            // the status phase hands out tokens and readies planets.
            if seen.phase() == ti4_model::state::Phase::Action {
                open.last_assets = assets;
            }
        }
        if let Some(open) = self.round.as_mut().filter(|_| voting) {
            for planet in &exhausted {
                if open.held.contains(planet) && open.voted.insert(planet.clone()) {
                    open.voted_influence += value(planet, Spend::Influence);
                }
            }
        }

        // Flows since the previous decision, charged to its purpose.
        let (mut purpose, planet_kind) = purpose_of(choice);
        if purpose.ends_with("|pay_influence") || purpose.ends_with("|pay_resources") {
            if let Some(before) = &self.last {
                purpose.clone_from(&before.purpose);
            }
        }
        if let Some(before) = self.last.clone() {
            let cat = category(&before.purpose);
            for (index, asset) in ASSETS.iter().enumerate() {
                let delta = assets[index] - before.assets[index];
                if delta > 0 {
                    self.add(format!("gain.{asset}"), delta);
                    self.add(format!("gain.{asset}.{cat}"), delta);
                } else if delta < 0 {
                    self.add(format!("spend.{asset}"), -delta);
                    self.add(format!("spend.{asset}.{cat}"), -delta);
                }
            }
            let face = before.planet_kind.or_else(|| default_face(cat));
            for planet in exhausted
                .difference(&before.exhausted)
                .filter(|p| before.controlled.contains(*p))
            {
                match face {
                    Some(Spend::Resources) => {
                        self.add(format!("face.R.{cat}"), value(planet, Spend::Resources));
                    }
                    Some(Spend::Influence) => {
                        self.add(format!("face.I.{cat}"), value(planet, Spend::Influence));
                    }
                    None => self.add(format!("face.unknown_planets.{cat}"), 1),
                }
            }
        }
        self.add("decisions".to_owned(), 1);
        self.last = Some(Snapshot {
            assets,
            exhausted,
            controlled,
            purpose,
            planet_kind,
        });
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        self.flush_round();
        for secret in std::mem::take(&mut self.secrets_held) {
            self.metrics.insert(format!("secret_held:{secret}"), 1);
        }
        for secret in std::mem::take(&mut self.secrets_now) {
            self.metrics.insert(format!("secret_end:{secret}"), 1);
        }
        let mut rows = self.rows.borrow_mut();
        for (metric, value) in std::mem::take(&mut self.metrics) {
            rows.push((
                self.seed,
                self.rotation,
                self.faction.clone(),
                metric,
                value,
            ));
        }
    }
}

impl Decider for Tracked {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.observe(choice, seen);
        self.inner.choose_seeing(choice, seen)
    }

    fn stage_scores(&mut self, scores: Vec<(Option<f64>, Option<f64>)>) {
        self.inner.stage_scores(scores);
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one measurement, as in clearance_eval"
)]
fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let temperature: f64 = argument("--temperature").map_or(0.001, |value| {
        value
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite() && *parsed > 0.0)
            .unwrap_or_else(|| refuse("--temperature must be a positive number"))
    });
    let seeds: u64 = argument("--seeds").map_or(100, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds must be a number"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(900_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base must be a number"))
    });
    let out = argument("--out").unwrap_or_else(|| refuse("--out is required"));
    let diplomacy = std::env::args().any(|a| a == "--diplomacy");
    // Experiment: trade goods every seat gains as each round begins. 0 is the printed game.
    // Experiment: first round that pays the income; only 1 (default) or 2 are supported.
    let income_from_round: u32 = argument("--income-from-round").map_or(1, |value| {
        value
            .parse()
            .ok()
            .filter(|r| *r == 1 || *r == 2)
            .unwrap_or_else(|| refuse("--income-from-round must be 1 or 2"))
    });
    // Experiment: only this seat (e.g. seat0) receives the income; every seat when absent.
    let income_seat: Option<PlayerId> = argument("--income-seat").map(PlayerId::new);
    let income: i32 = argument("--income-tg").map_or(0, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--income-tg must be a whole number"))
    });
    let rounds: u32 = argument("--rounds").map_or(1, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--rounds must be a number"))
    });

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;

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

    println!("economy audit for {bundle_path}");
    println!(
        "  temperature {temperature}, seeds {seed_base}..{} x 6 rotations, round income {income} TG from round {income_from_round} to {}",
        seed_base + seeds,
        income_seat
            .as_ref()
            .map_or("every seat".to_owned(), ToString::to_string)
    );

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
    let harvest: Vec<Result<Vec<Row>, String>> = chunks
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let rows: Rc<RefCell<Vec<Row>>> = Rc::new(RefCell::new(Vec::new()));
            for (seed, rotation) in chunk {
                let seated: BTreeMap<PlayerId, FactionId> = players
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
                let table: Rc<RefCell<TableView>> = Rc::new(RefCell::new(TableView::default()));
                let rollout = ti4_training::rollout::play_with_round_income_and_decider_factory(
                    content,
                    &players,
                    &seated,
                    DEFAULT,
                    seed,
                    ti4_training::rollout::Horizon {
                        rounds,
                        steps: 10_000,
                    },
                    ti4_engine::opening::DEFAULT_REQUIREMENT,
                    &ti4_training::rollout::OpeningMap::PythonPool {
                        pool: Arc::clone(&pool),
                        tile_seed_offset: TILE_SEED_OFFSET,
                    },
                    ti4_training::rollout::SimulationCapabilities { diplomacy },
                    income,
                    income_from_round > 1,
                    income_seat.clone(),
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
                            let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
                                &local,
                                vocabulary.clone(),
                                row,
                                stream,
                            )
                            .at_temperature(temperature)
                            .from_setup(baseline)
                            .seat();
                            deciders.insert(
                                player.clone(),
                                Box::new(Tracked {
                                    inner: decider,
                                    seed,
                                    rotation,
                                    faction: seated[player].to_string(),
                                    last: None,
                                    round: None,
                                    metrics: BTreeMap::new(),
                                    secrets_held: BTreeSet::new(),
                                    secrets_now: BTreeSet::new(),
                                    rows: Rc::clone(&rows),
                                    table: Rc::clone(&table),
                                }),
                            );
                        }
                        Ok(deciders)
                    },
                );
                if let Some(error) = &rollout.error {
                    return Err(format!("game {seed}/{rotation} failed: {error}"));
                }
                let view = table.borrow();
                let mut rows = rows.borrow_mut();
                for seat in &rollout.seats {
                    let faction = seat.faction.to_string();
                    let mut push = |metric: String, value: i64| {
                        rows.push((seed, rotation, faction.clone(), metric, value));
                    };
                    let fp = &seat.episode.final_progress;
                    push("final.vp".to_owned(), fp.victory_points);
                    push(
                        "final.vp_seen".to_owned(),
                        view.victory_points.get(&seat.player).copied().unwrap_or(0),
                    );
                    push("final.cleared".to_owned(), i64::from(seat.episode.cleared));
                    push(
                        "income.recipient".to_owned(),
                        i64::from(
                            income != 0
                                && income_seat.as_ref().is_none_or(|only| *only == seat.player),
                        ),
                    );
                    push("final.tg".to_owned(), fp.trade_goods);
                    push("final.fleet_tokens".to_owned(), fp.fleet_tokens);
                    push(
                        "final.fleet_value_permille".to_owned(),
                        fp.fleet_value_permille,
                    );
                    push("final.planets_gained".to_owned(), fp.planets_gained);
                    push("final.units_gained".to_owned(), fp.units_gained);
                    push(
                        "final.technologies_gained".to_owned(),
                        fp.technologies_gained,
                    );
                    let held = view.support.values().filter(|h| **h == seat.player).count();
                    push("support.held".to_owned(), count_usize(held));
                    push(
                        "support.given".to_owned(),
                        i64::from(view.support.contains_key(&seat.player)),
                    );
                    for objective in view.scored.get(&seat.player).into_iter().flatten() {
                        push(format!("obj:{objective}"), 1);
                    }
                }
            }
            let taken = std::mem::take(&mut *rows.borrow_mut());
            Ok(taken)
        })
        .collect();

    let mut text = String::from("seed\trotation\tfaction\tmetric\tvalue\n");
    let mut n = 0usize;
    for chunk in harvest {
        for (seed, rotation, faction, metric, value) in chunk.unwrap_or_else(|error| refuse(&error))
        {
            text.push_str(&format!(
                "{seed}\t{rotation}\t{faction}\t{metric}\t{value}\n"
            ));
            n += 1;
        }
    }
    std::fs::write(&out, text).unwrap_or_else(|error| refuse(&format!("writing {out}: {error}")));
    println!(
        "  wrote {n} rows to {out} in {:.1}s",
        started.elapsed().as_secs_f64()
    );
}

fn count_usize(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}
