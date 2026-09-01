//! Which single decision, changed, would have cleared a failed opening?
//!
//! # The question
//!
//! For a failed line `a_0 .. a_n`, replay `a_0 .. a_{i-1}` exactly, substitute one legal alternate
//! at index `i`, and let the seat play on under its own policy. If that clears, index `i` is
//! *causally* implicated:
//!
//! ```text
//! P(clear | do(a_i = a'))
//! ```
//!
//! This is not what the previous attempt at failure-directed training measured. `rescue_imitation`
//! sampled the whole line at temperature 2.5 and cloned the first decision where the rescue
//! diverged. At 2.5 a rescue diverges almost immediately, so "first divergence" collapsed onto
//! decision zero: two thirds of its training targets were the strategy card pick, credited for an
//! outcome twenty decisions later. Held out, it cost 2.2 points and every faction regressed
//! (`339f42d`). The fix recorded there is this: stop *finding* the divergence by sampling, and
//! *impose* it one index at a time.
//!
//! # Why this is affordable
//!
//! `failure_census` priced it. A failed seat-game runs 52 decisions and 251 alternates, at 128
//! games a second — about 22 minutes for a full enumeration of every failure a policy produces in
//! 10,800 seat-games. So the enumeration is exhaustive. Nothing is capped and nothing is sampled,
//! which is what keeps the `do()` semantics exact rather than approximate.
//!
//! # What makes the replay sound
//!
//! - **Greedy.** At the evaluation temperature the policy is effectively deterministic, so the
//!   prefix replays to the same position every time. A stochastic prefix would make "everything
//!   else held fixed" false, which is the whole claim.
//! - **The other five seats are never touched.** They play their own policy on their own streams.
//!   Perturbing them would change the contention the failing seat faced, and a position cleared
//!   against different opposition is a different position.
//! - **Only the substituted decision is imposed.** After index `i` the seat decides for itself. The
//!   state has diverged, so it is answering *different* questions from the ones it answered in the
//!   original line — "replaying its actions" after the intervention would not be a continuation of
//!   anything.
//! - **The option list is checked, not assumed.** If the number of options at index `i` differs
//!   from what pass one recorded, the prefix did not reproduce and the job is refused rather than
//!   silently substituting into a different decision.
//!
//! # What a repair does and does not prove
//!
//! That this alternate, followed by the policy's own play, cleared. Not that it is the uniquely
//! best action, and not that the original was the only mistake. That is exactly why the intended
//! consumer is a *preference* target -- rank the repair above the action that failed -- rather than
//! a one-hot label asserting the repair is correct and every other option wrong.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example counterfactual_repair -- \
//!   --bundle out/checkpoints/sweep-A-250/checkpoint-14476 --seeds 300 --max-failures 200
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::{ChoiceOption, Decider, IllegalChoice, SeatObservation};
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

/// One decision of the target seat's original line.
#[derive(Clone)]
struct Recorded {
    head: String,
    /// How many options were offered. The replay checks this before substituting.
    options: usize,
    /// Which one the policy took.
    chosen: usize,
}

/// What the target seat's decider is doing on this pass.
enum Mode {
    /// Write down the line.
    Record(Rc<RefCell<Vec<Recorded>>>),
    /// Take option `alternate` at decision `index`, and decide normally everywhere else.
    Substitute {
        index: usize,
        alternate: usize,
        expect_options: usize,
        /// Set to the option count actually offered, when it is not the one recorded. The
        /// substitution is then discarded: substituting by position into a decision that is not the
        /// decision recorded would be intervening on a different question.
        mismatch: Rc<RefCell<Option<usize>>>,
    },
}

/// The target seat's decider for one replay.
struct Intervene {
    inner: Box<dyn Decider>,
    seen: usize,
    mode: Mode,
}

