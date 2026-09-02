//! Build the permanent per-faction corpus of opening trajectories that cleared.
//!
//! ```text
//! play seeds x rotations
//!   -> keep seats that cleared the bar
//!   -> reject trajectories containing a wasted activation
//!   -> write one file per faction, plus a manifest
//! ```
//!
//! # What a trajectory is
//!
//! Its specification: seed, rotation, faction, and the option id chosen at every non-forced
//! decision. Not features. The engine is deterministic given a seed, so replaying those ids
//! reproduces the game exactly and features can be recomputed under whatever model is training. See
//! `ti4_mlp::positive_corpus` for why that matters.
//!
//! # Temperature is a real choice here, not a default
//!
//! At the greedy limit this collects the champion reproducing itself, and a policy cloned from its
//! own successes learns nothing it did not already know. A sampling temperature above zero explores
//! around the champion and the clearance filter throws away whatever did not work, which is the
//! cross-entropy method: sample, keep the successes, refit. `--temperature` therefore defaults to
//! 0.5 rather than to the evaluation setting, and a run may pass several.
//!
//! Every trajectory is verified to have cleared under the policy that produced it, so a hotter
//! temperature costs yield rather than quality.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example build_positive_corpus -- \
//!   --bundle out/checkpoints/sweep-A-250/checkpoint-14476 \
//!   --seeds 2000 --temperatures 0.25,0.5,0.75 --out out/corpus/positive
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::{ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_mlp::positive_corpus::{Note, Trajectory, actions_taken, wasted_activations, write_line};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

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

fn number<T: std::str::FromStr>(flag: &str, fallback: T) -> T {
    argument(flag).map_or(fallback, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{flag} expects a number")))
    })
}

/// Records what a seat did, and never changes it.
struct Watching {
    inner: Box<dyn Decider>,
    log: Rc<RefCell<Vec<Note>>>,
}

impl Watching {
    fn record(&self, choice: &Choice, chosen: &ChoiceOption) {
        // Forced decisions are skipped, matching what the recorder and the trainers do: with one
        // legal option the choice carries no preference, and keeping it here would put an action in
        // the corpus that no policy could have got wrong.
        if choice.options.len() < 2 {
            return;
        }
        let head = ti4_mlp::Actor::resolve_head(ti4_policy::learned::decision_head(choice));
        self.log.borrow_mut().push(Note {
            head: head.to_owned(),
            chosen: chosen.id.clone(),
            declined: chosen.is_decline(),
        });
    }
}

impl Decider for Watching {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let chosen = self.inner.choose(choice)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let chosen = self.inner.choose_seeing(choice, seen)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }
}

/// What one game produced for every seat.
struct Played {
    faction: String,
    cleared: bool,
    planets: usize,
    systems: usize,
    units_ok: bool,
    notes: Vec<Note>,
}

