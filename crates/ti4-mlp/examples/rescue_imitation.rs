//! Learn from the openings the champion could have cleared but did not.
//!
//! The reachability search finds a clearing continuation for about 65% of the champion's failures,
//! by sampling the champion's *own* policy at a raised temperature. Those lines are the strongest
//! supervision available anywhere in this project: they are known-good decisions, in positions the
//! champion actually reaches, that it currently assigns too little probability to.
//!
//! # What is cloned, and what is deliberately not
//!
//! Only the **first decision where the rescue diverged from the champion**.
//!
//! Cloning a whole rescue trajectory would be a mistake that looks like more data. A rescue is
//! sampled at temperature 2.5, so most of its decisions are *worse* than the champion's and it
//! cleared anyway; cloning all of them teaches the policy to be more random, which is the opposite
//! of the intent. Restricting to divergences is not enough either, because a divergence late in a
//! line is confounded by every earlier one.
//!
//! The first divergence is the only decision where the two lines are comparable. Up to that point
//! both followed identical play, so the position, the legal options and their features are the
//! same object — the champion's recorded step *is* the rescue's step, differing only in which
//! option was taken. One sample per rescued failure, and every one of them a controlled comparison.
//!
//! # What this cannot establish
//!
//! That the cloned action *caused* the clearance. The rescue diverged again after this point and
//! cleared at the end of a different line. What is true is narrower and still useful: at this
//! position the champion chose one option, and a continuation that cleared chose another.
//!
//! Behaviour cloning here is distillation against a one-hot teacher, so it reuses `distill::train`
//! rather than reimplementing cross-entropy.

use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_mlp::bundle::CriticMode;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

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

struct Setup {
    actor: std::rc::Rc<ti4_mlp::Actor>,
    vocabulary: ti4_policy::vocabulary::Vocabulary,
    pool: Arc<ti4_sim::MapPool>,
    content: &'static ContentStore,
    factions: Vec<FactionId>,
    critic_mode: CriticMode,
}

/// One seat's game: whether it cleared, and every decision it took.
type Seat = (bool, Vec<ti4_mlp::bot::PpoRecord>);

/// Play one game with every seat recording, optionally sampling a single seat hotter.
fn play(
    setup: &Setup,
    seed: u64,
    rotation: usize,
    hot: Option<(&PlayerId, f64, u64)>,
) -> BTreeMap<PlayerId, (FactionId, Seat)> {
    let handles: std::rc::Rc<
        std::cell::RefCell<
            BTreeMap<PlayerId, std::rc::Rc<std::cell::RefCell<Vec<ti4_mlp::bot::PpoRecord>>>>,
        >,
    > = std::rc::Rc::new(std::cell::RefCell::new(BTreeMap::new()));
    let seated_handles = std::rc::Rc::clone(&handles);

    let (_events, _picks, assignments, openings, _final) =
        ti4_training::rollout::audit_game_with_deciders(
            setup.content,
            &setup.factions,
            DEFAULT,
            seed,
            rotation,
            ti4_training::rollout::Horizon {
                rounds: 1,
                steps: 200_000,
            },
            &ti4_training::rollout::OpeningMap::PythonPool {
                pool: Arc::clone(&setup.pool),
                tile_seed_offset: TILE_SEED_OFFSET,
            },
            |seated, baselines| {
                let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                for (index, (player, faction)) in seated.iter().enumerate() {
                    let row = ti4_mlp::FactionRow::of(faction.as_str())
                        .map_err(|error| format!("{player}: {error}"))?;
                    let baseline = baselines
                        .get(player)
                        .copied()
                        .ok_or_else(|| format!("{player} has no setup baseline"))?;
                    let ordinary = seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(u64::try_from(index).unwrap_or(0));
                    let (temperature, stream) = match hot {
                        Some((target, temperature, stream)) if target == player => {
                            (temperature, stream)
                        }
                        _ => (1.0, ordinary),
                    };
                    let bot = ti4_mlp::bot::MlpBot::sharing(
                        &setup.actor,
                        setup.vocabulary.clone(),
                        row,
                        stream,
                    )
                    .from_setup(baseline)
                    .at_temperature(temperature)
                    .recording_ppo(setup.critic_mode);
                    seated_handles
                        .borrow_mut()
                        .insert(player.clone(), bot.ppo_records());
                    let (decider, _status) = bot.seat();
                    deciders.insert(player.clone(), decider);
                }
                Ok(deciders)
            },
        )
        .unwrap_or_else(|error| refuse(&error));

    let recorded = handles.borrow();
    openings
        .into_iter()
        .map(|(player, opening)| {
            let faction = assignments
                .get(&player)
                .cloned()
                .unwrap_or_else(|| FactionId::new(""));
            let steps = recorded
                .get(&player)
                .map(|handle| handle.borrow().clone())
                .unwrap_or_default();
            (player, (faction, (opening.cleared(), steps)))
        })
        .collect()
}