impl Intervene {
    fn answer(
        &mut self,
        choice: &Choice,
        delegate: impl FnOnce(&mut Box<dyn Decider>) -> Result<ChoiceOption, IllegalChoice>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let index = self.seen;
        self.seen += 1;

        match &self.mode {
            Mode::Record(log) => {
                let chosen = delegate(&mut self.inner)?;
                let position = choice
                    .options
                    .iter()
                    .position(|option| *option == chosen)
                    .unwrap_or(0);
                let head = ti4_mlp::Actor::resolve_head(ti4_policy::learned::decision_head(choice));
                log.borrow_mut().push(Recorded {
                    head: head.to_owned(),
                    options: choice.options.len(),
                    chosen: position,
                });
                Ok(chosen)
            }
            Mode::Substitute {
                index: target,
                alternate,
                expect_options,
                mismatch,
            } => {
                if index != *target {
                    return delegate(&mut self.inner);
                }
                // The prefix is supposed to have reproduced exactly. If the decision here is not
                // the decision that was recorded, substituting by position would be substituting
                // into a different question, so the run is marked and discarded.
                if choice.options.len() != *expect_options {
                    *mismatch.borrow_mut() = Some(choice.options.len());
                    return delegate(&mut self.inner);
                }
                choice.options.get(*alternate).cloned().ok_or_else(|| {
                    IllegalChoice::DeciderFailed {
                        player: choice.player.clone(),
                        prompt: choice.prompt.clone(),
                        reason: format!(
                            "alternate {alternate} is outside the {} options offered",
                            choice.options.len()
                        ),
                    }
                })
            }
        }
    }
}

impl Decider for Intervene {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.answer(choice, |inner| inner.choose(choice))
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.answer(choice, |inner| inner.choose_seeing(choice, seen))
    }
}

/// A failed seat-game to investigate.
#[derive(Clone)]
struct Target {
    seed: u64,
    rotation: usize,
    faction: String,
    planet_short: usize,
    system_short: usize,
    units_ok: bool,
}

/// What the enumeration found for one failure.
struct Repair {
    target: Target,
    decisions: usize,
    /// Alternates tried, and how many of them cleared.
    tried: usize,
    cleared: usize,
    /// Substitutions discarded because the decision at that index was not the one recorded.
    discarded: usize,
    /// Indices that had at least one clearing alternate, with the head at that index.
    repairing: Vec<(usize, String)>,
    /// Every index, so the analysis can be done outside this tool.
    per_index: Vec<IndexResult>,
}

/// What enumerating one index found.
struct IndexResult {
    index: usize,
    head: String,
    options: usize,
    /// How many alternates at this index cleared. One is strong evidence about *this* action;
    /// twenty is strong evidence the original was bad and weak evidence about which repair matters.
    cleared: usize,
    /// The Pareto-best deficit any non-clearing alternate here reached. `usize::MAX` when none
    /// improved on anything.
    best_planet_short: usize,
    best_system_short: usize,
}

/// Everything one replay needs, so a rayon worker can be handed it whole.
struct Table<'a> {
    content: &'static ContentStore,
    factions: &'a [FactionId],
    pool: &'a Arc<ti4_sim::MapPool>,
    vocabulary: &'a ti4_policy::vocabulary::Vocabulary,
    temperature: f64,
}

/// What one seat's opening came to.
///
/// The deficit is kept for interventions that did *not* clear, because "did not clear" is the least
/// informative thing that can be said about them. An intervention that took the seat from two
/// planets short to one short did something, and a second-order search wants to branch from those
/// rather than from all 251. Pareto comparison on this triple is what makes that frontier cheap.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Outcome {
    cleared: bool,
    planet_short: usize,
    system_short: usize,
    units_ok: bool,
}

