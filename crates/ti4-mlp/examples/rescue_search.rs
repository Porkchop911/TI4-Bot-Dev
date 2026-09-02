//! How many of the champion's failures can a search actually construct a clear for?
//!
//! # The number this exists to produce
//!
//! `opening_reachability` finds a clearing line for ~65% of failures, and that figure has been
//! carried as the constructive bound on what is reachable — 93.58 + 0.65 x 6.42 = 97.75%. It is a
//! weak searcher's answer. This measures what a better one finds, because the difference decides
//! whether 99% is live at all.
//!
//! # Why prefix branching rather than hot sampling
//!
//! `opening_reachability` replays a failed position with the seat sampling at temperature 2.5 for
//! the *whole* line. At 2.5 the line diverges at decision zero, so every attempt throws away the
//! champion's entire opening and re-improvises it. That is a bad way to find a fix that lives at
//! decision forty.
//!
//! This is the Go-Explore idea instead: return to a promising state, explore from *there*. In a
//! deterministic engine "returning to a state" is just replaying a prefix of the recorded
//! decisions, which costs nothing and needs no snapshotting. So the search replays the champion's
//! first `k` decisions exactly and samples only the remainder, for a schedule of `k`. Late
//! branch points keep the good early play and vary only the end; early ones recover the cases where
//! the opening itself was wrong.
//!
//! The archive is therefore the champion's own line, indexed by depth, and every cell is reachable
//! exactly rather than approximately.
//!
//! # What a rescue is
//!
//! A complete decision sequence — the forced prefix plus the sampled remainder — that cleared the
//! bar. It makes no claim about which decision was responsible, which is precisely the claim that
//! sank counterfactual repair. The only thing asserted is what was demonstrated: this whole line
//! reached the bar from this position.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example rescue_search -- \
//!   --bundle out/checkpoints/sweep-A-250/checkpoint-14476 \
//!   --seeds 300 --attempts 6 --temperature 1.5 --out out/corpus/rescued
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

/// Forces a prefix of recorded decisions, then lets the seat decide, recording everything.
struct Branching {
    inner: Box<dyn Decider>,
    /// The champion's line. Only the first `depth` entries are imposed.
    script: Vec<String>,
    depth: usize,
    seen: usize,
    log: Rc<RefCell<Vec<Note>>>,
    /// Set if a forced id was not on offer, which means the prefix did not reproduce.
    broken: Rc<RefCell<bool>>,
}

impl Branching {
    fn answer(
        &mut self,
        choice: &Choice,
        delegate: impl FnOnce(&mut Box<dyn Decider>) -> Result<ChoiceOption, IllegalChoice>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        // Forced decisions carry no preference, are absent from the corpus, and must not advance
        // the counter -- advancing here would shift the branch point.
        if choice.options.len() < 2 {
            return delegate(&mut self.inner);
        }
        let index = self.seen;
        self.seen += 1;

        let chosen = if index < self.depth {
            // Inside the prefix: impose the champion's decision, by id so a drifted offer fails
            // closed instead of silently selecting something else.
            let wanted = &self.script[index];
            match choice.options.iter().find(|option| option.id == *wanted) {
                Some(option) => {
                    // The bot still answers, so its own machinery stays in step; the answer is
                    // discarded.
                    let _ = delegate(&mut self.inner)?;
                    option.clone()
                }
                None => {
                    *self.broken.borrow_mut() = true;
                    return Err(IllegalChoice::DeciderFailed {
                        player: choice.player.clone(),
                        prompt: choice.prompt.clone(),
                        reason: format!("prefix option {wanted:?} is not on offer"),
                    });
                }
            }
        } else {
            delegate(&mut self.inner)?
        };

        let head = ti4_mlp::Actor::resolve_head(ti4_policy::learned::decision_head(choice));
        self.log.borrow_mut().push(Note {
            head: head.to_owned(),
            chosen: chosen.id.clone(),
            declined: chosen.is_decline(),
        });
        Ok(chosen)
    }
}

