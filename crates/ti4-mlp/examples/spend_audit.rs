//! What does a policy spend its resources, influence, trade goods, commodities and command tokens on?
//!
//! Plays the same games `clearance_eval` plays (held-out pool, fixed seeds, one temperature) and
//! wraps every seat's bot in a tracker. At each of a seat's own decisions the tracker snapshots that
//! seat's public economy -- trade goods, commodities, the three token pools, and which of its
//! planets are exhausted -- and attributes every *decrease* since its previous decision to that
//! previous decision's purpose: its typed `DecisionContext` source and subtype, or the prompt when
//! the decision carries no context.
//!
//! A newly exhausted planet is counted as resources when the purpose decision's outstanding
//! constraint is resources, as influence when it is influence, and as "planet (other)" otherwise
//! (e.g. a tech-specialty or ability exhaust). Printed values only; attachments are not added.
//!
//! Approximate by construction: a decrease caused by an opponent between two of the seat's
//! decisions (an action card, a transaction it did not choose) lands on the seat's own last
//! purpose, and spending after the seat's final decision of the game is not seen. Increases are
//! ignored, so token redistribution shows up as a decrease in the pool tokens left.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example spend_audit -- \
//!   --bundle <checkpoint> --temperature 0.001 --seeds 100 --diplomacy --rounds 4 --out spend.tsv
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