/// The first decision at which two lines from the same position disagree.
///
/// Positional comparison is only valid while the lines are identical, which is exactly up to this
/// index — so the search stops at the first mismatch rather than collecting all of them. The option
/// count is checked too: if the two lines are somehow offered different option sets at the same
/// index they are no longer the same decision, and comparing them would be comparing nothing.
fn first_divergence(
    champion: &[ti4_mlp::bot::PpoRecord],
    rescue: &[ti4_mlp::bot::PpoRecord],
) -> Option<usize> {
    champion
        .iter()
        .zip(rescue)
        .position(|(a, b)| {
            a.step.options.len() == b.step.options.len()
                && a.step.head == b.step.head
                && a.step.chosen != b.step.chosen
        })
        .filter(|index| {
            champion[..*index]
                .iter()
                .zip(&rescue[..*index])
                .all(|(a, b)| a.step.chosen == b.step.chosen)
        })
}

#[expect(
    clippy::too_many_lines,
    reason = "one collection pass; the search and what counts as a usable divergence belong together"
)]
fn main() {
    let bundle_path = argument("--bundle")
        .unwrap_or_else(|| refuse("--bundle is required: rescues belong to a specific champion"));
    let out = argument("--out").unwrap_or_else(|| "out/checkpoints/rescue".to_owned());
    let seeds: u64 = argument("--seeds").map_or(120, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds expects a positive integer"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(710_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base expects an unsigned integer"))
    });
    let attempts: usize = argument("--attempts").map_or(40, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--attempts expects a positive integer"))
    });
    let temperature: f64 = argument("--temperature").map_or(2.5, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--temperature expects a number"))
    });

    ti4_tensor::configure_deterministic(20_260_821)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));

    let mut trained = loaded.actor;
    let setup = Setup {
        actor: std::rc::Rc::new(trained.inference_copy().to_device(ti4_tensor::Device::Cpu)),
        vocabulary: loaded.vocabulary,
        pool: Arc::new(
            ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
                .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
        ),
        content: ContentStore::embedded(),
        factions: FACTIONS.iter().map(|name| FactionId::new(*name)).collect(),
        critic_mode: loaded.critic_mode,
    };

    println!("rescue imitation");
    println!("  bundle      {bundle_path}");
    println!(
        "  sample      {seeds} seeds x {} rotations, one round",
        FACTIONS.len()
    );
    println!("  search      {attempts} replays per failed seat at temperature {temperature}");

    let mut samples: Vec<ti4_mlp::distill::Sample> = Vec::new();
    let mut failures = 0usize;
    let mut rescued = 0usize;
    let mut no_divergence = 0usize;
    let mut by_head: BTreeMap<usize, usize> = BTreeMap::new();
    let started = std::time::Instant::now();

    for seed in seed_base..seed_base + seeds {
        for rotation in 0..FACTIONS.len() {
            let champion = play(&setup, seed, rotation, None);
            let failed: Vec<PlayerId> = champion
                .iter()
                .filter(|(_, (_, (cleared, _)))| !cleared)
                .map(|(player, _)| player.clone())
                .collect();

            for player in failed {
                failures += 1;
                let Some((_, (_, champion_steps))) = champion.get(&player) else {
                    continue;
                };
                if champion_steps.is_empty() {
                    continue;
                }
                for attempt in 0..attempts {
                    let stream = seed
                        .wrapping_mul(7_777_777)
                        .wrapping_add(u64::try_from(attempt).unwrap_or(0))
                        .wrapping_add(0x5EED_0000_0000);
                    let outcome =
                        play(&setup, seed, rotation, Some((&player, temperature, stream)));
                    let Some((_, (cleared, rescue_steps))) = outcome.get(&player) else {
                        continue;
                    };
                    if !cleared {
                        continue;
                    }
                    rescued += 1;
                    match first_divergence(champion_steps, rescue_steps) {
                        Some(index) => {
                            // The champion's own recorded step: at the first divergence the two
                            // lines are the same position, so its options and features are the
                            // rescue's too. Only the target differs.
                            let step = &champion_steps[index].step;
                            let target = rescue_steps[index].step.chosen;
                            let mut teacher = vec![0.0; step.options.len()];
                            if let Some(slot) = teacher.get_mut(target) {
                                *slot = 1.0;
                            } else {
                                continue;
                            }
                            *by_head.entry(step.head).or_default() += 1;
                            samples.push(ti4_mlp::distill::Sample {
                                row: step.row,
                                head: step.head,
                                options: step.options.clone(),
                                teacher,
                            });
                        }
                        // The rescue cleared without ever choosing differently, so the difference
                        // came from elsewhere -- opponents responding to the same actions, or the
                        // seat's own later decisions past where the lines stayed aligned. There is
                        // nothing here to imitate.
                        None => no_divergence += 1,
                    }
                    break;
                }
            }
        }
    }

    println!();
    println!("  failures    {failures}");
    println!("  rescued     {rescued}");
    println!("  usable      {} first-divergence samples", samples.len());
    println!("  discarded   {no_divergence} rescues that never chose differently");
    println!("  by head");
    for (head, count) in &by_head {
        let name = ti4_mlp::heads().get(*head).copied().unwrap_or("?");
        println!("    {name:<12} {count}");
    }
    println!("  collected   in {:.1}s", started.elapsed().as_secs_f64());

    if samples.len() < 32 {
        refuse("too few rescue samples to train on; widen --seeds or --attempts");
    }

    // A held-out tenth, so the fit is reported against decisions it did not see. `train` requires a
    // validation split and selects the earliest epoch that minimises it.
    let split = samples.len() / 10;
    let (validation, train_samples) = samples.split_at(split.max(1));
    println!();
    println!(
        "  cloning     {} train, {} validation",
        train_samples.len(),
        validation.len()
    );

    let settings = ti4_mlp::distill::Settings {
        max_epochs: argument("--epochs").map_or(8, |value| {
            value
                .parse()
                .unwrap_or_else(|_| refuse("--epochs expects a positive integer"))
        }),
        ..ti4_mlp::distill::Settings::default()
    };
    let outcome =
        ti4_mlp::distill::train(&mut trained, train_samples, validation, settings, |epoch| {
            println!(
                "    epoch {:>2}  train KL {:>8.5}  validation KL {:>8.5}",
                epoch.number, epoch.train_kl, epoch.validation_kl
            );
        })
        .unwrap_or_else(|error| refuse(&format!("cloning: {error}")));
    println!("  selected    epoch {}", outcome.selected);

    let slots_text = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));
    let actor = trained.to_device(ti4_tensor::Device::Cpu);
    let bundle = ti4_mlp::bundle::write(
        std::path::Path::new(&out),
        &actor,
        &slots_text,
        setup.critic_mode,
        &ti4_mlp::bundle::Provenance {
            source: format!(
                "rescue imitation over {} first-divergence decisions from {bundle_path}",
                train_samples.len()
            ),
            git_commit: std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unrecorded".to_owned()),
            update: 0,
        },
    )
    .unwrap_or_else(|error| refuse(&format!("writing the bundle: {error}")));
    println!("  written     {}", bundle.directory.display());
}
