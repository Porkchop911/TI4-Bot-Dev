//! Two diagnostics for the economy: does the critic value money, and where does production stop?
//!
//! 1. **Critic probe.** At every `--probe-every`-th decision of each seat, the seat's critic input
//!    is built exactly as the bot builds it, `V(s)` is read, and then read again with one input
//!    nudged (more trade goods, one more token of each pool, one more planet, ...). The difference
//!    is how much the value head thinks that extra is worth, in reward units (roughly VP here).
//!    Only inputs the critic already carries can be nudged; anything it does not carry (ready
//!    resources, ready influence) is reported as absent.
//! 2. **Production audit.** Every `produce_unit` decision is logged: how many produce options
//!    were offered, what was chosen, and what the seat could still spend.
//!
//! Same games as `clearance_eval` / `economy_audit`: held-out pool, fixed seeds, greedy.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example economy_probe -- \
//!   --bundle <checkpoint> --seeds 100 --diplomacy --rounds 4 --out probe
//! ```
//! writes `probe.critic.tsv` and `probe.production.tsv`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_engine::production::Spend;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 0;

/// `(label, critic fact, delta)`.
const PROBES: [(&str, &str, f32); 11] = [
    ("tg+1", "critic-state:trade_goods", 1.0),
    ("tg+3", "critic-state:trade_goods", 3.0),
    ("tg+10", "critic-state:trade_goods", 10.0),
    ("commodities+3", "critic-state:commodities", 3.0),
    ("tactic+1", "critic-state:tactic_tokens", 1.0),
    ("strategy+1", "critic-state:strategic_tokens", 1.0),
    ("fleet+1", "critic-state:fleet_tokens", 1.0),
    ("planets+1", "critic-state:controlled_planets", 1.0),
    ("units+3", "critic-state:units_held", 3.0),
    ("tech+1", "critic-state:technologies", 1.0),
    ("action_cards+1", "critic-state:action_cards_held", 1.0),
];

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

struct Probe {
    inner: Box<dyn Decider>,
    actor: Rc<ti4_mlp::Actor>,
    vocabulary: ti4_policy::vocabulary::Vocabulary,
    row: ti4_mlp::FactionRow,
    seed: u64,
    rotation: usize,
    faction: String,
    every: usize,
    seen: usize,
    critic_rows: Rc<RefCell<Vec<String>>>,
    production_rows: Rc<RefCell<Vec<String>>>,
}

impl Probe {
    fn probe_critic(&self, choice: &Choice, seen: &SeatObservation<'_>) {
        let vector =
            ti4_policy::critic::critic_vector(seen, ti4_policy::critic::CriticFeatures::full());
        let input = ti4_mlp::CriticInput::new(&vector, &self.vocabulary);
        let Ok(base) = self.actor.value(&input, self.row) else {
            return;
        };
        let phase = format!("{:?}", seen.phase());
        let mut rows = self.critic_rows.borrow_mut();
        for (label, fact, delta) in PROBES {
            let column = i64::try_from(self.vocabulary.column_of(fact)).unwrap_or(-1);
            let shifted = input
                .with_shifted_value(column, delta)
                .and_then(|probe| self.actor.value(&probe, self.row).ok());
            let diff = shifted.map_or("absent".to_owned(), |v| format!("{:.5}", v - base));
            rows.push(format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{base:.5}\t{label}\t{diff}",
                self.seed,
                self.rotation,
                self.faction,
                seen.round(),
                phase,
                choice.options.len()
            ));
        }
    }

    fn log_production(&self, choice: &Choice, seen: &SeatObservation<'_>, answer: &ChoiceOption) {
        let produce = choice
            .options
            .iter()
            .filter(|o| o.kind == ti4_engine::production::PRODUCE_KIND)
            .count();
        let player = &choice.player;
        let resources = seen.available_spend(player, Spend::Resources);
        let trade_goods = seen.seat(player).map_or(0, |s| s.trade_goods);
        let cheapest = choice
            .options
            .iter()
            .filter_map(|o| o.payload.get("cost").and_then(serde_json::Value::as_i64))
            .min()
            .map_or("-".to_owned(), |c| c.to_string());
        let unit = answer
            .payload
            .get("unit")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("-");
        self.production_rows.borrow_mut().push(format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.seed,
            self.rotation,
            self.faction,
            seen.round(),
            choice.options.len(),
            produce,
            cheapest,
            resources,
            trade_goods,
            answer.kind,
            unit
        ));
    }
}

