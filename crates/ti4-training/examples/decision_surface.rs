//! Deterministic empirical census of the non-forced decision surface (OBS-002a).
//!
//! This records which `Decider` entry point the engine actually invokes, then groups decisions by
//! learned head and the set of option kinds on offer. It is evidence of exercised behavior, not a
//! substitute for the source inventory: a rare producer can be absent from a bounded run.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice};
use ti4_engine::opening::DEFAULT_REQUIREMENT;
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::bot::ScoredBot;
use ti4_policy::learned::decision_head;
use ti4_training::rollout::{Horizon, OpeningMap, play_with_deciders};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    path: &'static str,
    head: &'static str,
    kinds: String,
}

#[derive(Debug, Default)]
struct Row {
    decisions: usize,
    options: usize,
    prompt_samples: BTreeSet<String>,
}

struct Watching {
    inner: ScoredBot,
    rows: Arc<Mutex<BTreeMap<Key, Row>>>,
}

impl Watching {
    fn note(&self, choice: &Choice, path: &'static str) {
        if choice.options.len() <= 1 {
            return;
        }
        let kinds: BTreeSet<&str> = choice
            .options
            .iter()
            .map(|option| option.kind.as_str())
            .collect();
        let key = Key {
            path,
            head: decision_head(choice),
            kinds: kinds.into_iter().collect::<Vec<_>>().join("+"),
        };
        let mut rows = self.rows.lock().expect("decision census lock");
        let row = rows.entry(key).or_default();
        row.decisions += 1;
        row.options += choice.options.len();
        if row.prompt_samples.len() < 3 {
            row.prompt_samples.insert(choice.prompt.clone());
        }
    }
}

impl Decider for Watching {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.note(choice, "viewless");
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &ti4_engine::choice::SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.note(choice, "observed");
        self.inner.choose_seeing(choice, seen)
    }
}

fn argument(name: &str, fallback: u64) -> Result<u64, String> {
    let args: Vec<String> = std::env::args().collect();
    match args.iter().position(|argument| argument == name) {
        Some(index) => args
            .get(index + 1)
            .ok_or_else(|| format!("{name} needs a value"))?
            .parse()
            .map_err(|_| format!("{name} needs an unsigned integer")),
        None => Ok(fallback),
    }
}

fn main() -> Result<(), String> {
    let games = argument("--games", 4)?;
    let rounds = u32::try_from(argument("--rounds", 4)?)
        .map_err(|_| "--rounds does not fit u32".to_owned())?;
    if games == 0 || rounds == 0 {
        return Err("--games and --rounds must be positive".to_owned());
    }

    let players: Vec<PlayerId> = (0..6)
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let factions: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .cloned()
        .zip(FACTIONS.into_iter().map(FactionId::new))
        .collect();
    let rows = Arc::new(Mutex::new(BTreeMap::<Key, Row>::new()));

    for seed in 0..games {
        let deciders: BTreeMap<PlayerId, Box<dyn Decider>> = players
            .iter()
            .enumerate()
            .map(|(index, player)| {
                let bot_seed = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                let watcher = Watching {
                    inner: ScoredBot::new(bot_seed),
                    rows: Arc::clone(&rows),
                };
                (player.clone(), Box::new(watcher) as Box<dyn Decider>)
            })
            .collect();
        let rollout = play_with_deciders(
            ContentStore::embedded(),
            &players,
            &factions,
            FULL,
            seed,
            Horizon::rounds(rounds),
            DEFAULT_REQUIREMENT,
            &OpeningMap::RustVaried,
            deciders,
        );
        if let Some(error) = rollout.error {
            return Err(format!("seed {seed}: {error}"));
        }
    }

    let rows = rows.lock().expect("decision census lock");
    let total: usize = rows.values().map(|row| row.decisions).sum();
    let viewless: usize = rows
        .iter()
        .filter(|(key, _)| key.path == "viewless")
        .map(|(_, row)| row.decisions)
        .sum();
    println!("games={games} rounds={rounds} non_forced={total} viewless={viewless}");
    println!("path\thead\tkinds\tdecisions\tmean_options\tprompt_samples");
    for (key, row) in rows.iter() {
        #[expect(clippy::cast_precision_loss, reason = "bounded diagnostic counts")]
        let mean_options = row.options as f64 / row.decisions as f64;
        let prompts = row
            .prompt_samples
            .iter()
            .map(|prompt| prompt.replace(['\t', '\n'], " "))
            .collect::<Vec<_>>()
            .join(" | ");
        println!(
            "{}\t{}\t{}\t{}\t{mean_options:.2}\t{}",
            key.path, key.head, key.kinds, row.decisions, prompts
        );
    }
    Ok(())
}
