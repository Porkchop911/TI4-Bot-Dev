//! Which decisions in a clearing line actually decide whether it clears?
//!
//! # Why this exists
//!
//! `demo_benchmark` reported that the policy ranks the demonstrated action first 88% of the time on
//! activation and 39.6% on tokens, and it was tempting to read that as "the policy is good at
//! activation and bad at tokens". That reading has two holes.
//!
//! The first is circularity: the demonstrations were **sampled from the policy itself**, so actions
//! it already scores highly are over-represented by construction, and the per-head contrast can come
//! from nothing more than different entropy and margin structure across heads.
//!
//! The second is that agreement is only interesting where the decision matters. If most legal token
//! alternatives would also have cleared, then 39.6% agreement is not a weakness — the demonstration
//! simply did not care which one was copied, and imitating it teaches noise.
//!
//! So this measures the other half: take a line that **cleared**, substitute one legal alternate at
//! one index with everything else held identical, and ask whether it still clears.
//!
//! ```text
//! all alternates still clear   -> the decision was free; agreement there means nothing
//! none still clear             -> the decision was forced; agreement there is everything
//! ```
//!
//! Per head, that gives the criticality the benchmark could not: how often a decision of this kind
//! is load-bearing at all. A head can only be a real weakness where agreement is low **and**
//! criticality is high.
//!
//! # What is held fixed
//!
//! Everything except the one substituted decision. The five opponents replay under the convention
//! the corpus was generated with, the prefix is imposed by id and refused if it is not on offer, and
//! the seat plays on for itself after the substitution. This is the same `do(a_i = a')` intervention
//! `counterfactual_repair` performs, pointed at successes rather than failures.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example decision_criticality -- \
//!   --bundle out/checkpoints/sweep-A-250/checkpoint-14476 --corpus out/corpus/positive \
//!   --trajectories 150
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::{ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_mlp::positive_corpus::{Trajectory, read_all};
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

/// Replays a recorded line, optionally substituting one alternate at one index.
struct Intervene {
    inner: Box<dyn Decider>,
    script: Vec<String>,
    seen: usize,
    /// `None` records the line; `Some((index, alternate))` substitutes at that decision.
    swap: Option<(usize, usize)>,
    /// Head and option count at each decision, filled on the recording pass.
    seen_heads: Rc<RefCell<Vec<(String, usize)>>>,
    broken: Rc<RefCell<bool>>,
}

impl Intervene {
    fn answer(
        &mut self,
        choice: &Choice,
        delegate: impl FnOnce(&mut Box<dyn Decider>) -> Result<ChoiceOption, IllegalChoice>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if choice.options.len() < 2 {
            return delegate(&mut self.inner);
        }
        let index = self.seen;
        self.seen += 1;
        // The bot answers regardless, so its own state advances identically on every pass. Its
        // answer is kept only for the tail past the recorded line, where the seat plays for itself.
        let own = delegate(&mut self.inner)?;

        if let Some((at, alternate)) = self.swap
            && at == index
        {
            return choice.options.get(alternate).cloned().ok_or_else(|| {
                IllegalChoice::DeciderFailed {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                    reason: "alternate out of range".to_owned(),
                }
            });
        }

        // Past the substitution the line has diverged, so the seat plays on for itself. That
        // continuation *is* the intervention. An earlier version only did this once the script ran
        // out, so every decision after a swap still tried to match a recorded id that was no longer
        // on offer, failed, and threw the substitution away -- about 60% of them, leaving a table
        // that read 100% "still clears" because it had almost nothing left to disagree with.
        if self.swap.is_some_and(|(at, _)| index > at) {
            return Ok(own);
        }
        let Some(wanted) = self.script.get(index) else {
            if self.swap.is_some() {
                return Ok(own);
            }
            *self.broken.borrow_mut() = true;
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: "script exhausted".to_owned(),
            });
        };
        let Some(position) = choice.options.iter().position(|o| o.id == *wanted) else {
            *self.broken.borrow_mut() = true;
            return Err(IllegalChoice::DeciderFailed {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
                reason: format!("recorded option {wanted:?} is not on offer"),
            });
        };
        if self.swap.is_none() {
            let head = ti4_mlp::Actor::resolve_head(ti4_policy::learned::decision_head(choice));
            self.seen_heads
                .borrow_mut()
                .push((head.to_owned(), choice.options.len()));
        }
        Ok(choice.options[position].clone())
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

struct Table<'a> {
    content: &'static ContentStore,
    factions: &'a [FactionId],
    pool: &'a Arc<ti4_sim::MapPool>,
    vocabulary: &'a ti4_policy::vocabulary::Vocabulary,
}

