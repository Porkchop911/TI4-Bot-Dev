//! Does a seat ever score a public *and* a secret objective in one status phase?
//!
//! LRR 61.6 grants two independent allowances in the status phase: one public objective and one
//! secret. `ScoringWindow` used to spend a single per-player allowance on whichever came first, so
//! scoring a public silently consumed the secret slot and the pair could never happen. This asks
//! the empirical question the unit test cannot: across real greedy games, does the pair actually
//! occur?
//!
//! Recorded **at the decision**, not from the final state. `state.scored_objectives` is a
//! per-player set with no round in it, so a final reading cannot tell one status phase from the
//! next -- a seat that scored a public in round two and a secret in round three looks identical to
//! one that scored both at once. The `Choice` carries everything needed instead:
//! `Choice::player` names the seat, its `DecisionContext` carries the round and the `score_objective`
//! subtype that marks status timing (the event-scoped path is secret-only and uses
//! `score_secret_objective`), and the chosen option id is the alias. Grouping those by
//! (seat, round) is exactly "in the same status phase".
//!
//! Inference is CPU-only under §7.1, so this runs beside a training job.

use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::Decider;
use ti4_model::content_types::{ContentType, DEFAULT};
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

/// The prompt `ScoringWindow::choice` builds, and the context subtype that marks status timing.
const SCORE_PROMPT: &str = "score an objective";
const STATUS_SUBTYPE: &str = "score_objective";

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(argument) = args.next() {
        if argument == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

/// One objective scored, by whom and when.
struct Scored {
    faction: String,
    round: u32,
    alias: String,
    public: bool,
}

/// A decider that answers exactly as the one it wraps and writes down what it scored.
struct Watching {
    inner: Box<dyn Decider>,
    faction: String,
    log: std::rc::Rc<std::cell::RefCell<Vec<Scored>>>,
}

impl Watching {
    fn record(&self, choice: &Choice, chosen: &ti4_engine::choice::ChoiceOption) {
        if choice.prompt != SCORE_PROMPT || chosen.is_decline() {
            return;
        }
        let Some(context) = choice.context.as_ref() else {
            return;
        };
        // Only status timing has two allowances to keep apart. The event-scoped path is secret-only
        // and would inflate the count with pairs LRR 61.6 has nothing to say about.
        if context.subtype != STATUS_SUBTYPE {
            return;
        }
        let content = ContentStore::embedded();
        let public = content
            .get(ContentType::PublicObjectives, &chosen.id)
            .is_some();
        let secret = content
            .get(ContentType::SecretObjectives, &chosen.id)
            .is_some();
        // An alias in neither corpus, or somehow in both, would be silently miscounted as secret by
        // a bare `!public`. Drop it loudly instead of guessing.
        if public == secret {
            refuse(&format!(
                "{}: alias {} is in {} objective corpus",
                self.faction,
                chosen.id,
                if public { "both" } else { "neither" }
            ));
        }
        self.log.borrow_mut().push(Scored {
            faction: self.faction.clone(),
            round: context.round,
            alias: chosen.id.clone(),
            public,
        });
    }
}

impl Decider for Watching {
    fn choose(
        &mut self,
        choice: &Choice,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        let chosen = self.inner.choose(choice)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &ti4_engine::choice::SeatObservation<'_>,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        let chosen = self.inner.choose_seeing(choice, seen)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let temperature: f64 = argument("--temperature").map_or(0.001, |value| {
        value
            .parse()
            .ok()
            .filter(|parsed: &f64| *parsed > 0.0)
            .unwrap_or_else(|| refuse("--temperature expects a positive number"))
    });
    let seeds: u64 = argument("--seeds").map_or(50, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds expects a positive integer"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(690_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base expects an integer"))
    });
    let rounds: u32 = argument("--rounds").map_or(4, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--rounds expects a positive integer"))
    });
    ti4_tensor::configure_deterministic(20_260_821)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = std::rc::Rc::new(
        loaded
            .actor
            .inference_copy()
            .to_device(ti4_tensor::Device::Cpu),
    );

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

    let content = ContentStore::embedded();
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();

    println!("double-score probe (LRR 61.6)");
    println!("  bundle      {bundle_path}");
    println!(
        "  sample      {seeds} seeds x {} rotations, {rounds} round(s), temperature {temperature}",
        FACTIONS.len()
    );

    let mut pairs = 0usize;
    let mut publics = 0usize;
    let mut secrets = 0usize;
    let mut scoring_phases = 0usize;
    let mut games = 0usize;
    let mut by_faction: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: Vec<String> = Vec::new();

    for seed in seed_base..seed_base + seeds {
        for rotation in 0..FACTIONS.len() {
            let log: std::rc::Rc<std::cell::RefCell<Vec<Scored>>> =
                std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            ti4_training::rollout::audit_game_with_deciders(
                content,
                &factions,
                DEFAULT,
                seed,
                rotation,
                ti4_training::rollout::Horizon {
                    rounds,
                    steps: 200_000,
                },
                &ti4_training::rollout::OpeningMap::PythonPool {
                    pool: Arc::clone(&pool),
                    tile_seed_offset: TILE_SEED_OFFSET,
                },
                |seated, baselines| {
                    let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                    for (index, (player, faction)) in seated.iter().enumerate() {
                        let row = ti4_mlp::FactionRow::of(faction.as_str())
                            .map_err(|error| format!("{player}: {error}"))?;
                        let stream = seed
                            .wrapping_mul(1_000_003)
                            .wrapping_add(u64::try_from(index).unwrap_or(0));
                        let baseline = baselines
                            .get(player)
                            .copied()
                            .ok_or_else(|| format!("{player} has no setup baseline"))?;
                        let (decider, _status) =
                            ti4_mlp::bot::MlpBot::sharing(&actor, vocabulary.clone(), row, stream)
                                .at_temperature(temperature)
                                .from_setup(baseline)
                                .seat();
                        deciders.insert(
                            player.clone(),
                            Box::new(Watching {
                                inner: decider,
                                faction: faction.to_string(),
                                log: std::rc::Rc::clone(&log),
                            }),
                        );
                    }
                    Ok(deciders)
                },
            )
            .unwrap_or_else(|error| refuse(&error));

            games += 1;
            // Grouped by (seat, round): one status phase per round, so a seat appearing twice in a
            // group scored twice in the same one.
            let mut grouped: BTreeMap<(String, u32), (usize, usize)> = BTreeMap::new();
            for scored in log.borrow().iter() {
                let entry = grouped
                    .entry((scored.faction.clone(), scored.round))
                    .or_insert((0, 0));
                if scored.public {
                    entry.0 += 1;
                    publics += 1;
                } else {
                    entry.1 += 1;
                    secrets += 1;
                }
            }
            scoring_phases += grouped.len();
            for ((faction, round), (public, secret)) in grouped {
                if public > 0 && secret > 0 {
                    pairs += 1;
                    *by_faction.entry(faction.clone()).or_default() += 1;
                    if examples.len() < 12 {
                        let aliases: Vec<&str> = log
                            .borrow()
                            .iter()
                            .filter(|s| s.faction == faction && s.round == round)
                            .map(|s| if s.public { "public" } else { "secret" })
                            .collect();
                        let names: Vec<String> = log
                            .borrow()
                            .iter()
                            .filter(|s| s.faction == faction && s.round == round)
                            .map(|s| s.alias.clone())
                            .collect();
                        examples.push(format!(
                            "  seed {seed} rotation {rotation} round {round}: {faction} scored {} ({})",
                            names.join(" + "),
                            aliases.join(" + ")
                        ));
                    }
                }
            }
        }
    }

    println!("\n  {games} games, {scoring_phases} seat-status-phases with at least one score");
    println!("  {publics} public and {secrets} secret objectives scored");
    println!("  {pairs} seat-status-phases scored BOTH a public and a secret");
    if pairs == 0 {
        println!("\n  none found -- either the 61.6 allowances are still shared, or the sample is");
        println!("  too small to contain a seat that could satisfy both at once.");
        return;
    }
    println!("\n  by faction");
    for (faction, count) in &by_faction {
        println!("    {faction:<10} {count}");
    }
    println!("\n  examples");
    for line in &examples {
        println!("{line}");
    }
}
