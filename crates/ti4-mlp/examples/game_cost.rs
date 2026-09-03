//! Why some stage-2 games cannot be played in bounded time, and where the victory points come from.
//!
//! Two problems the overnight campaign left open, and one question about its results. All three
//! need the same instrument: play games one at a time and record what each cost and what it
//! produced, rather than an aggregate that hides the outliers.
//!
//! # Cost
//!
//! Seven of run 3's twenty checkpoints could not be evaluated inside four minutes each, where the
//! stage-1 champion plays 144 games in three seconds. Lowering the step cap did not help and no
//! game reported a step-limit truncation, so the cost is not the NUMBER of decisions. This splits
//! wall time into time spent inside the decider (the network) and time spent in the engine between
//! decisions, which separates "the policy is slow to ask" from "the position is slow to resolve".
//!
//! # Points
//!
//! Per-faction victory points differ by more than a factor of two -- Sol above five, Hacan and
//! L1Z1X near two -- and the count of scoring OFFERS runs the other way, with L1Z1X offered the
//! most and scoring the least. So offers do not explain points. `scored_objectives` records what
//! each seat actually scored and `points_for` prices it, so the residual against final victory
//! points is everything that is not an objective: Mecatol, relics, agendas, laws.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example game_cost -- \
//!   --bundle out/champions/stage2-r5-m2.526_clear93.75 \
//!   --opponent out/champions/best-94.97_r2-epoch22 \
//!   --seeds 6 --seed-base 900000100 --rounds 4
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

fn number<T: std::str::FromStr>(flag: &str, fallback: T) -> T {
    argument(flag).map_or(fallback, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{flag} expects a number")))
    })
}

/// Times every decision without changing any of them.
struct Timing {
    inner: Box<dyn Decider>,
    /// Print each decision as it is taken. The engine can loop INSIDE one `step()`, where the
    /// step-limit check never runs, so the only cheap way to localise the hang is the last
    /// decision answered before it.
    trace: bool,
    who: String,
    /// Time inside the wrapped decider, summed. Everything else in the wall time is the engine
    /// advancing between decisions.
    spent: Rc<RefCell<Duration>>,
    calls: Rc<RefCell<usize>>,
    /// Per head, so a slow policy can be told apart from one slow kind of decision.
    by_head: Rc<RefCell<BTreeMap<String, (usize, Duration)>>>,
}