impl Decider for Branching {
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

/// What one play produced for the target seat.
struct Outcome {
    cleared: bool,
    planets: usize,
    systems: usize,
    units_ok: bool,
    notes: Vec<Note>,
    broken: bool,
}

/// Play one game with the target seat branching from `depth`.
///
/// `temperature` is the target seat's; the other five always play greedily, because they are the
/// opponents the failure happened against and perturbing them would change the position.
#[expect(
    clippy::too_many_arguments,
    reason = "one call site, all of it required"
)]
fn play(
    table: &Table<'_>,
    actor: &Rc<ti4_mlp::Actor>,
    seed: u64,
    rotation: usize,
    faction: &str,
    script: &[String],
    depth: usize,
    temperature: f64,
    stream_salt: u64,
) -> Result<Outcome, String> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let broken = Rc::new(RefCell::new(false));
    let captured = Rc::clone(&log);
    let fault = Rc::clone(&broken);
    let script = script.to_vec();
    let want = faction.to_owned();

    let played = ti4_training::rollout::audit_game_with_deciders(
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
                    .ok_or_else(|| format!("{player} has no baseline"))?;
                let mine = seat_faction.as_str() == want;
                // Only the searching seat's stream is salted, so every attempt explores a
                // different continuation while the five opponents stay exactly as they were.
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0))
                    .wrapping_add(if mine { stream_salt } else { 0 });
                let bot =
                    ti4_mlp::bot::MlpBot::sharing(actor, table.vocabulary.clone(), row, stream)
                        .at_temperature(if mine { temperature } else { 0.001 })
                        .from_setup(baseline);
                let (decider, _status) = bot.seat();
                if mine {
                    deciders.insert(
                        player.clone(),
                        Box::new(Branching {
                            inner: decider,
                            script: script.clone(),
                            depth,
                            seen: 0,
                            log: Rc::clone(&captured),
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
        return Ok(Outcome {
            cleared: false,
            planets: 0,
            systems: 0,
            units_ok: false,
            notes: Vec::new(),
            broken: true,
        });
    }
    let (_e, _s, assignments, openings, _f) = played?;
    for (player, opening) in &openings {
        if assignments
            .get(player)
            .is_some_and(|seated| seated.as_str() == want)
        {
            return Ok(Outcome {
                cleared: opening.cleared(),
                planets: opening.planets_gained,
                systems: opening.systems,
                units_ok: opening.units_ok(),
                notes: log.borrow().clone(),
                broken: false,
            });
        }
    }
    Err(format!("{want} was not seated in {seed}/{rotation}"))
}

#[expect(
    clippy::too_many_lines,
    reason = "one search and the census it reports"
)]
fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let seeds: u64 = number("--seeds", 300);
    let seed_base: u64 = number("--seed-base", 800_000_000);
    let attempts: usize = number("--attempts", 6);
    let branches: usize = number("--branches", 10);
    let temperature: f64 = number("--temperature", 1.5);
    let max_failures: usize = number("--max-failures", usize::MAX);
    let out = argument("--out");

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
    let git_commit = std::env::var("GIT_COMMIT").unwrap_or_else(|_| "unrecorded".to_owned());

    println!("rescue search from {bundle_path}");
    println!("  seeds        {seed_base}..{}", seed_base + seeds);
    println!("  branch       {branches} depths x {attempts} attempts at temperature {temperature}");
    println!();

    // ---- the champion's failures, and its line at each ----------------------------------------
    let started = std::time::Instant::now();
    let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers).max(1);

    let found: Vec<Result<(usize, Vec<(u64, usize, String, Vec<String>)>), String>> = jobs
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
            let mut seats = 0usize;
            let mut failures = Vec::new();
            for (seed, rotation) in chunk {
                for faction in FACTIONS {
                    // Depth 0 with the greedy temperature *is* the champion's own line.
                    let outcome = play(&table, &local, seed, rotation, faction, &[], 0, 0.001, 0)?;
                    seats += 1;
                    if !outcome.cleared {
                        failures.push((
                            seed,
                            rotation,
                            faction.to_owned(),
                            outcome.notes.iter().map(|n| n.chosen.clone()).collect(),
                        ));
                    }
                }
            }
            Ok((seats / FACTIONS.len(), failures))
        })
        .collect();

    let mut seat_games = 0usize;
    let mut failures: Vec<(u64, usize, String, Vec<String>)> = Vec::new();
    for chunk in found {
        let (n, mut rows) = chunk.unwrap_or_else(|error| refuse(&error));
        seat_games += n * FACTIONS.len();
        failures.append(&mut rows);
    }
    failures.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
    let all = failures.len();
    failures.truncate(max_failures);
    println!(
        "  {seat_games} seat-games, {all} failures; searching {} ({:.0?} to find them)",
        failures.len(),
        started.elapsed()
    );
    println!();

    // ---- branch from each depth ---------------------------------------------------------------
    let searching = std::time::Instant::now();
    let per_worker = failures.len().div_ceil(workers).max(1);
    let harvest: Vec<Vec<(u64, usize, String, usize, Vec<Vec<Note>>)>> = failures
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
            let mut out = Vec::new();
            for (seed, rotation, faction, line) in chunk {
                let mut rescues: Vec<Vec<Note>> = Vec::new();
                let mut tried = 0usize;
                if line.is_empty() {
                    out.push((seed, rotation, faction, 0, rescues));
                    continue;
                }
                // Depths spread over the line, deepest first: a late branch keeps the champion's
                // whole opening and varies only the end, which is the cheapest kind of fix and the
                // one hot whole-line sampling can never find.
                for step in 0..branches {
                    let depth = line
                        .len()
                        .saturating_sub((step + 1) * line.len() / branches.max(1));
                    for attempt in 0..attempts {
                        tried += 1;
                        let salt = 1 + (step as u64 * 977) + (attempt as u64 * 31);
                        match play(
                            &table,
                            &local,
                            seed,
                            rotation,
                            &faction,
                            &line,
                            depth,
                            temperature,
                            salt,
                        ) {
                            Ok(outcome) if outcome.cleared && !outcome.broken => {
                                rescues.push(outcome.notes);
                            }
                            Ok(_) => {}
                            Err(_) => {}
                        }
                    }
                }
                out.push((seed, rotation, faction, tried, rescues));
            }
            out
        })
        .collect();

    let mut rescued = 0usize;
    let mut total_rescues = 0usize;
    let mut clean_rescues = 0usize;
    let mut replays = 0usize;
    let mut by_faction: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut kept: BTreeMap<String, Vec<Trajectory>> = BTreeMap::new();

    for chunk in harvest {
        for (seed, rotation, faction, tried, rescues) in chunk {
            replays += tried;
            let entry = by_faction.entry(faction.clone()).or_insert((0, 0));
            entry.0 += 1;
            if !rescues.is_empty() {
                rescued += 1;
                entry.1 += 1;
            }
            total_rescues += rescues.len();
            for notes in rescues {
                // The same admission test the positive corpus uses: cleared, and no activation
                // that did nothing. A hot continuation is exactly where a pointless activation
                // would come from, so the filter matters more here, not less.
                if wasted_activations(&notes) > 0 {
                    continue;
                }
                clean_rescues += 1;
                kept.entry(faction.clone()).or_default().push(Trajectory {
                    seed,
                    rotation,
                    faction: faction.clone(),
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a positive temperature under a thousand"
                    )]
                    temperature_milli: (temperature * 1_000.0) as u64,
                    planets: 0,
                    systems: 0,
                    units_ok: true,
                    actions: actions_taken(&notes),
                    decisions: notes.iter().map(|n| n.chosen.clone()).collect(),
                });
            }
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let share = |part: usize, whole: usize| -> f64 {
        if whole == 0 {
            0.0
        } else {
            part as f64 / whole as f64 * 100.0
        }
    };

    println!("  {replays} replays in {:.0?}", searching.elapsed());
    println!();
    println!("  CONSTRUCTIVE COVERAGE OF CHAMPION FAILURES");
    println!();
    println!(
        "    {rescued} of {} failures have at least one clearing line   {:.1}%",
        failures.len(),
        share(rescued, failures.len())
    );
    println!("    {total_rescues} clearing lines found, {clean_rescues} with no wasted activation");
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let per_rescued = total_rescues as f64 / rescued.max(1) as f64;
    println!("    {per_rescued:.1} clearing lines per rescued start");
    println!();
    println!("    faction      failures   rescued");
    for (faction, (total, fixed)) in &by_faction {
        println!(
            "    {faction:<12} {total:>8}   {fixed:>5}  {:>5.1}%",
            share(*fixed, *total)
        );
    }
    println!();

    // What this implies for the ceiling, stated as the arithmetic it is.
    // The *whole* failure population, not the searched subset. Using the truncated count made a
    // capped run report a 2.08% failure rate against a true 5.56%, and therefore a reachable
    // ceiling far higher than the evidence supports.
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let failure_rate = all as f64 / seat_games.max(1) as f64 * 100.0;
    let coverage = share(rescued, failures.len()) / 100.0;
    println!(
        "    failure rate {failure_rate:.2}%, coverage {:.1}% -> constructively reachable {:.2}%",
        coverage * 100.0,
        100.0 - failure_rate + failure_rate * coverage
    );

    if let Some(path) = out {
        let directory = std::path::Path::new(&path);
        std::fs::create_dir_all(directory)
            .unwrap_or_else(|error| refuse(&format!("creating {}: {error}", directory.display())));
        let mut written = 0usize;
        for (faction, rows) in &kept {
            let mut body = String::new();
            for trajectory in rows {
                body.push_str(
                    &write_line(trajectory)
                        .unwrap_or_else(|error| refuse(&format!("writing {faction}: {error}"))),
                );
                body.push('\n');
            }
            std::fs::write(directory.join(format!("{faction}.corpus")), body)
                .unwrap_or_else(|error| refuse(&format!("writing {faction}: {error}")));
            written += rows.len();
        }
        std::fs::write(
            directory.join("manifest.txt"),
            format!(
                "schema ti4-rescued-corpus-v1\nbundle {bundle_path}\ncommit {git_commit}\nseeds \
                 {seed_base}..{}\ntemperature {temperature}\nbranches {branches}\nattempts \
                 {attempts}\nfailures {}\nrescued {rescued}\nkept {written}\n",
                seed_base + seeds,
                failures.len()
            ),
        )
        .unwrap_or_else(|error| refuse(&format!("writing the manifest: {error}")));
        println!();
        println!(
            "  wrote {written} rescued trajectories to {}",
            directory.display()
        );
    }
}
