//! Play one evaluation game and log every decision as it is made, to find where a game stalls.
//!
//! Same seating, map and bot settings as `clearance_eval` (BF-22 wide-roster diagnosis). Each
//! decision prints `elapsed seat faction subtype options` to stderr before the bot answers, so a
//! stall shows as the last line; the interval between lines shows a slow decision.
//!
//! ```text
//! wide_stall_probe --bundle <checkpoint> --seed <u64> --rotation <n> [--roster wide]
//!   [--rounds 4] [--temperature 0.25] [--map-pool out/pools/full_np8_12_holdout.json]
//! ```

use std::collections::BTreeMap;
use std::io::Write as _;
use std::sync::Arc;
use std::time::Instant;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 0;

struct Logged {
    inner: Box<dyn Decider>,
    faction: String,
    started: Instant,
}

impl Logged {
    fn note(&self, choice: &Choice) {
        let subtype = choice
            .context
            .as_ref()
            .map_or("none", |context| context.subtype.as_str());
        let mut err = std::io::stderr().lock();
        let _ = writeln!(
            err,
            "{:>9.3} {} {} {} {} {}",
            self.started.elapsed().as_secs_f64(),
            choice.player,
            self.faction,
            choice.prompt,
            subtype,
            choice.options.len()
        );
        let _ = err.flush();
    }
}

impl Decider for Logged {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.note(choice);
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.note(choice);
        self.inner.choose_seeing(choice, seen)
    }
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + 1))
        .cloned()
}

fn refuse(reason: &str) -> ! {
    eprintln!("REFUSED: {reason}");
    std::process::exit(2)
}

fn number<T: std::str::FromStr>(name: &str, fallback: T) -> T {
    argument(name).map_or(fallback, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{name} expects a number, got {value:?}")))
    })
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let pool_path = argument("--map-pool")
        .unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
    let seed: u64 = number("--seed", 900_000_000);
    let rotation: usize = number("--rotation", 0);
    let rounds: u32 = number("--rounds", 4);
    let temperature: f64 = number("--temperature", 0.25);
    let roster = argument("--roster").map_or(ti4_engine::seating::FactionRoster::Wide, |value| {
        ti4_training::rollout::parse_roster(&value).unwrap_or_else(|error| refuse(&error))
    });

    ti4_tensor::configure_deterministic(20_260_922)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let bundle = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = bundle.vocabulary;
    let actor = std::rc::Rc::new(bundle.actor);
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            std::fs::read(&pool_path)
                .unwrap_or_else(|error| refuse(&format!("reading {pool_path}: {error}"))),
        ))
        .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let players: Vec<PlayerId> = (0..6)
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let factions =
        ti4_training::rollout::game_factions(content, roster, &FACTIONS, &players, DEFAULT, seed)
            .unwrap_or_else(|error| refuse(&error));
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
    eprintln!("seed {seed} rotation {rotation}: {seated:?}");
    let started = Instant::now();
    let (rollout, _) = ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
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
        ti4_training::rollout::SimulationCapabilities { diplomacy: true },
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
                let (decider, _status) =
                    ti4_mlp::bot::MlpBot::sharing(&actor, vocabulary.clone(), row, stream)
                        .at_temperature(temperature)
                        .from_setup(baseline)
                        .seat();
                deciders.insert(
                    player.clone(),
                    Box::new(Logged {
                        inner: decider,
                        faction: seated[player].to_string(),
                        started,
                    }),
                );
            }
            Ok(deciders)
        },
    );
    eprintln!(
        "done in {:.1}s, error {:?}",
        started.elapsed().as_secs_f64(),
        rollout.error
    );
}