impl Timing {
    fn record(&self, choice: &Choice, elapsed: Duration) {
        if self.trace {
            println!(
                "    [{}] {} {} option(s): {}",
                self.who,
                ti4_policy::learned::decision_head(choice),
                choice.options.len(),
                choice.prompt.chars().take(90).collect::<String>()
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        *self.spent.borrow_mut() += elapsed;
        *self.calls.borrow_mut() += 1;
        let head = ti4_policy::learned::decision_head(choice).to_owned();
        let mut heads = self.by_head.borrow_mut();
        let slot = heads.entry(head).or_insert((0, Duration::ZERO));
        slot.0 += 1;
        slot.1 += elapsed;
    }
}

impl Decider for Timing {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let started = Instant::now();
        let chosen = self.inner.choose(choice);
        self.record(choice, started.elapsed());
        chosen
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let started = Instant::now();
        let chosen = self.inner.choose_seeing(choice, seen);
        self.record(choice, started.elapsed());
        chosen
    }
}

struct Played {
    seed: u64,
    rotation: usize,
    candidate_seat: usize,
    wall: Duration,
    in_decider: Duration,
    decisions: usize,
    units: usize,
    /// The CANDIDATE seat only: faction, victory points, points from scored objectives, and the
    /// number of objectives scored. Averaging all six seats answers a different question, because
    /// five of them are the frozen benchmark and drag every mean toward its play, not the
    /// candidate's -- which is exactly what the first version of this tool did.
    seat: (String, i64, i32, usize),
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let opponent_path = argument("--opponent").unwrap_or_else(|| bundle_path.clone());
    let seeds: u64 = number("--seeds", 6);
    let seed_base: u64 = number("--seed-base", 900_000_100);
    let rounds: u32 = number("--rounds", 4);
    let max_steps: usize = number("--max-steps", 400_000);
    // Trace every seat, not just the candidate: a loop inside one engine step may follow any
    // seat's decision, and the answer is the LAST line printed before the hang.
    let trace = std::env::args().any(|arg| arg == "--trace");
    let only_rotation: Option<usize> = argument("--only-rotation").and_then(|v| v.parse().ok());
    let only_seat: Option<usize> = argument("--only-seat").and_then(|v| v.parse().ok());

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let candidate = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let opponent = ti4_mlp::bundle::read(std::path::Path::new(&opponent_path))
        .unwrap_or_else(|error| refuse(&format!("reading {opponent_path}: {error}")));
    let vocabulary = candidate.vocabulary.clone();
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            ti4_sim::artifacts::read_and_verify_pool_role(
                std::path::Path::new("out/pools/full_np8_12_holdout.json"),
                &[ti4_sim::artifacts::ArtifactRole::Validation],
            )
            .unwrap_or_else(|error| refuse(&format!("holdout pool: {error}"))),
        ))
        .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();
    let candidate_actor = Rc::new(candidate.actor.inference_copy());
    let opponent_actor = Rc::new(opponent.actor.inference_copy());

    println!("game cost and victory-point sources");
    println!("  candidate  {bundle_path}");
    println!("  benchmark  {opponent_path}");
    println!(
        "  games      seeds {seed_base}..{}, all 6 rotations",
        seed_base + seeds
    );
    println!("  rounds     {rounds}   max steps {max_steps}");
    println!();

    let mut played: Vec<Played> = Vec::new();
    let mut head_totals: BTreeMap<String, (usize, Duration)> = BTreeMap::new();

    for seed in seed_base..seed_base + seeds {
        for rotation in 0..FACTIONS.len() {
            if only_rotation.is_some_and(|want| want != rotation) {
                continue;
            }
            for candidate_seat in 0..FACTIONS.len() {
                if only_seat.is_some_and(|want| want != candidate_seat) {
                    continue;
                }
                let spent = Rc::new(RefCell::new(Duration::ZERO));
                let calls = Rc::new(RefCell::new(0usize));
                let by_head = Rc::new(RefCell::new(BTreeMap::new()));
                let (s, c, h) = (Rc::clone(&spent), Rc::clone(&calls), Rc::clone(&by_head));
                let (cand, opp, vocab) = (
                    Rc::clone(&candidate_actor),
                    Rc::clone(&opponent_actor),
                    vocabulary.clone(),
                );

                let started = Instant::now();
                let audited = ti4_training::rollout::audit_game_with_deciders(
                    content,
                    &factions,
                    DEFAULT,
                    seed,
                    rotation,
                    ti4_training::rollout::Horizon {
                        rounds,
                        steps: max_steps,
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
                            let baseline = baselines
                                .get(player)
                                .copied()
                                .ok_or_else(|| format!("{player} has no baseline"))?;
                            let stream = seed
                                .wrapping_mul(1_000_003)
                                .wrapping_add(u64::try_from(index).unwrap_or(0));
                            // The candidate takes each seat in turn, as cross-play does, because a rare
                            // pathological game need not be in seat 0 and a four-seed sweep of one seat
                            // will not find it. Only the candidate seat is timed.
                            let actor = if index == candidate_seat { &cand } else { &opp };
                            let (decider, _status) =
                                ti4_mlp::bot::MlpBot::sharing(actor, vocab.clone(), row, stream)
                                    .at_temperature(0.001)
                                    .from_setup(baseline)
                                    .seat();
                            if index == candidate_seat {
                                deciders.insert(
                                    player.clone(),
                                    Box::new(Timing {
                                        inner: decider,
                                        trace,
                                        who: format!("seat{index} {faction} CANDIDATE"),
                                        spent: Rc::clone(&s),
                                        calls: Rc::clone(&c),
                                        by_head: Rc::clone(&h),
                                    }),
                                );
                            } else if trace {
                                // Benchmark seats are traced but not counted, so the timing totals still
                                // describe the candidate alone.
                                deciders.insert(
                                    player.clone(),
                                    Box::new(Timing {
                                        inner: decider,
                                        trace,
                                        who: format!("seat{index} {faction}"),
                                        spent: Rc::new(RefCell::new(Duration::ZERO)),
                                        calls: Rc::new(RefCell::new(0)),
                                        by_head: Rc::new(RefCell::new(BTreeMap::new())),
                                    }),
                                );
                            } else {
                                deciders.insert(player.clone(), decider);
                            }
                        }
                        Ok(deciders)
                    },
                );
                let wall = started.elapsed();

                let Ok((_events, _setup, assignments, _openings, final_state)) = audited else {
                    println!("  {seed}/{rotation}/seat{candidate_seat}  FAILED after {wall:.1?}");
                    continue;
                };

                let units: usize = final_state
                    .board
                    .values()
                    .map(|system| {
                        system.units.len()
                            + system.planet_units.values().map(Vec::len).sum::<usize>()
                    })
                    .sum();

                let me = PlayerId::new(format!("seat{candidate_seat}"));
                let Some(seat) = final_state.players.iter().find(|seat| seat.id == me) else {
                    println!("  {seed}/{rotation}/seat{candidate_seat}  candidate not seated");
                    continue;
                };
                let faction = assignments
                    .get(&seat.id)
                    .map_or_else(|| "?".to_owned(), ToString::to_string);
                let scored = final_state.scored_objectives.get(&seat.id);
                let from_objectives: i32 = scored.map_or(0, |set| {
                    set.iter()
                        .filter_map(|alias| ti4_engine::objectives::points_for(content, alias))
                        .sum()
                });
                let seat_row = (
                    faction,
                    i64::from(seat.victory_points),
                    from_objectives,
                    scored.map_or(0, std::collections::BTreeSet::len),
                );

                for (head, (count, time)) in by_head.borrow().iter() {
                    let slot = head_totals
                        .entry(head.clone())
                        .or_insert((0, Duration::ZERO));
                    slot.0 += count;
                    slot.1 += *time;
                }

                println!(
                    " {:.0?} ({} decisions, {} units)",
                    wall,
                    *calls.borrow(),
                    units
                );
                played.push(Played {
                    seed,
                    rotation,
                    candidate_seat,
                    wall,
                    in_decider: *spent.borrow(),
                    decisions: *calls.borrow(),
                    units,
                    seat: seat_row,
                });
            }
        }
    }

    if played.is_empty() {
        refuse("no game finished");
    }

    println!("=== cost per game, slowest first ===");
    println!("  seed/rot/seat       wall   in decider   in engine   decisions   units on board");
    let mut order: Vec<&Played> = played.iter().collect();
    order.sort_by(|a, b| b.wall.cmp(&a.wall));
    for game in order.iter().take(12) {
        let engine = game.wall.saturating_sub(game.in_decider);
        println!(
            "  {}/{}/{}  {:>9.2?}  {:>10.2?}  {:>10.2?}  {:>9}  {:>8}",
            game.seed,
            game.rotation,
            game.candidate_seat,
            game.wall,
            game.in_decider,
            engine,
            game.decisions,
            game.units
        );
    }

    let total: Duration = played.iter().map(|g| g.wall).sum();
    let decider: Duration = played.iter().map(|g| g.in_decider).sum();
    let decisions: usize = played.iter().map(|g| g.decisions).sum();
    println!();
    println!(
        "  {} games in {:.1?}: {:.1?} inside the decider, {:.1?} in the engine, {decisions} decisions",
        played.len(),
        total,
        decider,
        total.saturating_sub(decider)
    );
    let slowest = order.first().expect("checked non-empty");
    let fastest = order.last().expect("checked non-empty");
    println!(
        "  slowest game {:.2?} against fastest {:.2?}: a factor of {:.0}",
        slowest.wall,
        fastest.wall,
        slowest.wall.as_secs_f64() / fastest.wall.as_secs_f64().max(1e-9)
    );

    println!();
    println!("=== decision cost by head (candidate seat only) ===");
    println!("  head             calls        total       per call");
    let mut heads: Vec<(&String, &(usize, Duration))> = head_totals.iter().collect();
    heads.sort_by(|a, b| b.1.1.cmp(&a.1.1));
    for (head, (count, time)) in heads {
        let per = time
            .checked_div(u32::try_from(*count).unwrap_or(1))
            .unwrap_or_default();
        println!("  {head:<14} {count:>8}  {time:>11.2?}  {per:>13.2?}");
    }

    println!();
    println!("=== victory points by faction, CANDIDATE SEAT ONLY, and where they came from ===");
    println!("  faction      games       VP   from objectives   other   objectives scored");
    let mut by_faction: BTreeMap<String, (usize, i64, i64, usize)> = BTreeMap::new();
    for game in &played {
        let (faction, vp, from_objectives, count) = &game.seat;
        let slot = by_faction.entry(faction.clone()).or_insert((0, 0, 0, 0));
        slot.0 += 1;
        slot.1 += vp;
        slot.2 += i64::from(*from_objectives);
        slot.3 += count;
    }
    for (faction, (games, vp, from_objectives, count)) in &by_faction {
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let n = *games as f64;
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let line = format!(
            "  {faction:<12} {games:>5}   {:>6.3}   {:>15.3}   {:>5.3}   {:>16.3}",
            *vp as f64 / n,
            *from_objectives as f64 / n,
            (*vp - *from_objectives) as f64 / n,
            *count as f64 / n
        );
        println!("{line}");
    }
    println!();
    println!("  'other' is every point that is not a scored objective: Mecatol's custodians,");
    println!("  relics, agendas and laws. A faction that leads on VP but not on objectives is");
    println!("  winning its points somewhere the scoring head never sees.");
}
