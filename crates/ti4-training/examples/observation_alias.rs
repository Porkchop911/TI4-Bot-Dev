//! Does the observation distinguish situations the rules distinguish? (OBS-002b)
//!
//! The completeness question that needs no teacher, no training and no reward: **if two different
//! situations produce the same observation but offer different legal actions, the observation is
//! missing state the rules depend on.** No optimiser and no network size fixes that; the two
//! situations are one situation as far as the model is concerned.
//!
//! `separability` and `inert_audit` already ask the option-side version of this — whether the
//! options of a single choice can be told apart. This asks the state-side version, across
//! decisions, which nothing currently covers.
//!
//! # What is hashed
//!
//! `explicit_choice_features` returns one vector per option built over a shared per-choice context.
//! The state the model sees is therefore the **option-invariant** part: keys carrying the same value
//! on every option of the choice. That subset is hashed together with the learned head.
//!
//! # What a collision does and does not prove
//!
//! The obvious test -- "same observation, different legal actions, therefore incomplete" -- is
//! WRONG for this architecture, and the first version of this tool got it wrong. The model scores
//! each option from state features crossed with that option's own features, so the option set is
//! part of what it sees. Two decisions sharing a state context but offering different options are
//! distinguishable to the model through the options themselves. That is normal, not a defect.
//!
//! The collisions that matter are those where the state context AND the option set are both
//! identical. Only then has the model been given one input for two situations. Even those are only
//! CANDIDATE aliases: two genuinely identical positions should collide, and separating a true alias
//! from an honest repeat needs a value signal this diagnostic deliberately does not have.
//!
//! Absence of collisions in a bounded run proves nothing either: it is a lower bound on aliasing,
//! never an upper one.
//!
//! ```text
//! cargo run -p ti4-training --example observation_alias --release -- --games 4 --rounds 4
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_engine::opening::DEFAULT_REQUIREMENT;
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::bot::ScoredBot;
use ti4_policy::features::{explicit_choice_features, names_of, seat_facts, value_of};
use ti4_policy::learned::decision_head;
use ti4_training::rollout::{Horizon, OpeningMap, play_with_deciders};

static SHARED_STATS: Mutex<Vec<(usize, usize, usize)>> = Mutex::new(Vec::new());

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

/// One observed situation, keyed by what the model can see before reading options.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Observation {
    head: &'static str,
    /// The option-invariant features, rendered stably. This is the state the model sees.
    state: String,
}

#[derive(Debug, Default)]
struct Seen {
    decisions: usize,
    /// Every distinct legal option set observed under this one observation. More than one entry
    /// means the same observation was offered different legal actions.
    option_sets: BTreeSet<String>,
    prompts: BTreeSet<String>,
    /// The seat's own facts at the moment of the decision, as ground truth. If these differ inside
    /// a group that shares an observation AND an option set, the observation provably lost them:
    /// the engine had the distinction and the model did not receive it. This is what turns a
    /// candidate into a proof, and it needs no value signal to do it.
    truths: BTreeSet<String>,
}

struct Watching {
    inner: ScoredBot,
    player: PlayerId,
    seen: Arc<Mutex<BTreeMap<Observation, Seen>>>,
}

impl Watching {
    /// The option-invariant subset of the choice's features, rendered stably.
    ///
    /// Values are formatted to six decimals so that float noise cannot split an observation that is
    /// otherwise identical; the alternative, hashing raw bits, would report aliasing as absent
    /// merely because two equal quantities were computed by different routes.
    fn state_key(seen: &SeatObservation<'_>, choice: &Choice, player: &PlayerId) -> Option<String> {
        let vectors = explicit_choice_features(seen, choice, player, &[]);
        let first = vectors.first()?;
        let mut shared: BTreeMap<String, f64> = BTreeMap::new();
        for name in names_of(first) {
            let Some(value) = value_of(first, &name) else {
                continue;
            };
            if vectors
                .iter()
                .all(|vector| value_of(vector, &name).is_some_and(|other| other == value))
            {
                shared.insert(name, value);
            }
        }
        // Instrument check: how much of the state key survives, and does the PROMPT reach it?
        // If prompt features are crossed with option identity they are not option-invariant, and
        // this key would understate what the model sees -- which would make the aliasing below an
        // artefact of the measurement rather than a property of the observation.
        let prompts = shared.keys().filter(|n| n.starts_with("prompt")).count();
        let total_first = names_of(first).len();
        SHARED_STATS
            .lock()
            .expect("stats")
            .push((total_first, shared.len(), prompts));
        Some(
            shared
                .into_iter()
                .map(|(name, value)| format!("{name}={value:.6}"))
                .collect::<Vec<_>>()
                .join("|"),
        )
    }