/// `(faction, purpose, currency)` -> total spent.
type Ledger = BTreeMap<(String, String, &'static str), i64>;

struct Snapshot {
    trade_goods: i64,
    commodities: i64,
    tactic: i64,
    fleet: i64,
    strategic: i64,
    exhausted: BTreeSet<PlanetId>,
    controlled: BTreeSet<PlanetId>,
    purpose: String,
    planet_kind: Option<Spend>,
}

/// What the table has seen of objectives, refreshed at every decision any seat makes.
#[derive(Default)]
struct TableView {
    scored: BTreeMap<PlayerId, BTreeSet<String>>,
    revealed: BTreeSet<String>,
    victory_points: BTreeMap<PlayerId, i64>,
    /// Support for the Throne: owner -> holder, as last seen.
    support: BTreeMap<PlayerId, PlayerId>,
}

struct Tracked {
    inner: Box<dyn Decider>,
    faction: String,
    last: Option<Snapshot>,
    ledger: Rc<RefCell<Ledger>>,
    table: Rc<RefCell<TableView>>,
    round: Option<RoundUse>,
}

/// One seat's planets over one round: what it held at its first decision of the round, and every
/// one of those it was seen to have exhausted at any decision of the round.
struct RoundUse {
    round: u32,
    capacity_resources: i64,
    capacity_influence: i64,
    held: BTreeSet<PlanetId>,
    used: BTreeSet<PlanetId>,
    used_resources: i64,
    used_influence: i64,
}

impl Tracked {
    fn flush_round(&mut self) {
        if let Some(done) = self.round.take() {
            let mut ledger = self.ledger.borrow_mut();
            let key = format!("round{}", done.round);
            for (currency, amount) in [
                ("capacity_resources", done.capacity_resources),
                ("capacity_influence", done.capacity_influence),
                ("used_resources", done.used_resources),
                ("used_influence", done.used_influence),
                ("held_planets", i64::try_from(done.held.len()).unwrap_or(0)),
                ("used_planets", i64::try_from(done.used.len()).unwrap_or(0)),
                ("seat_rounds", 1),
            ] {
                *ledger
                    .entry((self.faction.clone(), key.clone(), currency))
                    .or_default() += amount;
            }
        }
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        self.flush_round();
    }
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
    fn observe(&mut self, choice: &Choice, seen: &SeatObservation<'_>) {
        {
            let mut table = self.table.borrow_mut();
            table
                .revealed
                .extend(seen.revealed_objectives().iter().map(ToString::to_string));
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
        let controlled: Vec<PlanetId> = seen
            .controlled_planets(player)
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect();
        let exhausted: BTreeSet<PlanetId> = controlled
            .iter()
            .filter(|planet| !seen.planet_is_ready(planet))
            .cloned()
            .collect();
        {
            let content = seen.content();
            let sources = seen.sources();
            let round = seen.round();
            if self.round.as_ref().is_some_and(|open| open.round != round) {
                self.flush_round();
            }
            if self.round.is_none() {
                let held: BTreeSet<PlanetId> = controlled.iter().cloned().collect();
                self.round = Some(RoundUse {
                    round,
                    capacity_resources: held
                        .iter()
                        .map(|p| planet_value(content, sources, p, Spend::Resources))
                        .sum(),
                    capacity_influence: held
                        .iter()
                        .map(|p| planet_value(content, sources, p, Spend::Influence))
                        .sum(),
                    held,
                    used: BTreeSet::new(),
                    used_resources: 0,
                    used_influence: 0,
                });
            }
            if let Some(open) = self.round.as_mut() {
                for planet in controlled.iter().filter(|p| !seen.planet_is_ready(p)) {
                    if open.held.contains(planet) && open.used.insert(planet.clone()) {
                        open.used_resources +=
                            planet_value(content, sources, planet, Spend::Resources);
                        open.used_influence +=
                            planet_value(content, sources, planet, Spend::Influence);
                    }
                }
            }
        }
        let (mut purpose, planet_kind) = purpose_of(choice);
        // A payment step names how it pays, not what for: charge it to the decision it pays for.
        if purpose.ends_with("|pay_influence") || purpose.ends_with("|pay_resources") {
            if let Some(before) = &self.last {
                purpose.clone_from(&before.purpose);
            }
        }
        let now = Snapshot {
            trade_goods: i64::from(seat.trade_goods),
            commodities: i64::from(seat.commodities),
            tactic: i64::from(seat.tactic_tokens),
            fleet: i64::from(seat.fleet_tokens),
            strategic: i64::from(seat.strategic_tokens),
            exhausted,
            controlled: controlled.iter().cloned().collect(),
            purpose,
            planet_kind,
        };
        if let Some(before) = &self.last {
            let mut ledger = self.ledger.borrow_mut();
            let mut add = |currency: &'static str, amount: i64| {
                if amount > 0 {
                    *ledger
                        .entry((self.faction.clone(), before.purpose.clone(), currency))
                        .or_default() += amount;
                }
            };
            add("trade_goods", before.trade_goods - now.trade_goods);
            add("commodities", before.commodities - now.commodities);
            add("tactic_tokens", before.tactic - now.tactic);
            add("fleet_tokens", before.fleet - now.fleet);
            add("strategy_tokens", before.strategic - now.strategic);
            // Only planets held at both decisions: a planet taken by invasion enters play exhausted
            // (LRR 37.4) without anything having been spent.
            for planet in now
                .exhausted
                .difference(&before.exhausted)
                .filter(|planet| before.controlled.contains(*planet))
            {
                let content = seen.content();
                let sources = seen.sources();
                match before.planet_kind {
                    Some(Spend::Resources) => {
                        add(
                            "resources",
                            planet_value(content, sources, planet, Spend::Resources),
                        );
                    }
                    Some(Spend::Influence) => {
                        add(
                            "influence",
                            planet_value(content, sources, planet, Spend::Influence),
                        );
                    }
                    None => add("planets_other", 1),
                }
            }
        }
        self.last = Some(now);
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

    println!("spend audit for {bundle_path}");
    println!(
        "  temperature {temperature}, seeds {seed_base}..{} x 6 rotations",
        seed_base + seeds
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
    let harvest: Vec<Result<(Ledger, BTreeMap<String, usize>), String>> = chunks
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let ledger: Rc<RefCell<Ledger>> = Rc::new(RefCell::new(BTreeMap::new()));
            let mut seats: BTreeMap<String, usize> = BTreeMap::new();
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
                let (rollout, _) =
                    ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
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
                        false,
                        |baselines| {
                            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> =
                                BTreeMap::new();
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
                                        faction: seated[player].to_string(),
                                        last: None,
                                        ledger: Rc::clone(&ledger),
                                        table: Rc::clone(&table),
                                        round: None,
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
                let mut ledger = ledger.borrow_mut();
                for objective in &view.revealed {
                    *ledger
                        .entry(("*".to_owned(), objective.clone(), "objective_revealed"))
                        .or_default() += 1;
                }
                for seat in &rollout.seats {
                    let faction = seat.faction.to_string();
                    *seats.entry(faction.clone()).or_default() += 1;
                    for objective in view.scored.get(&seat.player).into_iter().flatten() {
                        *ledger
                            .entry((faction.clone(), objective.clone(), "objective_scored"))
                            .or_default() += 1;
                    }
                    let held = view.support.values().filter(|h| **h == seat.player).count();
                    *ledger
                        .entry((
                            faction.clone(),
                            "support_for_the_throne_held".to_owned(),
                            "vp",
                        ))
                        .or_default() += i64::try_from(held).unwrap_or(0);
                    if let Some(owner_faction) = view
                        .support
                        .iter()
                        .find(|(owner, _)| **owner == seat.player)
                        .and_then(|(_, holder)| seated.get(holder))
                    {
                        *ledger
                            .entry((
                                faction.clone(),
                                format!("support_given_to:{owner_faction}"),
                                "vp",
                            ))
                            .or_default() += 1;
                    }
                    // VP the table never saw at a decision: scoring after the last decision.
                    let seen = view.victory_points.get(&seat.player).copied().unwrap_or(0);
                    *ledger
                        .entry((faction.clone(), "final_vp".to_owned(), "vp"))
                        .or_default() += seat.episode.final_progress.victory_points;
                    *ledger
                        .entry((faction, "vp_unseen_by_tracker".to_owned(), "vp"))
                        .or_default() += seat.episode.final_progress.victory_points - seen;
                }
            }
            let ledger = ledger.borrow().clone();
            Ok((ledger, seats))
        })
        .collect();

    let mut total: Ledger = BTreeMap::new();
    let mut seats: BTreeMap<String, usize> = BTreeMap::new();
    for chunk in harvest {
        let (ledger, counted) = chunk.unwrap_or_else(|error| refuse(&error));
        for (key, amount) in ledger {
            *total.entry(key).or_default() += amount;
        }
        for (faction, n) in counted {
            *seats.entry(faction).or_default() += n;
        }
    }

    let mut text = String::from("faction\tseats\tpurpose\tcurrency\ttotal\n");
    for ((faction, purpose, currency), amount) in &total {
        let n = seats.get(faction).copied().unwrap_or(0);
        text.push_str(&format!(
            "{faction}\t{n}\t{purpose}\t{currency}\t{amount}\n"
        ));
    }
    std::fs::write(&out, text).unwrap_or_else(|error| refuse(&format!("writing {out}: {error}")));
    println!(
        "  wrote {} rows to {out} in {:.1}s",
        total.len(),
        started.elapsed().as_secs_f64()
    );
}