/// Play one game, with the named seat's decider built by `wrap`.
///
/// Returns that seat's opening outcome. The other five are always plain policy seats.
fn play<W>(
    table: &Table<'_>,
    actor: &Rc<ti4_mlp::Actor>,
    seed: u64,
    rotation: usize,
    faction: &str,
    wrap: W,
) -> Result<Outcome, String>
where
    W: FnOnce(Box<dyn Decider>) -> Box<dyn Decider>,
{
    let wrap = RefCell::new(Some(wrap));
    let (_events, _setup, assignments, openings, _final) =
        ti4_training::rollout::audit_game_with_deciders(
            table.content,
            table.factions,
            DEFAULT,
            seed,
            rotation,
            ti4_training::rollout::Horizon {
                rounds: 1,
                steps: 200_000,
            },
            &ti4_training::rollout::OpeningMap::PythonPool {
                pool: Arc::clone(table.pool),
                tile_seed_offset: TILE_SEED_OFFSET,
            },
            |seated, baselines| {
                let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                for (index, (player, seat_faction)) in seated.iter().enumerate() {
                    let row = ti4_mlp::FactionRow::of(seat_faction.as_str())
                        .map_err(|error| format!("{player}: {error}"))?;
                    let baseline = baselines
                        .get(player)
                        .copied()
                        .ok_or_else(|| format!("{player} has no setup baseline"))?;
                    let stream = seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(u64::try_from(index).unwrap_or(0));
                    let (decider, _status) =
                        ti4_mlp::bot::MlpBot::sharing(actor, table.vocabulary.clone(), row, stream)
                            .at_temperature(table.temperature)
                            .from_setup(baseline)
                            .seat();
                    let decider = if seat_faction.as_str() == faction {
                        let taken = wrap
                            .borrow_mut()
                            .take()
                            .ok_or_else(|| format!("{faction} was seated twice"))?;
                        taken(decider)
                    } else {
                        decider
                    };
                    deciders.insert(player.clone(), decider);
                }
                Ok(deciders)
            },
        )?;

    for (player, opening) in &openings {
        if assignments
            .get(player)
            .is_some_and(|seated| seated.as_str() == faction)
        {
            return Ok(Outcome {
                cleared: opening.cleared(),
                planet_short: opening.planet_shortfall(),
                system_short: opening.system_shortfall(),
                units_ok: opening.units_ok(),
            });
        }
    }
    Err(format!("{faction} was not seated in {seed}/{rotation}"))
}

