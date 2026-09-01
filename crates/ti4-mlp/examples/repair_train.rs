//! Train on decisions that were demonstrated wrong, and measure whether it helps.
//!
//! One process, because the labels expire. Whether substituting an action clears depends on the
//! policy that plays the rest of the round, so a rescue set collected against one policy is stale
//! for the next. Each round therefore re-collects failures, re-enumerates repairs against the
//! *current* weights, trains, and measures.
//!
//! ```text
//! collect failures (Train pool, greedy)
//!   -> enumerate single-decision repairs, exhaustively
//!   -> build preference samples at repairing indices
//!   -> train  L = (1/N) Σ (1/|C|) Σ softplus(s_failed − s_clearing)
//!   -> measure greedy clearance (Validation pool)
//!   -> keep the best weights, regenerate, repeat
//! ```
//!
//! # Why the measurement is separate from everything else
//!
//! Failures are collected on the **Train** pool and clearance is measured on the **Validation**
//! pool, with role guards on both. The training signal and the number that decides whether it
//! worked never come from the same maps.
//!
//! Evaluation is greedy. Training at a temperature rescales the logits, so a fixed non-zero
//! evaluation temperature is not a fixed scale across policies; `argmax(s) = argmax(s/c)` is the
//! only reading that survives that, and it is what the target is stated in.
//!
//! # The safeguard
//!
//! This is an auxiliary objective run on its own, with no PPO term to hold the policy in place, so
//! the guard against drift is empirical: a small step size, held-out clearance after every epoch,
//! and the best weights kept rather than the last. The previous attempt at failure-directed
//! training regressed 2.2 points and would have been caught by exactly this
//! (`339f42d`) — it was, in fact, and reported honestly.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example repair_train -- \
//!   --bundle out/checkpoints/sweep-A-250/checkpoint-14476 --rounds 3 --out out/checkpoints/repair
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::{ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_mlp::repair::{Anchor, Sample};
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

/// Substitute one option at one decision index; decide normally everywhere else.
struct Intervene {
    inner: Box<dyn Decider>,
    seen: usize,
    index: usize,
    alternate: usize,
    expect_options: usize,
    mismatch: Rc<RefCell<bool>>,
}

impl Intervene {
    fn answer(
        &mut self,
        choice: &Choice,
        delegate: impl FnOnce(&mut Box<dyn Decider>) -> Result<ChoiceOption, IllegalChoice>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        // Count exactly what the recorder counts. `MlpBot::record` drops forced decisions -- with
        // one legal option the probability is 1.0 whatever the policy believes, so the ratio is
        // identically 1 and the record would carry weight but no gradient. Counting them here and
        // not there shifts every index after the first forced decision, which discarded 99% of
        // substitutions before this line existed.
        if choice.options.len() < 2 {
            return delegate(&mut self.inner);
        }
        let index = self.seen;
        self.seen += 1;
        if index != self.index {
            return delegate(&mut self.inner);
        }
        // A different option set here means the prefix did not reproduce, so substituting by
        // position would intervene on a different question. Flagged and discarded, never counted.
        if choice.options.len() != self.expect_options {
            *self.mismatch.borrow_mut() = true;
            return delegate(&mut self.inner);
        }
        choice
            .options
            .get(self.alternate)
            .cloned()
            .ok_or_else(|| IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "alternate is out of range".to_owned(),
            })
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

/// Everything a replay needs.
struct Table<'a> {
    content: &'static ContentStore,
    factions: &'a [FactionId],
    pool: &'a Arc<ti4_sim::MapPool>,
    vocabulary: &'a ti4_policy::vocabulary::Vocabulary,
}

/// Play one game and report, for every seat, whether it cleared.
///
/// `wrap` is applied to the seat holding `faction`, when one is named.
fn play<W>(
    table: &Table<'_>,
    actor: &Rc<ti4_mlp::Actor>,
    seed: u64,
    rotation: usize,
    faction: Option<&str>,
    wrap: W,
) -> Result<BTreeMap<String, bool>, String>
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
                    // Greedy, so the prefix reproduces and the substitution is the only difference.
                    let (decider, _status) =
                        ti4_mlp::bot::MlpBot::sharing(actor, table.vocabulary.clone(), row, stream)
                            .at_temperature(0.001)
                            .from_setup(baseline)
                            .seat();
                    let decider = if faction.is_some_and(|want| want == seat_faction.as_str()) {
                        match wrap.borrow_mut().take() {
                            Some(apply) => apply(decider),
                            None => return Err(format!("{seat_faction} was seated twice")),
                        }
                    } else {
                        decider
                    };
                    deciders.insert(player.clone(), decider);
                }
                Ok(deciders)
            },
        )?;

    let mut cleared = BTreeMap::new();
    for (player, opening) in &openings {
        if let Some(seated) = assignments.get(player) {
            cleared.insert(seated.to_string(), opening.cleared());
        }
    }
    Ok(cleared)
}