/// Play the line, optionally with one substitution. Returns whether the seat cleared.
fn play(
    table: &Table<'_>,
    actor: &Rc<ti4_mlp::Actor>,
    trajectory: &Trajectory,
    swap: Option<(usize, usize)>,
    heads: &Rc<RefCell<Vec<(String, usize)>>>,
) -> Result<Option<bool>, String> {
    let broken = Rc::new(RefCell::new(false));
    let fault = Rc::clone(&broken);
    let captured = Rc::clone(heads);
    let script = trajectory.decisions.clone();
    let want = trajectory.faction.clone();
    #[expect(clippy::cast_precision_loss, reason = "thousandths, well within f64")]
    let temperature = trajectory.temperature_milli as f64 / 1_000.0;

    let played = ti4_training::rollout::audit_game_with_deciders(
        table.content,
        table.factions,
        DEFAULT,
        trajectory.seed,
        trajectory.rotation,
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
            for (index, (player, faction)) in seated.iter().enumerate() {
                let row = ti4_mlp::FactionRow::of(faction.as_str())
                    .map_err(|error| format!("{player}: {error}"))?;
                let baseline = baselines
                    .get(player)
                    .copied()
                    .ok_or_else(|| format!("{player} has no baseline"))?;
                // The positive corpus put every seat at the corpus temperature, so every seat is
                // replayed that way. Getting this wrong makes the line not exist.
                let stream = trajectory
                    .seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0))
                    .wrapping_add(trajectory.temperature_milli);
                let bot =
                    ti4_mlp::bot::MlpBot::sharing(actor, table.vocabulary.clone(), row, stream)
                        .at_temperature(temperature)
                        .from_setup(baseline);
                let (decider, _status) = bot.seat();
                if faction.as_str() == want {
                    deciders.insert(
                        player.clone(),
                        Box::new(Intervene {
                            inner: decider,
                            script: script.clone(),
                            seen: 0,
                            swap,
                            seen_heads: Rc::clone(&captured),
                            broken: Rc::clone(&fault),
                        }),
                    );
                } else {
                    deciders.insert(player.clone(), decider);
                }
            }
            Ok(deciders)
        },
    );
    if *broken.borrow() {
        return Ok(None);
    }
    let (_e, _s, assignments, openings, _f) = played?;
    for (player, opening) in &openings {
        if assignments
            .get(player)
            .is_some_and(|seated| seated.as_str() == want)
        {
            return Ok(Some(opening.cleared()));
        }
    }
    Ok(None)
}