#[expect(
    clippy::too_many_lines,
    reason = "one measurement: the enumeration and the split it reports are one thing"
)]
fn main() {
    let bundle_path = argument("--bundle")
        .unwrap_or_else(|| refuse("--bundle is required: repairs are relative to a policy"));
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
    // Replay the recording pass N times per failure and report whether the lines agree, instead of
    // enumerating. The whole method assumes a greedy prefix reproduces exactly; this measures
    // whether it does, which is not the same as assuming it and is how the assumption was found to
    // be false.
    let verify: usize = argument("--verify").map_or(0, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--verify must be a number"))
    });
    // Failures are taken in the order they were found, which is seed order, so a cap is a prefix of
    // the same distribution rather than a selection from it.
    // Where to write the per-failure rows. The confidence interval on a repairability rate has to
    // be bootstrapped over *map seeds*, not failures: a single map contributes up to fourteen
    // failures and they are not independent -- same topology, same opponents, same slice. Treating
    // 250 failures as 250 observations understates the interval badly. That analysis belongs
    // outside a Rust binary, so the rows go to a file.
    let out_path = argument("--out");
    let max_failures: usize = argument("--max-failures").map_or(usize::MAX, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--max-failures must be a number"))
    });

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;

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
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();

    println!("counterfactual repair for {bundle_path}");
    println!("  temperature {temperature} (greedy: the prefix must replay exactly)");
    println!("  maps        {pool_path} (Train)");
    println!(
        "  seeds       {seed_base}..{} x {} rotations",
        seed_base + seeds,
        FACTIONS.len()
    );
    println!();

    // ---- find the failures ------------------------------------------------------------------
    let started = std::time::Instant::now();
    let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers);

    let found: Vec<Result<(usize, Vec<Target>), String>> = jobs
        .chunks(per_worker)
        .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let table = Table {
                content,
                factions: &factions,
                pool: &pool,
                vocabulary: &vocabulary,
                temperature,
            };
            let mut seats = 0usize;
            let mut targets = Vec::new();
            for (seed, rotation) in chunk {
                let (_events, _setup, assignments, openings, _final) =
                    ti4_training::rollout::audit_game_with_deciders(
                        table.content,
                        table.factions,
                        DEFAULT,
                        seed,
                        rotation,
                        ti4_training::rollout::Horizon {
                            rounds: 1,
                            steps: 200_000,
                        },
                        &ti4_training::rollout::OpeningMap::PythonPool {
                            pool: Arc::clone(table.pool),
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
                                    table.vocabulary.clone(),
                                    row,
                                    stream,
                                )
                                .at_temperature(table.temperature)
                                .from_setup(baseline)
                                .seat();
                                deciders.insert(player.clone(), decider);
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
                    targets.push(Target {
                        seed,
                        rotation,
                        faction: faction.to_string(),
                        planet_short: opening.planet_shortfall(),
                        system_short: opening.system_shortfall(),
                        units_ok: opening.units_ok(),
                    });
                }
            }
            Ok((seats, targets))
        })
        .collect();

    let mut seats = 0usize;
    let mut targets: Vec<Target> = Vec::new();
    for chunk in found {
        let (n, mut rows) = chunk.unwrap_or_else(|error| refuse(&error));
        seats += n;
        targets.append(&mut rows);
    }
    targets.sort_by_key(|t| (t.seed, t.rotation, t.faction.clone()));
    let all_failures = targets.len();
    targets.truncate(max_failures);
    println!(
        "  {seats} seat-games, {all_failures} failures; investigating {} ({:.1?} to find them)",
        targets.len(),
        started.elapsed()
    );
    println!();

    // ---- determinism check ------------------------------------------------------------------
    if verify > 0 {
        let per_worker = targets.len().div_ceil(workers).max(1);
        let checks: Vec<Result<Vec<(Target, bool, usize)>, String>> = targets
            .chunks(per_worker)
            .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
            .collect::<Vec<_>>()
            .into_par_iter()
            .map(|(local, chunk)| {
                let local = Rc::new(local);
                let table = Table {
                    content,
                    factions: &factions,
                    pool: &pool,
                    vocabulary: &vocabulary,
                    temperature,
                };
                let mut rows = Vec::new();
                for target in chunk {
                    let mut lines: Vec<Vec<(String, usize, usize)>> = Vec::new();
                    for _ in 0..verify {
                        let log = Rc::new(RefCell::new(Vec::new()));
                        let recording = Rc::clone(&log);
                        play(
                            &table,
                            &local,
                            target.seed,
                            target.rotation,
                            &target.faction,
                            move |inner| {
                                Box::new(Intervene {
                                    inner,
                                    seen: 0,
                                    mode: Mode::Record(recording),
                                })
                            },
                        )?;
                        let line = log.borrow();
                        lines.push(
                            line.iter()
                                .map(|d| (d.head.clone(), d.options, d.chosen))
                                .collect(),
                        );
                    }
                    // Where the first disagreement is, if there is one. A late divergence and an
                    // immediate one mean different things about the cause.
                    let first = &lines[0];
                    let mut diverged_at = usize::MAX;
                    for other in &lines[1..] {
                        let limit = first.len().min(other.len());
                        for index in 0..limit {
                            if first[index] != other[index] {
                                diverged_at = diverged_at.min(index);
                                break;
                            }
                        }
                        if first.len() != other.len() {
                            diverged_at = diverged_at.min(limit);
                        }
                    }
                    rows.push((target, diverged_at == usize::MAX, first.len()));
                }
                Ok(rows)
            })
            .collect();

        let mut agree = 0usize;
        let mut differ = 0usize;
        let mut differing: Vec<(u64, usize, String, usize)> = Vec::new();
        for chunk in checks {
            for (target, same, decisions) in chunk.unwrap_or_else(|error| refuse(&error)) {
                if same {
                    agree += 1;
                } else {
                    differ += 1;
                    differing.push((target.seed, target.rotation, target.faction, decisions));
                }
            }
        }
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let rate = differ as f64 / (agree + differ).max(1) as f64 * 100.0;
        println!("  determinism: {verify} recording passes per failure");
        println!();
        println!("    {agree} reproduced identically");
        println!("    {differ} did not   ({rate:.2}%)");
        for (seed, rotation, faction, decisions) in differing.iter().take(12) {
            println!("      {seed}/{rotation} {faction} ({decisions} decisions)");
        }
        return;
    }

    // ---- enumerate ---------------------------------------------------------------------------
    let enumerated = std::time::Instant::now();
    let per_worker = targets.len().div_ceil(workers).max(1);
    let results: Vec<Result<Vec<Repair>, String>> = targets
        .chunks(per_worker)
        .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let table = Table {
                content,
                factions: &factions,
                pool: &pool,
                vocabulary: &vocabulary,
                temperature,
            };
            let mut out = Vec::new();

            for target in chunk {
                // Pass one: the line as it was played.
                let log = Rc::new(RefCell::new(Vec::new()));
                let recording = Rc::clone(&log);
                let cleared = play(
                    &table,
                    &local,
                    target.seed,
                    target.rotation,
                    &target.faction,
                    move |inner| {
                        Box::new(Intervene {
                            inner,
                            seen: 0,
                            mode: Mode::Record(recording),
                        })
                    },
                )?;
                if cleared.cleared {
                    // The census said this seat failed. If the recording pass clears it, the replay
                    // is not reproducing the game and every repair below would be measured against
                    // the wrong line.
                    return Err(format!(
                        "{}/{} {}: the recorded replay cleared, so the run is not reproducible",
                        target.seed, target.rotation, target.faction
                    ));
                }
                let line = log.borrow().clone();

                // Pass two: one alternate at one index, everything else identical.
                let mut tried = 0usize;
                let mut cleared_count = 0usize;
                let mut discarded = 0usize;
                let mut repairing: Vec<(usize, String)> = Vec::new();
                let mut per_index: Vec<IndexResult> = Vec::new();
                for (index, decision) in line.iter().enumerate() {
                    let mut repaired_here = false;
                    let mut here_cleared = 0usize;
                    let mut best: Option<Outcome> = None;
                    for alternate in 0..decision.options {
                        if alternate == decision.chosen {
                            continue;
                        }
                        tried += 1;
                        let mismatch = Rc::new(RefCell::new(None));
                        let flag = Rc::clone(&mismatch);
                        let options = decision.options;
                        let ok = play(
                            &table,
                            &local,
                            target.seed,
                            target.rotation,
                            &target.faction,
                            move |inner| {
                                Box::new(Intervene {
                                    inner,
                                    seen: 0,
                                    mode: Mode::Substitute {
                                        index,
                                        alternate,
                                        expect_options: options,
                                        mismatch: flag,
                                    },
                                })
                            },
                        )?;
                        // A mismatch means this index was not the decision that was recorded, so
                        // the substitution never happened and the game that ran is the unmodified
                        // one. Counting it either way would be wrong: as a repair it is a false
                        // positive, as a non-repair it is a substitution that was never tried. It
                        // is discarded and the rate reported, because a discard rate that stopped
                        // being negligible would invalidate the measurement.
                        if mismatch.borrow().is_some() {
                            discarded += 1;
                            tried -= 1;
                            continue;
                        }
                        if ok.cleared {
                            cleared_count += 1;
                            repaired_here = true;
                            here_cleared += 1;
                        } else if best.is_none_or(|current: Outcome| {
                            // Pareto: strictly better on one axis and no worse on any.
                            let better = ok.planet_short <= current.planet_short
                                && ok.system_short <= current.system_short
                                && (ok.units_ok || !current.units_ok);
                            let strict = ok.planet_short < current.planet_short
                                || ok.system_short < current.system_short
                                || (ok.units_ok && !current.units_ok);
                            better && strict
                        }) {
                            best = Some(ok);
                        }
                    }
                    if repaired_here {
                        repairing.push((index, decision.head.clone()));
                    }
                    per_index.push(IndexResult {
                        index,
                        head: decision.head.clone(),
                        options: decision.options,
                        cleared: here_cleared,
                        best_planet_short: best.map_or(usize::MAX, |o| o.planet_short),
                        best_system_short: best.map_or(usize::MAX, |o| o.system_short),
                    });
                }

                out.push(Repair {
                    target,
                    decisions: line.len(),
                    tried,
                    cleared: cleared_count,
                    discarded,
                    repairing,
                    per_index,
                });
            }
            Ok(out)
        })
        .collect();

    let mut repairs: Vec<Repair> = Vec::new();
    for chunk in results {
        repairs.append(&mut chunk.unwrap_or_else(|error| refuse(&error)));
    }
    if repairs.is_empty() {
        refuse("no failures were investigated");
    }

    // ---- the split ---------------------------------------------------------------------------
    let investigated = repairs.len();
    let repairable = repairs.iter().filter(|r| !r.repairing.is_empty()).count();
    let total_tried: usize = repairs.iter().map(|r| r.tried).sum();
    let total_cleared: usize = repairs.iter().map(|r| r.cleared).sum();
    let total_discarded: usize = repairs.iter().map(|r| r.discarded).sum();

    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let share = |part: usize, whole: usize| -> f64 {
        if whole == 0 {
            0.0
        } else {
            part as f64 / whole as f64 * 100.0
        }
    };

    println!("  {} replays in {:.1?}", total_tried, enumerated.elapsed());
    println!();
    println!("  REPAIRABLE BY CHANGING EXACTLY ONE DECISION");
    println!();
    println!(
        "    {repairable} of {investigated} failures   {:.1}%",
        share(repairable, investigated)
    );
    println!(
        "    {total_cleared} of {total_tried} single substitutions cleared   {:.2}%",
        share(total_cleared, total_tried)
    );
    println!(
        "    {total_discarded} discarded (index was not the recorded decision)   {:.3}%",
        share(total_discarded, total_tried + total_discarded)
    );
    println!();

    // Which head, when changed, rescues. An index is counted once however many of its alternates
    // worked, because the question is which decision was wrong, not how forgiving it was.
    let mut by_head: BTreeMap<String, usize> = BTreeMap::new();
    for repair in &repairs {
        for (_, head) in &repair.repairing {
            *by_head.entry(head.clone()).or_default() += 1;
        }
    }
    let mut heads: Vec<(&String, &usize)> = by_head.iter().collect();
    heads.sort_by(|a, b| b.1.cmp(a.1));
    println!("  which decision, changed, rescues (repairing indices by head)");
    println!();
    let repairing_total: usize = by_head.values().sum();
    for (head, count) in heads {
        println!(
            "    {head:<14} {count:>5}   {:>5.1}%",
            share(*count, repairing_total)
        );
    }
    println!();

    // By faction, because Jol-Nar fails differently from everyone else and an aggregate hides it.
    let mut faction_rows: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for repair in &repairs {
        let entry = faction_rows
            .entry(repair.target.faction.clone())
            .or_insert((0, 0));
        entry.0 += 1;
        if !repair.repairing.is_empty() {
            entry.1 += 1;
        }
    }
    println!("  by faction");
    println!();
    println!("    faction      failures   repairable");
    for (faction, (total, fixed)) in &faction_rows {
        println!(
            "    {faction:<12} {total:>8}   {fixed:>5}  {:>5.1}%",
            share(*fixed, *total)
        );
    }
    println!();

    // By the shape of the failure, which is what decides whether a curriculum is worth building.
    let mut shape_rows: BTreeMap<(usize, usize, bool), (usize, usize)> = BTreeMap::new();
    for repair in &repairs {
        let entry = shape_rows
            .entry((
                repair.target.planet_short,
                repair.target.system_short,
                repair.target.units_ok,
            ))
            .or_insert((0, 0));
        entry.0 += 1;
        if !repair.repairing.is_empty() {
            entry.1 += 1;
        }
    }
    let mut shapes: Vec<((usize, usize, bool), (usize, usize))> = shape_rows.into_iter().collect();
    shapes.sort_by(|a, b| b.1.0.cmp(&a.1.0));
    println!("  by failure shape");
    println!();
    println!("    planets  systems  composition   failures   repairable");
    for ((planets, systems, units), (total, fixed)) in shapes.iter().take(8) {
        println!(
            "      -{planets}       -{systems}      {}      {total:>7}   {fixed:>5}  {:>5.1}%",
            if *units { "ok   " } else { "SHORT" },
            share(*fixed, *total)
        );
    }
    println!();

    // Where in the line the repairing decision sits. A repair available only at decision zero is
    // the failure mode that sank the last attempt, so the position matters as much as the count.
    let mut buckets = [0usize; 5];
    let mut earliest_zero = 0usize;
    for repair in &repairs {
        if repair.decisions == 0 {
            continue;
        }
        if let Some((first, _)) = repair.repairing.first() {
            if *first == 0 {
                earliest_zero += 1;
            }
        }
        for (index, _) in &repair.repairing {
            #[expect(clippy::cast_precision_loss, reason = "line lengths are small")]
            let position = *index as f64 / repair.decisions as f64;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "position is in [0, 1]"
            )]
            let bucket = ((position * 5.0) as usize).min(4);
            buckets[bucket] += 1;
        }
    }
    println!("  where the repairing decision sits in the line");
    println!();
    for (index, count) in buckets.iter().enumerate() {
        println!(
            "    {:>3}-{:>3}%   {count:>5}   {:>5.1}%",
            index * 20,
            (index + 1) * 20,
            share(*count, repairing_total)
        );
    }
    println!();
    println!(
        "    failures whose earliest repair is decision 0: {earliest_zero} ({:.1}% of repairable)",
        share(earliest_zero, repairable)
    );

    if let Some(path) = out_path {
        let mut json = String::from("{\"schema\":\"ti4-counterfactual-repair-v1\",\"bundle\":\"");
        json.push_str(&bundle_path.replace('\\', "/"));
        json.push_str(&format!(
            "\",\"temperature\":{temperature},\"seat_games\":{seats},\"all_failures\":{all_failures},\"failures\":["
        ));
        for (n, repair) in repairs.iter().enumerate() {
            if n > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"seed\":{},\"rotation\":{},\"faction\":\"{}\",\"planet_short\":{},\
                 \"system_short\":{},\"units_ok\":{},\"decisions\":{},\"tried\":{},\
                 \"cleared\":{},\"discarded\":{},\"indices\":[",
                repair.target.seed,
                repair.target.rotation,
                repair.target.faction,
                repair.target.planet_short,
                repair.target.system_short,
                repair.target.units_ok,
                repair.decisions,
                repair.tried,
                repair.cleared,
                repair.discarded
            ));
            for (m, row) in repair.per_index.iter().enumerate() {
                if m > 0 {
                    json.push(',');
                }
                json.push_str(&format!(
                    "{{\"i\":{},\"head\":\"{}\",\"options\":{},\"cleared\":{},\"bp\":{},\"bs\":{}}}",
                    row.index,
                    row.head,
                    row.options,
                    row.cleared,
                    if row.best_planet_short == usize::MAX {
                        -1
                    } else {
                        i64::try_from(row.best_planet_short).unwrap_or(-1)
                    },
                    if row.best_system_short == usize::MAX {
                        -1
                    } else {
                        i64::try_from(row.best_system_short).unwrap_or(-1)
                    }
                ));
            }
            json.push_str("]}");
        }
        json.push_str("]}");
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|error| refuse(&format!("creating {}: {error}", parent.display())));
        }
        std::fs::write(&path, json)
            .unwrap_or_else(|error| refuse(&format!("writing {path}: {error}")));
        println!();
        println!("  wrote {} failures to {path}", repairs.len());
    }
}