/// Play a game recording the target seat's decisions, and return them.
fn record_line(
    table: &Table<'_>,
    actor: &Rc<ti4_mlp::Actor>,
    seed: u64,
    rotation: usize,
    faction: &str,
) -> Result<(bool, Vec<ti4_mlp::ppo::Step>), String> {
    let handle: Rc<RefCell<Option<Rc<RefCell<Vec<ti4_mlp::bot::PpoRecord>>>>>> =
        Rc::new(RefCell::new(None));
    // `recording_ppo` is a builder step on `MlpBot`, not something that can wrap a finished
    // decider, so the target seat is *built* as the recorder rather than wrapped. `record` refuses
    // a decision it cannot record rather than skipping it, so the step list is one entry per
    // decision and its indices line up with the substitution counter used below.
    // Build the seat ourselves so the recorder is the seat, not a wrapper around it.
    let cleared = play_recording(table, actor, seed, rotation, faction, &handle)?;
    let records = handle
        .borrow_mut()
        .take()
        .ok_or_else(|| format!("{faction} recorded nothing in {seed}/{rotation}"))?;
    let steps: Vec<ti4_mlp::ppo::Step> = records.borrow().iter().map(|r| r.step.clone()).collect();
    Ok((cleared, steps))
}

/// As `play`, but the target seat is a recording bot.
fn play_recording(
    table: &Table<'_>,
    actor: &Rc<ti4_mlp::Actor>,
    seed: u64,
    rotation: usize,
    faction: &str,
    handle: &Rc<RefCell<Option<Rc<RefCell<Vec<ti4_mlp::bot::PpoRecord>>>>>>,
) -> Result<bool, String> {
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
                    let bot =
                        ti4_mlp::bot::MlpBot::sharing(actor, table.vocabulary.clone(), row, stream)
                            .at_temperature(0.001)
                            .from_setup(baseline);
                    if seat_faction.as_str() == faction {
                        let bot = bot.recording_ppo(ti4_mlp::bundle::CriticMode::BatchMean);
                        *handle.borrow_mut() = Some(bot.ppo_records());
                        let (decider, _status) = bot.seat();
                        deciders.insert(player.clone(), decider);
                    } else {
                        let (decider, _status) = bot.seat();
                        deciders.insert(player.clone(), decider);
                    }
                }
                Ok(deciders)
            },
        )?;
    for (player, opening) in &openings {
        if assignments
            .get(player)
            .is_some_and(|seated| seated.as_str() == faction)
        {
            return Ok(opening.cleared());
        }
    }
    Err(format!("{faction} was not seated in {seed}/{rotation}"))
}

/// A failed seat-game.
#[derive(Clone)]
struct Target {
    seed: u64,
    rotation: usize,
    faction: String,
}

/// Collect every failure on the training pool.
fn collect_failures(
    table: &Table<'_>,
    actor: &ti4_mlp::Actor,
    seeds: u64,
    seed_base: u64,
) -> Result<(usize, Vec<Target>), String> {
    let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers).max(1);
    let harvest: Vec<Result<(usize, Vec<Target>), String>> = jobs
        .chunks(per_worker)
        .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let mut seats = 0usize;
            let mut targets = Vec::new();
            for (seed, rotation) in chunk {
                let cleared = play(table, &local, seed, rotation, None, |inner| inner)?;
                for (faction, ok) in cleared {
                    seats += 1;
                    if !ok {
                        targets.push(Target {
                            seed,
                            rotation,
                            faction,
                        });
                    }
                }
            }
            Ok((seats, targets))
        })
        .collect();
    let mut seats = 0usize;
    let mut targets = Vec::new();
    for chunk in harvest {
        let (n, mut rows) = chunk?;
        seats += n;
        targets.append(&mut rows);
    }
    targets.sort_by_key(|t| (t.seed, t.rotation, t.faction.clone()));
    Ok((seats, targets))
}