    fn record(&self, choice: &Choice, seen: &SeatObservation<'_>) {
        // Forced decisions carry no information about preference and are excluded, matching the
        // census in `decision_surface`.
        if choice.options.len() < 2 {
            return;
        }
        let Some(state) = Self::state_key(seen, choice, &self.player) else {
            return;
        };
        let key = Observation {
            head: decision_head(choice),
            state,
        };
        let options: Vec<&str> = {
            let mut ids: Vec<&str> = choice.options.iter().map(|o| o.id.as_str()).collect();
            ids.sort_unstable();
            ids
        };
        let mut table = self.seen.lock().expect("census lock");
        let row = table.entry(key).or_default();
        row.decisions += 1;
        row.option_sets.insert(options.join(","));
        row.prompts
            .insert(choice.prompt.chars().take(70).collect::<String>());
        row.truths.insert(
            seat_facts(seen, &self.player)
                .iter()
                .map(|(name, value)| format!("{name}={value:.3}"))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
}

impl Decider for Watching {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        // A viewless ask has no seat observation to hash, so it cannot be censused here. OBS-002a
        // names all fifteen; they are migration work, and their absence here is a consequence of
        // that, not a separate finding.
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.record(choice, seen);
        self.inner.choose_seeing(choice, seen)
    }
}

fn number(flag: &str, fallback: usize) -> usize {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next().and_then(|v| v.parse().ok()).unwrap_or(fallback);
        }
    }
    fallback
}