/// Per-head totals.
#[derive(Default)]
struct Tally {
    decisions: usize,
    options: usize,
    alternates: usize,
    still_cleared: usize,
    /// Decisions where every alternate still cleared: the choice was free.
    free: usize,
    /// Decisions where no alternate cleared: the choice was forced.
    forced: usize,
    /// Substitutions that could not be evaluated at all. A rate that stops being small means the
    /// row is a remnant.
    unevaluated: usize,
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let corpus = argument("--corpus").unwrap_or_else(|| "out/corpus/positive".to_owned());
    let wanted: usize = number("--trajectories", 120);

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            ti4_sim::artifacts::read_and_verify_pool_role(
                std::path::Path::new("out/pools/full_np8_12_train.json"),
                &[ti4_sim::artifacts::ArtifactRole::Train],
            )
            .unwrap_or_else(|error| refuse(&format!("train pool: {error}"))),
        ))
        .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();

    let mut trajectories: Vec<Trajectory> = Vec::new();
    for faction in FACTIONS {
        let path = std::path::Path::new(&corpus).join(format!("{faction}.corpus"));
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (_, rows) in read_all(&text)
            .unwrap_or_else(|error| refuse(&format!("parsing {}: {error}", path.display())))
        {
            trajectories.extend(rows);
        }
    }
    trajectories.sort_by_key(|t| (t.seed, t.rotation, t.faction.clone()));
    // Even spread rather than a prefix, so every faction and seed range is represented.
    let stride = (trajectories.len() / wanted.max(1)).max(1);
    trajectories = trajectories
        .into_iter()
        .step_by(stride)
        .take(wanted)
        .collect();

    println!("decision criticality for {bundle_path}");
    println!("  corpus       {corpus}");
    println!("  trajectories {}", trajectories.len());
    println!();

    let started = std::time::Instant::now();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = trajectories.len().div_ceil(workers).max(1);
    let harvest: Vec<(BTreeMap<String, Tally>, usize, usize)> = trajectories
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
            };
            let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();
            let mut replays = 0usize;
            let mut skipped = 0usize;

            for trajectory in chunk {
                let heads = Rc::new(RefCell::new(Vec::new()));
                match play(&table, &local, &trajectory, None, &heads) {
                    Ok(Some(true)) => {}
                    // Either it did not replay, or the corpus claims a clear this replay does not
                    // reproduce. Neither can be used, and neither is counted.
                    _ => {
                        skipped += 1;
                        continue;
                    }
                }
                let line = heads.borrow().clone();
                for (index, (head, options)) in line.iter().enumerate() {
                    let entry = tallies.entry(head.clone()).or_default();
                    entry.decisions += 1;
                    entry.options += options;
                    let mut tried = 0usize;
                    let mut cleared = 0usize;
                    let mut lost = 0usize;
                    for alternate in 0..*options {
                        // The recorded action's own index is unknown here, but substituting it
                        // reproduces the line and would count as a trivial clear, so every
                        // alternate is tried and the one that matches is harmless: it clears, and
                        // it is one of `options`, so the ratio is over the offer as a whole.
                        let scratch = Rc::new(RefCell::new(Vec::new()));
                        match play(
                            &table,
                            &local,
                            &trajectory,
                            Some((index, alternate)),
                            &scratch,
                        ) {
                            Ok(Some(ok)) => {
                                tried += 1;
                                replays += 1;
                                cleared += usize::from(ok);
                            }
                            _ => lost += 1,
                        }
                    }
                    entry.unevaluated += lost;
                    entry.alternates += tried;
                    entry.still_cleared += cleared;
                    if tried > 0 && cleared == tried {
                        entry.free += 1;
                    }
                    if tried > 0 && cleared <= 1 {
                        // At most the recorded action itself cleared.
                        entry.forced += 1;
                    }
                }
            }
            (tallies, replays, skipped)
        })
        .collect();

    let mut totals: BTreeMap<String, Tally> = BTreeMap::new();
    let mut replays = 0usize;
    let mut skipped = 0usize;
    for (tallies, n, bad) in harvest {
        replays += n;
        skipped += bad;
        for (head, tally) in tallies {
            let entry = totals.entry(head).or_default();
            entry.decisions += tally.decisions;
            entry.options += tally.options;
            entry.alternates += tally.alternates;
            entry.still_cleared += tally.still_cleared;
            entry.free += tally.free;
            entry.forced += tally.forced;
            entry.unevaluated += tally.unevaluated;
        }
    }

    println!(
        "  {replays} substitutions in {:.0?}, {skipped} trajectories unusable",
        started.elapsed()
    );
    println!();
    println!("    head         decisions   options   alt still clears    free    forced   lost");
    let mut rows: Vec<(&String, &Tally)> = totals.iter().collect();
    rows.sort_by(|a, b| b.1.decisions.cmp(&a.1.decisions));
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    for (head, tally) in rows {
        if tally.decisions == 0 {
            continue;
        }
        let n = tally.decisions as f64;
        println!(
            "    {head:<12} {:>7}   {:>6.1}   {:>14.1}%   {:>5.1}%   {:>6.1}%  {:>4.1}%",
            tally.decisions,
            tally.options as f64 / n,
            tally.still_cleared as f64 / tally.alternates.max(1) as f64 * 100.0,
            tally.free as f64 / n * 100.0,
            tally.forced as f64 / n * 100.0,
            tally.unevaluated as f64 / (tally.alternates + tally.unevaluated).max(1) as f64 * 100.0
        );
    }
    println!();
    println!(
        "  A head is only a real weakness where agreement is low AND 'alt still clears' is low."
    );
    println!("  Where almost every alternate clears, the demonstration did not care and imitating");
    println!("  it teaches noise.");
}