impl Decider for Probe {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if self.seen % self.every == 0 {
            self.probe_critic(choice, seen);
        }
        self.seen += 1;
        let answer = self.inner.choose_seeing(choice, seen)?;
        let producing = choice
            .context
            .as_ref()
            .is_some_and(|c| c.subtype == "produce_unit")
            || choice
                .options
                .iter()
                .any(|o| o.kind == ti4_engine::production::PRODUCE_KIND);
        if producing {
            self.log_production(choice, seen, &answer);
        }
        Ok(answer)
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
    let temperature: f64 = argument("--temperature").map_or(0.001, |v| {
        v.parse::<f64>()
            .ok()
            .filter(|t| t.is_finite() && *t > 0.0)
            .unwrap_or_else(|| refuse("--temperature must be a positive number"))
    });
    let seeds: u64 = argument("--seeds").map_or(100, |v| {
        v.parse()
            .unwrap_or_else(|_| refuse("--seeds must be a number"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(900_000_000, |v| {
        v.parse()
            .unwrap_or_else(|_| refuse("--seed-base must be a number"))
    });
    let every: usize = argument("--probe-every").map_or(5, |v| {
        v.parse()
            .ok()
            .filter(|n| *n > 0)
            .unwrap_or_else(|| refuse("--probe-every must be a positive number"))
    });
    let out = argument("--out").unwrap_or_else(|| refuse("--out is required"));
    let diplomacy = std::env::args().any(|a| a == "--diplomacy");
    let rounds: u32 = argument("--rounds").map_or(1, |v| {
        v.parse()
            .unwrap_or_else(|_| refuse("--rounds must be a number"))
    });

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;
    for (_, fact, _) in PROBES {
        let assigned = vocabulary.is_assigned(fact);
        println!(
            "  {fact:<40} {}",
            if assigned {
                "in vocabulary"
            } else {
                "NOT ASSIGNED (routes to an OOV column)"
            }
        );
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
    let harvest: Vec<Result<(Vec<String>, Vec<String>), String>> = chunks
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let critic_rows: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
            let production_rows: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
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
                                    Box::new(Probe {
                                        inner: decider,
                                        actor: Rc::clone(&local),
                                        vocabulary: vocabulary.clone(),
                                        row,
                                        seed,
                                        rotation,
                                        faction: seated[player].to_string(),
                                        every,
                                        seen: 0,
                                        critic_rows: Rc::clone(&critic_rows),
                                        production_rows: Rc::clone(&production_rows),
                                    }),
                                );
                            }
                            Ok(deciders)
                        },
                    );
                if let Some(error) = &rollout.error {
                    return Err(format!("game {seed}/{rotation} failed: {error}"));
                }
            }
            let critic = std::mem::take(&mut *critic_rows.borrow_mut());
            let production = std::mem::take(&mut *production_rows.borrow_mut());
            Ok((critic, production))
        })
        .collect();

    let mut critic = String::from(
        "seed\trotation\tfaction\tround\tphase\toptions\tbase_value\tprobe\tdelta_value\n",
    );
    let mut production = String::from(
        "seed\trotation\tfaction\tround\toptions\tproduce_options\tcheapest_cost\tavailable_resources\ttrade_goods\tchosen_kind\tchosen_unit\n",
    );
    for chunk in harvest {
        let (c, p) = chunk.unwrap_or_else(|error| refuse(&error));
        for line in c {
            critic.push_str(&line);
            critic.push('\n');
        }
        for line in p {
            production.push_str(&line);
            production.push('\n');
        }
    }
    let critic_path = format!("{out}.critic.tsv");
    let production_path = format!("{out}.production.tsv");
    std::fs::write(&critic_path, critic).unwrap_or_else(|e| refuse(&format!("{critic_path}: {e}")));
    std::fs::write(&production_path, production)
        .unwrap_or_else(|e| refuse(&format!("{production_path}: {e}")));
    println!(
        "  wrote {critic_path} and {production_path} in {:.1}s",
        started.elapsed().as_secs_f64()
    );
}