/// Greedy clearance on the validation pool.
fn measure(
    content: &'static ContentStore,
    factions: &[FactionId],
    pool: &Arc<ti4_sim::MapPool>,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    actor: &ti4_mlp::Actor,
    seeds: u64,
    seed_base: u64,
) -> Result<f64, String> {
    let table = Table {
        content,
        factions,
        pool,
        vocabulary,
    };
    let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers).max(1);
    let harvest: Vec<Result<(usize, usize), String>> = jobs
        .chunks(per_worker)
        .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let mut seats = 0usize;
            let mut cleared = 0usize;
            for (seed, rotation) in chunk {
                for (_, ok) in play(&table, &local, seed, rotation, None, |inner| inner)? {
                    seats += 1;
                    cleared += usize::from(ok);
                }
            }
            Ok((seats, cleared))
        })
        .collect();
    let mut seats = 0usize;
    let mut cleared = 0usize;
    for chunk in harvest {
        let (n, c) = chunk?;
        seats += n;
        cleared += c;
    }
    if seats == 0 {
        return Err("no seat-games measured".to_owned());
    }
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    Ok(cleared as f64 / seats as f64 * 100.0)
}

#[expect(
    clippy::too_many_lines,
    reason = "one experiment: collect, enumerate, train and measure are one loop and splitting \
              them would separate the labels from the policy they were collected against"
)]
fn main() {
    let bundle_path = argument("--bundle")
        .unwrap_or_else(|| refuse("--bundle is required: repairs are relative to a policy"));
    let rounds: usize = number("--rounds", 3);
    let seeds: u64 = number("--seeds", 300);
    let seed_base: u64 = number("--seed-base", 800_000_000);
    let eval_seeds: u64 = number("--eval-seeds", 200);
    let epochs: usize = number("--epochs", 6);
    let learning_rate: f64 = number("--learning-rate", 1e-5);
    let max_failures: usize = number("--max-failures", usize::MAX);
    // Weight on the trust region. Without it the repair objective took held-out clearance from
    // 93.96% to 12.94% in sixteen epochs while its own loss fell smoothly: repair states are 0.4%
    // of the decision distribution and rewriting the rest is free unless something forbids it.
    let anchor_weight: f64 = number("--anchor-weight", 1.0);
    // A comma-separated list turns the run into a sweep: collect and enumerate **once**, then train
    // each weight from the same starting weights against the same labels. Enumeration is 25 minutes
    // and the weights do not change what it would find, so re-running it per weight would be pure
    // waste and would also let sampling noise between collections masquerade as a weight effect.
    let anchor_sweep: Vec<f64> = argument("--anchor-weights").map_or_else(
        || vec![anchor_weight],
        |text| {
            text.split(',')
                .map(|piece| {
                    piece
                        .trim()
                        .parse()
                        .unwrap_or_else(|_| refuse("--anchor-weights expects numbers"))
                })
                .collect()
        },
    );
    // How many games' worth of ordinary decisions hold the policy in place. These are drawn from
    // whole games, cleared and failed alike, so the anchor covers the behaviour that is already
    // right rather than only the neighbourhood of the failures.
    let anchor_games: u64 = number("--anchor-games", 40);
    let out = argument("--out").unwrap_or_else(|| "out/checkpoints/repair".to_owned());

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let mut actor = loaded.actor;

    // The vocabulary as it was written, so every checkpoint this produces carries the same one. A
    // bundle is addressed by its slots; rebuilding them here would risk writing weights against a
    // vocabulary they were not trained on.
    let slots_text = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("reading the bundle's slots.json: {error}")));
    // Fail closed, as every checkpoint path in this project does: a bundle whose manifest cannot
    // name the commit that produced it is not traceable and should not exist.
    let git_commit = std::env::var("GIT_COMMIT")
        .unwrap_or_else(|_| refuse("GIT_COMMIT is required so checkpoints can be traced"));

    let train_pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            ti4_sim::artifacts::read_and_verify_pool_role(
                std::path::Path::new("out/pools/full_np8_12_train.json"),
                &[ti4_sim::artifacts::ArtifactRole::Train],
            )
            .unwrap_or_else(|error| refuse(&format!("train pool: {error}"))),
        ))
        .unwrap_or_else(|error| refuse(&format!("parsing the train pool: {error}"))),
    );
    let holdout_pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            ti4_sim::artifacts::read_and_verify_pool_role(
                std::path::Path::new("out/pools/full_np8_12_holdout.json"),
                &[ti4_sim::artifacts::ArtifactRole::Validation],
            )
            .unwrap_or_else(|error| refuse(&format!("holdout pool: {error}"))),
        ))
        .unwrap_or_else(|error| refuse(&format!("parsing the holdout pool: {error}"))),
    );
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();

    println!("repair training from {bundle_path}");
    println!("  rounds      {rounds}, {epochs} epochs each");
    println!(
        "  failures    Train pool, seeds {seed_base}..{}",
        seed_base + seeds
    );
    println!("  measured    Validation pool, greedy, {eval_seeds} seeds");
    println!("  step        {learning_rate}");
    println!();

    let baseline = measure(
        content,
        &factions,
        &holdout_pool,
        &vocabulary,
        &actor,
        eval_seeds,
        900_000_000,
    )
    .unwrap_or_else(|error| refuse(&error));
    println!("  baseline    {baseline:.2}% greedy, held out");
    println!();

    // Anchor states, captured **once** against the starting policy. They are the reference the
    // trust region is measured to, so re-capturing them each round would let the target drift with
    // the policy and the anchor would hold nothing.
    let anchors = {
        let table = Table {
            content,
            factions: &factions,
            pool: &train_pool,
            vocabulary: &vocabulary,
        };
        let jobs: Vec<(u64, usize)> = (seed_base..seed_base + anchor_games)
            .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
            .collect();
        let workers = rayon::current_num_threads().max(1);
        let per_worker = jobs.len().div_ceil(workers).max(1);
        let harvest: Vec<Result<Vec<Anchor>, String>> = jobs
            .chunks(per_worker)
            .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
            .collect::<Vec<_>>()
            .into_par_iter()
            .map(|(local, chunk)| {
                let local = Rc::new(local);
                let mut rows = Vec::new();
                for (seed, rotation) in chunk {
                    for faction in FACTIONS {
                        let (_cleared, line) =
                            record_line(&table, &local, seed, rotation, faction)?;
                        for step in line {
                            let head = ti4_mlp::heads()
                                .get(step.head)
                                .ok_or_else(|| format!("head {} is out of range", step.head))?;
                            // Temperature 1.0: the anchor is about the shape of the distribution,
                            // and reading it at the greedy limit would record a near one-hot that
                            // says nothing about the mass the policy puts elsewhere.
                            let reference = local
                                .probabilities(&step.options, head, step.row, 1.0)
                                .map_err(|error| format!("anchor reference: {error}"))?;
                            rows.push(Anchor {
                                row: step.row,
                                head: step.head,
                                options: step.options,
                                reference,
                            });
                        }
                    }
                }
                Ok(rows)
            })
            .collect();
        let mut rows = Vec::new();
        for chunk in harvest {
            rows.append(&mut chunk.unwrap_or_else(|error| refuse(&error)));
        }
        rows
    };
    println!(
        "  anchor      {} states from {anchor_games} maps, weight {anchor_weight}",
        anchors.len()
    );
    println!();

    let mut best = baseline;
    let mut best_round = 0usize;

    for round in 1..=rounds {
        let started = std::time::Instant::now();
        let train_table = Table {
            content,
            factions: &factions,
            pool: &train_pool,
            vocabulary: &vocabulary,
        };

        // ---- collect ------------------------------------------------------------------------
        let (seats, mut targets) = collect_failures(&train_table, &actor, seeds, seed_base)
            .unwrap_or_else(|error| refuse(&error));
        let found = targets.len();
        targets.truncate(max_failures);
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let train_rate = (seats - found) as f64 / seats as f64 * 100.0;
        println!(
            "  round {round}: {seats} seat-games, {train_rate:.2}% cleared on Train, {found} failures"
        );

        // ---- enumerate and build samples ----------------------------------------------------
        let workers = rayon::current_num_threads().max(1);
        let per_worker = targets.len().div_ceil(workers).max(1);
        let harvest: Vec<Result<(Vec<Sample>, usize, usize), String>> = targets
            .chunks(per_worker)
            .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
            .collect::<Vec<_>>()
            .into_par_iter()
            .map(|(local, chunk)| {
                let local = Rc::new(local);
                let table = Table {
                    content,
                    factions: &factions,
                    pool: &train_pool,
                    vocabulary: &vocabulary,
                };
                let mut samples = Vec::new();
                let mut repairable = 0usize;
                let mut discarded = 0usize;

                for target in chunk {
                    let (cleared, line) = record_line(
                        &table,
                        &local,
                        target.seed,
                        target.rotation,
                        &target.faction,
                    )?;
                    if cleared {
                        // The collection pass said this failed. A recording pass that clears means
                        // the replay is not reproducing, and every label below it would be wrong.
                        return Err(format!(
                            "{}/{} {}: the recorded replay cleared",
                            target.seed, target.rotation, target.faction
                        ));
                    }
                    let mut any = false;
                    for (index, step) in line.iter().enumerate() {
                        let options = step.options.len();
                        if options < 2 {
                            continue;
                        }
                        let mut clearing = Vec::new();
                        for alternate in 0..options {
                            if alternate == step.chosen {
                                continue;
                            }
                            let mismatch = Rc::new(RefCell::new(false));
                            let flag = Rc::clone(&mismatch);
                            let cleared = play(
                                &table,
                                &local,
                                target.seed,
                                target.rotation,
                                Some(&target.faction),
                                move |inner| {
                                    Box::new(Intervene {
                                        inner,
                                        seen: 0,
                                        index,
                                        alternate,
                                        expect_options: options,
                                        mismatch: flag,
                                    })
                                },
                            )?;
                            if *mismatch.borrow() {
                                discarded += 1;
                                continue;
                            }
                            if cleared.get(&target.faction).copied().unwrap_or(false) {
                                clearing.push(alternate);
                            }
                        }
                        if clearing.is_empty() {
                            continue;
                        }
                        any = true;
                        match Sample::new(
                            step.row,
                            step.head,
                            step.options.clone(),
                            step.chosen,
                            clearing,
                        ) {
                            Ok(sample) => samples.push(sample),
                            Err(error) => return Err(format!("building a repair sample: {error}")),
                        }
                    }
                    if any {
                        repairable += 1;
                    }
                }
                Ok((samples, repairable, discarded))
            })
            .collect();

        let mut samples: Vec<Sample> = Vec::new();
        let mut repairable = 0usize;
        let mut discarded = 0usize;
        for chunk in harvest {
            let (mut rows, fixed, dropped) = chunk.unwrap_or_else(|error| refuse(&error));
            samples.append(&mut rows);
            repairable += fixed;
            discarded += dropped;
        }
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let repair_rate = repairable as f64 / targets.len().max(1) as f64 * 100.0;
        println!(
            "            {repairable} repairable ({repair_rate:.1}%), {} samples, {discarded} discarded, {:.1?}",
            samples.len(),
            started.elapsed()
        );
        if samples.is_empty() {
            println!("            nothing to train on; stopping");
            break;
        }

        // ---- train --------------------------------------------------------------------------
        // The round's starting weights on disk, so each anchor weight in the sweep can be trained
        // from exactly the same policy. Round 1 could reuse `--bundle`, but later rounds start from
        // whatever the previous round produced and there is no other copy of it.
        let round_start = std::path::Path::new(&out)
            .join(format!("round-{round}"))
            .join("start")
            .display()
            .to_string();
        if anchor_sweep.len() > 1 {
            ti4_mlp::bundle::write(
                std::path::Path::new(&round_start),
                &actor,
                &slots_text,
                ti4_mlp::bundle::CriticMode::BatchMean,
                &ti4_mlp::bundle::Provenance {
                    source: "repair_train".to_owned(),
                    git_commit: git_commit.clone(),
                    update: u64::try_from(round).unwrap_or(0),
                },
            )
            .unwrap_or_else(|error| refuse(&format!("writing {round_start}: {error}")));
        }

        // `distill::Adam` over the actor's own parameter handles. `ppo::Adam` wraps the same
        // optimiser but its `step` is private and keyed to a PPO batch; here the gradient comes from
        // one backward over the whole preference set, which is small enough to need no minibatching.
        #[expect(unused_mut, reason = "reassigned inside the sweep loop")]
        let settings = ti4_mlp::ppo::Settings {
            learning_rate,
            ..ti4_mlp::ppo::Settings::default()
        };
        let mut optimizer =
            ti4_mlp::ppo::Adam::new(&mut actor, ti4_mlp::bundle::CriticMode::BatchMean, settings)
                .unwrap_or_else(|error| refuse(&format!("optimizer: {error}")));

        let mut round_best = f64::NEG_INFINITY;
        let mut round_best_epoch = 0usize;
        let mut round_best_weight = anchor_sweep[0];
        for anchor_weight in anchor_sweep.iter().copied() {
            // Back to the round's starting weights, so every weight in the sweep is trained from the
            // same policy against the same labels and the comparison is about the weight alone.
            if anchor_sweep.len() > 1 {
                let restart = ti4_mlp::bundle::read(std::path::Path::new(&round_start))
                    .unwrap_or_else(|error| refuse(&format!("restoring {round_start}: {error}")));
                actor = restart.actor;
                optimizer = ti4_mlp::ppo::Adam::new(
                    &mut actor,
                    ti4_mlp::bundle::CriticMode::BatchMean,
                    settings,
                )
                .unwrap_or_else(|error| refuse(&format!("optimizer: {error}")));
                println!("            --- anchor weight {anchor_weight} ---");
            }
            for epoch in 1..=epochs {
                optimizer
                    .zero_grad(&actor)
                    .unwrap_or_else(|error| refuse(&format!("clearing gradients: {error}")));
                let repair = ti4_mlp::repair::loss(&actor, &samples)
                    .unwrap_or_else(|error| refuse(&format!("repair loss: {error}")))
                    .unwrap_or_else(|| refuse("the repair batch was empty"));
                let value = f64::try_from(&repair).unwrap_or(f64::NAN);
                let anchor = ti4_mlp::repair::anchor_loss(&actor, &anchors)
                    .unwrap_or_else(|error| refuse(&format!("anchor loss: {error}")))
                    .unwrap_or_else(|| refuse("the anchor set was empty"));
                let drift = f64::try_from(&anchor).unwrap_or(f64::NAN);
                let loss = repair + anchor * anchor_weight;
                loss.backward();
                optimizer
                    .step(&actor)
                    .unwrap_or_else(|error| refuse(&format!("optimizer step: {error}")));

                let held = measure(
                    content,
                    &factions,
                    &holdout_pool,
                    &vocabulary,
                    &actor,
                    eval_seeds,
                    900_000_000,
                )
                .unwrap_or_else(|error| refuse(&error));
                let mark = if held > best {
                    "  <-- best overall"
                } else {
                    ""
                };
                println!(
                    "            epoch {epoch}: repair {value:.5}  KL {drift:.5}  held out {held:.2}%{mark}"
                );

                // A bundle per epoch, because an auxiliary objective with no PPO term to hold the
                // policy in place can and does walk past its own optimum. Keeping the best means being
                // able to go back to it, and bundle round-trips are already verified.
                let path = std::path::Path::new(&out)
                    .join(format!("round-{round}"))
                    .join(format!("w{anchor_weight}-epoch-{epoch}"));
                if let Err(error) = ti4_mlp::bundle::write(
                    &path,
                    &actor,
                    &slots_text,
                    ti4_mlp::bundle::CriticMode::BatchMean,
                    &ti4_mlp::bundle::Provenance {
                        source: "repair_train".to_owned(),
                        git_commit: git_commit.clone(),
                        update: u64::try_from(round * 1000 + epoch).unwrap_or(0),
                    },
                ) {
                    refuse(&format!("writing {}: {error}", path.display()));
                }

                if held > round_best {
                    round_best = held;
                    round_best_epoch = epoch;
                    round_best_weight = anchor_weight;
                }
                if held > best {
                    best = held;
                    best_round = round;
                }
            }
        }

        // Go back to the best epoch of this round before regenerating labels against it. Carrying
        // the last epoch forward would collect the next round's rescues against weights that were
        // measured to be worse.
        let best_path = std::path::Path::new(&out)
            .join(format!("round-{round}"))
            .join(format!("w{round_best_weight}-epoch-{round_best_epoch}"));
        let reloaded = ti4_mlp::bundle::read(&best_path)
            .unwrap_or_else(|error| refuse(&format!("reloading {}: {error}", best_path.display())));
        actor = reloaded.actor;
        println!(
            "            round {round} best {round_best:.2}% (weight {round_best_weight}, epoch {round_best_epoch}), reloaded"
        );
        println!();
    }

    println!();
    println!("  baseline {baseline:.2}%  ->  best {best:.2}% (round {best_round})");
    if best <= baseline {
        println!("  NO IMPROVEMENT. Recorded as the result.");
    }
}