fn main() {
    let games = number("--games", 4);
    let rounds = u32::try_from(number("--rounds", 4)).unwrap_or(4);
    let content = ContentStore::embedded();
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            ti4_sim::artifacts::read_and_verify_pool_role(
                std::path::Path::new("out/pools/full_np8_12_train.json"),
                &[ti4_sim::artifacts::ArtifactRole::Train],
            )
            .expect("train pool"),
        ))
        .expect("parse pool"),
    );

    let census: Arc<Mutex<BTreeMap<Observation, Seen>>> = Arc::new(Mutex::new(BTreeMap::new()));
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();

    println!("observation aliasing census (OBS-002b)");
    println!("  games   {games} x {rounds} rounds");
    println!("  hashing the option-invariant features plus the learned head");
    println!();

    for game in 0..games {
        let seed = 700_000_000 + game as u64;
        let seated: BTreeMap<PlayerId, FactionId> = players
            .iter()
            .enumerate()
            .map(|(index, player)| (player.clone(), FactionId::new(FACTIONS[index])))
            .collect();
        let deciders: BTreeMap<PlayerId, Box<dyn Decider>> = players
            .iter()
            .map(|player| {
                let decider: Box<dyn Decider> = Box::new(Watching {
                    inner: ScoredBot::new(seed),
                    player: player.clone(),
                    seen: Arc::clone(&census),
                });
                (player.clone(), decider)
            })
            .collect();
        let _ = play_with_deciders(
            content,
            &players,
            &seated,
            FULL,
            seed,
            Horizon {
                rounds,
                steps: 60_000,
            },
            DEFAULT_REQUIREMENT,
            &OpeningMap::PythonPool {
                pool: Arc::clone(&pool),
                tile_seed_offset: 0,
            },
            deciders,
        );
    }

    let table = census.lock().expect("census lock");
    let decisions: usize = table.values().map(|row| row.decisions).sum();
    let repeated: Vec<(&Observation, &Seen)> = table
        .iter()
        .filter(|(_, row)| row.decisions > 1)
        .collect::<Vec<_>>();
    // Same state context AND one single option set, seen more than once: the model received one
    // input for two or more situations. These are the candidates.
    let candidates: Vec<(&Observation, &Seen)> = table
        .iter()
        .filter(|(_, row)| row.decisions > 1 && row.option_sets.len() == 1)
        .collect::<Vec<_>>();
    // Same state context but different options. NOT a defect here: the options are part of the
    // model's input, so it can still tell these apart.
    let differing: usize = table
        .values()
        .filter(|row| row.option_sets.len() > 1)
        .count();

    {
        let stats = SHARED_STATS.lock().expect("stats");
        let n = stats.len().max(1);
        let per_option: usize = stats.iter().map(|s| s.0).sum::<usize>() / n;
        let shared: usize = stats.iter().map(|s| s.1).sum::<usize>() / n;
        let with_prompt = stats.iter().filter(|s| s.2 > 0).count();
        println!(
            "  INSTRUMENT: mean features on option 0 = {per_option}, mean option-invariant = {shared}"
        );
        println!(
            "  INSTRUMENT: decisions whose state key contains any prompt feature = {with_prompt} of {}",
            stats.len()
        );
        println!();
    }
    println!("  decisions recorded          {decisions}");
    println!("  distinct observations       {}", table.len());
    println!("  observations seen more once {}", repeated.len());
    println!(
        "  CANDIDATE ALIASES (same state context AND same option set)  {}",
        candidates.len()
    );
    println!("  same context, different options (expected, not a defect)    {differing}");
    let proven: Vec<&(&Observation, &Seen)> = candidates
        .iter()
        .filter(|(_, row)| row.truths.len() > 1)
        .collect();
    println!(
        "  PROVEN (candidate whose seat facts differed between decisions)  {}",
        proven.len()
    );
    println!();
    if !proven.is_empty() {
        println!("=== proven aliases: the engine distinguished these, the model did not ===");
        let mut worst = proven.clone();
        worst.sort_by_key(|(_, row)| std::cmp::Reverse(row.truths.len()));
        for (key, row) in worst.iter().take(6) {
            println!(
                "  head {:<10} {} decisions, {} DISTINCT seat states, one input",
                key.head,
                row.decisions,
                row.truths.len()
            );
            for prompt in row.prompts.iter().take(1) {
                println!("      prompt: {prompt}");
            }
            for truth in row.truths.iter().take(3) {
                println!(
                    "      seat:   {}",
                    truth.chars().take(96).collect::<String>()
                );
            }
        }
        println!();
    }

    if candidates.is_empty() {
        println!("  No candidate aliases in this sample. That is a lower bound, not a clean bill.");
    } else {
        println!("=== candidate aliases, most repeated first ===");
        let mut worst = candidates;
        worst.sort_by_key(|(_, row)| std::cmp::Reverse(row.decisions));
        for (key, row) in worst.iter().take(12) {
            println!(
                "  head {:<12} one option set, {} decisions on one input",
                key.head, row.decisions
            );
            for prompt in row.prompts.iter().take(2) {
                println!("      prompt: {prompt}");
            }
            for set in row.option_sets.iter().take(2) {
                println!(
                    "      options: {}",
                    set.chars().take(90).collect::<String>()
                );
            }
        }
    }

    println!();
    println!("=== per head ===");
    println!("  head           observations   repeated   candidate aliases");
    let mut by_head: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    for (key, row) in table.iter() {
        let slot = by_head.entry(key.head).or_default();
        slot.0 += 1;
        slot.1 += usize::from(row.decisions > 1);
        slot.2 += usize::from(row.decisions > 1 && row.option_sets.len() == 1);
    }
    for (head, (total, repeated, aliased)) in by_head {
        println!("  {head:<14} {total:>10}   {repeated:>8}   {aliased:>7}");
    }
}