#[expect(
    clippy::too_many_lines,
    reason = "one pass: playing the games, filtering them and writing them are one thing"
)]
fn main() {
    let bundle_path = argument("--bundle")
        .unwrap_or_else(|| refuse("--bundle is required: demonstrations come from a policy"));
    let seeds: u64 = number("--seeds", 500);
    let seed_base: u64 = number("--seed-base", 700_000_000);
    let out = argument("--out").unwrap_or_else(|| "out/corpus/positive".to_owned());
    let temperatures: Vec<f64> = argument("--temperatures").map_or_else(
        || vec![0.5],
        |text| {
            text.split(',')
                .map(|piece| {
                    piece
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite() && *value > 0.0)
                        .unwrap_or_else(|| refuse("--temperatures expects positive numbers"))
                })
                .collect()
        },
    );

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;

    // The Train pool. A corpus of demonstrations is training data, so it must not be built from the
    // maps the policy is scored on.
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            ti4_sim::artifacts::read_and_verify_pool_role(
                std::path::Path::new(&pool_path),
                &[ti4_sim::artifacts::ArtifactRole::Train],
            )
            .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}"))),
        ))
        .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();

    let git_commit = std::env::var("GIT_COMMIT")
        .unwrap_or_else(|_| refuse("GIT_COMMIT is required so the corpus can be traced"));

    println!("positive corpus from {bundle_path}");
    println!("  maps         {pool_path} (Train)");
    println!("  seeds        {seed_base}..{}", seed_base + seeds);
    println!(
        "  temperatures {}",
        temperatures
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!();

    let started = std::time::Instant::now();
    let mut kept: BTreeMap<String, Vec<Trajectory>> = BTreeMap::new();
    let mut seats_total = 0usize;
    let mut failed_bar = 0usize;
    let mut rejected_waste = 0usize;
    // Waste split by outcome, because the two mean different things. An activation wasted *after*
    // the bar is met costs nothing: the reward has stopped caring and the seat has nothing left to
    // do. One wasted *before* is a turn that could have taken the third planet. A single rate over
    // all seats mixes them, so a policy that clears less often has fewer spare activations to waste
    // and can score better on the combined figure by playing worse.
    let mut waste_when_cleared = 0usize;
    let mut waste_when_failed = 0usize;
    // The four quantities that have to be reported together, because two of them were compared
    // across runs as though they were the same number and they are not: the expected COUNT of
    // wasted activations per seat-game, and the INCIDENCE of seat-games with at least one. A policy
    // that wastes rarely but repeatedly can beat one that wastes often but once on the first and
    // lose on the second, so an ordering established on one says nothing about the other.
    let mut tactical_total = 0usize;
    let mut waste_count_total = 0usize;
    let mut any_waste_seats = 0usize;
    let mut cleared_total = 0usize;
    let explain: usize = number("--explain", 0);
    let mut explained = 0usize;

    for temperature in temperatures.iter().copied() {
        // Thousandths, and the stream offset is taken from the same integer. A replay reproduces
        // the other five seats from this value, so it has to be exact rather than nearly right.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a positive temperature under a thousand"
        )]
        let milli = (temperature * 1_000.0) as u64;
        let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
            .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
            .collect();
        let workers = rayon::current_num_threads().max(1);
        let per_worker = jobs.len().div_ceil(workers).max(1);

        let harvest: Vec<Result<Vec<(u64, usize, Played)>, String>> = jobs
            .chunks(per_worker)
            .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
            .collect::<Vec<_>>()
            .into_par_iter()
            .map(|(local, chunk)| {
                let local = Rc::new(local);
                let mut out = Vec::new();
                for (seed, rotation) in chunk {
                    // One log per seat. The stream is per seat by construction, which is what makes
                    // the wasted-activation segmentation attributable -- the event log carries names
                    // without an owner and could not say whose activation it was.
                    let mut logs: BTreeMap<PlayerId, Rc<RefCell<Vec<Note>>>> = BTreeMap::new();
                    let (_events, _setup, assignments, openings, _final) =
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
                                        .ok_or_else(|| format!("{player} has no baseline"))?;
                                    // The stream is offset by the temperature so a second pass at a
                                    // different temperature explores a different set of lines rather
                                    // than re-drawing the same ones.
                                    let stream = seed
                                        .wrapping_mul(1_000_003)
                                        .wrapping_add(u64::try_from(index).unwrap_or(0))
                                        .wrapping_add(milli);
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
                                        Box::new(Watching {
                                            inner: decider,
                                            log,
                                        }),
                                    );
                                }
                                Ok(deciders)
                            },
                        )?;

                    for (player, opening) in &openings {
                        let Some(faction) = assignments.get(player) else {
                            return Err(format!("{player} has no faction"));
                        };
                        let notes = logs
                            .get(player)
                            .ok_or_else(|| format!("{player} has no log"))?
                            .borrow()
                            .clone();
                        out.push((
                            seed,
                            rotation,
                            Played {
                                faction: faction.to_string(),
                                cleared: opening.cleared(),
                                planets: opening.planets_gained,
                                systems: opening.systems,
                                units_ok: opening.units_ok(),
                                notes,
                            },
                        ));
                    }
                }
                Ok(out)
            })
            .collect();

        for chunk in harvest {
            for (seed, rotation, played) in chunk.unwrap_or_else(|error| refuse(&error)) {
                seats_total += 1;
                let waste_count = wasted_activations(&played.notes);
                let wasted = waste_count > 0;
                let tactical = played
                    .notes
                    .iter()
                    .filter(|note| note.head == "turn" && note.chosen == "tactical")
                    .count();
                tactical_total += tactical;
                waste_count_total += waste_count;
                any_waste_seats += usize::from(wasted);
                cleared_total += usize::from(played.cleared);
                if wasted {
                    if played.cleared {
                        waste_when_cleared += 1;
                    } else {
                        waste_when_failed += 1;
                    }
                }
                if !played.cleared {
                    failed_bar += 1;
                    continue;
                }
                if wasted {
                    rejected_waste += 1;
                    if explain > 0 && explained < explain {
                        explained += 1;
                        println!(
                            "  -- rejected {seed}/{rotation} {} ({} decisions) --",
                            played.faction,
                            played.notes.len()
                        );
                        for note in &played.notes {
                            let mark = if note.declined { " (decline)" } else { "" };
                            println!("       {:<12} {}{mark}", note.head, note.chosen);
                        }
                    }
                    continue;
                }
                let trajectory = Trajectory {
                    seed,
                    rotation,
                    faction: played.faction.clone(),
                    temperature_milli: milli,
                    planets: played.planets,
                    systems: played.systems,
                    units_ok: played.units_ok,
                    actions: actions_taken(&played.notes),
                    decisions: played
                        .notes
                        .iter()
                        .map(|note| note.chosen.clone())
                        .collect(),
                };
                kept.entry(played.faction).or_default().push(trajectory);
            }
        }
    }

    // ---- write -------------------------------------------------------------------------------
    let directory = std::path::Path::new(&out);
    std::fs::create_dir_all(directory)
        .unwrap_or_else(|error| refuse(&format!("creating {}: {error}", directory.display())));

    // Admission is binary -- cleared, and no wasted activation -- so this table reports size, not
    // quality. There is no ranking among admitted trajectories.
    println!("  faction      kept   mean actions   mean decisions");
    let mut written = 0usize;
    for (faction, trajectories) in &kept {
        let mut body = String::new();
        for trajectory in trajectories {
            let line = write_line(trajectory)
                .unwrap_or_else(|error| refuse(&format!("writing {faction}: {error}")));
            body.push_str(&line);
            body.push('\n');
        }
        let path = directory.join(format!("{faction}.corpus"));
        std::fs::write(&path, body)
            .unwrap_or_else(|error| refuse(&format!("writing {}: {error}", path.display())));
        written += trajectories.len();

        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let mean_actions = trajectories.iter().map(|t| t.actions).sum::<usize>() as f64
            / trajectories.len().max(1) as f64;
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let mean_decisions = trajectories
            .iter()
            .map(|t| t.decisions.len())
            .sum::<usize>() as f64
            / trajectories.len().max(1) as f64;
        println!(
            "  {faction:<10} {:>6}   {mean_actions:>12.1}   {mean_decisions:>14.1}",
            trajectories.len()
        );
    }

    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let waste_share = rejected_waste as f64 / seats_total.max(1) as f64 * 100.0;
    let manifest = format!(
        "schema ti4-positive-corpus-v1\nbundle {bundle_path}\npool {pool_path}\ncommit \
         {git_commit}\nseeds {seed_base}..{}\ntemperatures {}\nseat_games {seats_total}\nkept \
         {written}\nfailed_bar {failed_bar}\nrejected_wasted_activation {rejected_waste}\n",
        seed_base + seeds,
        temperatures
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    let manifest_path = directory.join("manifest.txt");
    std::fs::write(&manifest_path, manifest)
        .unwrap_or_else(|error| refuse(&format!("writing the manifest: {error}")));

    println!();
    println!("  {seats_total} seat-games in {:.1?}", started.elapsed());
    println!("  {failed_bar} failed the bar");
    println!(
        "  {rejected_waste} rejected for a wasted activation ({waste_share:.2}% of all seats)"
    );
    let cleared_seats = seats_total - failed_bar;
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let of = |part: usize, whole: usize| -> f64 {
        if whole == 0 {
            0.0
        } else {
            part as f64 / whole as f64 * 100.0
        }
    };
    println!(
        "  waste when cleared {waste_when_cleared}/{cleared_seats} ({:.2}%), when failed {waste_when_failed}/{failed_bar} ({:.2}%)",
        of(waste_when_cleared, cleared_seats),
        of(waste_when_failed, failed_bar)
    );
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    {
        let seats = seats_total.max(1) as f64;
        println!();
        println!(
            "  TABLE  clear {:.2}%  tactical/seat {:.3}  waste/seat {:.3}  any-waste {:.2}%  waste/tactical {:.3}",
            of(cleared_total, seats_total),
            tactical_total as f64 / seats,
            waste_count_total as f64 / seats,
            of(any_waste_seats, seats_total),
            waste_count_total as f64 / tactical_total.max(1) as f64
        );
    }
    println!(
        "  {written} trajectories written to {}",
        directory.display()
    );
}
