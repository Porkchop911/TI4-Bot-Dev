//! All-seat CPU self-play cost attribution, preserving the `game_cost` workload.
//!
//! Run with --bundle PATH --rounds 4 --seeds 6. --legacy-audit uses the original
//! audit runner for differential comparison; the default measures step phases directly.
//! PHASES reports (step count, inclusive wall, wall minus complete decider wrappers).
//! GAPS/HEADGAPS associate intervals with the NEXT decision, not exclusive subsystem work.
//! CHOICES is a diagnostic Debug digest (not the versioned engine fingerprint format).

use sha2::{Digest, Sha256};
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

/// Gaps are attributed to the NEXT observation's phase, not an exclusive function profile.
#[derive(Default)]
struct Gaps {
    last: Option<Instant>,
    by_next_phase: BTreeMap<String, Duration>,
    digest: Sha256,
    wrapper: Duration,
    phases: BTreeMap<String, (usize, Duration, Duration)>,
    setup: Duration,
    by_next_head: BTreeMap<String, Duration>,
}
impl Gaps {
    fn before(&mut self, phase: &str, head: &str) {
        if let Some(last) = self.last {
            let elapsed = last.elapsed();
            *self.by_next_phase.entry(phase.to_owned()).or_default() += elapsed;
            *self.by_next_head.entry(head.to_owned()).or_default() += elapsed;
        }
    }
}

/// Times every decision without changing any of them.
struct Timing {
    inner: Box<dyn Decider>,
    gaps: Rc<RefCell<Gaps>>,
    /// Print each decision as it is taken. The engine can loop INSIDE one `step()`, where the
    /// step-limit check never runs, so the only cheap way to localise the hang is the last
    /// decision answered before it.
    trace: bool,
    who: String,
    /// Time inside the wrapped decider, summed across every seat.
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
        let wrapper_started = Instant::now();
        self.gaps
            .borrow_mut()
            .before("unobserved", ti4_policy::learned::decision_head(choice));
        let started = Instant::now();
        let chosen = self.inner.choose(choice);
        self.record(choice, started.elapsed());
        self.gaps
            .borrow_mut()
            .digest
            .update(format!("{choice:?}{chosen:?}"));
        self.gaps.borrow_mut().wrapper += wrapper_started.elapsed();
        self.gaps.borrow_mut().last = Some(Instant::now());
        chosen
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let wrapper_started = Instant::now();
        self.gaps.borrow_mut().before(
            &format!("{:?}", seen.observed().phase()),
            ti4_policy::learned::decision_head(choice),
        );
        let started = Instant::now();
        let chosen = self.inner.choose_seeing(choice, seen);
        self.record(choice, started.elapsed());
        self.gaps
            .borrow_mut()
            .digest
            .update(format!("{choice:?}{chosen:?}"));
        self.gaps.borrow_mut().wrapper += wrapper_started.elapsed();
        self.gaps.borrow_mut().last = Some(Instant::now());
        chosen
    }
}

/// Uses the same public setup path as the existing audit, but refuses every step error.
/// Step buckets use the phase at entry and subtract complete wrapper time, including hashing.
#[expect(
    clippy::too_many_arguments,
    reason = "matches the audited workload plus timing"
)]
fn profile_audit<F>(
    gaps: &Rc<RefCell<Gaps>>,
    content: &'static ContentStore,
    factions: &[FactionId],
    sources: ti4_model::content_types::SourceSet,
    seed: u64,
    rotation: usize,
    horizon: ti4_training::rollout::Horizon,
    map: &ti4_training::rollout::OpeningMap,
    factory: F,
) -> Result<ti4_training::rollout::Audited, String>
where
    F: FnOnce(
        &BTreeMap<PlayerId, FactionId>,
        &BTreeMap<PlayerId, ti4_policy::progress::Baseline>,
    ) -> Result<BTreeMap<PlayerId, Box<dyn Decider>>, String>,
{
    if std::env::args().any(|arg| arg == "--legacy-audit") {
        return ti4_training::rollout::audit_game_with_deciders(
            content, factions, sources, seed, rotation, horizon, map, factory,
        );
    }
    let players: Vec<PlayerId> = (0..factions.len())
        .map(|i| PlayerId::new(format!("seat{i}")))
        .collect();
    let assignments: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let faction = if ti4_training::rollout::seat_scramble() {
                ti4_training::rollout::scrambled_seated_faction(factions, seed, rotation, i)
            } else {
                factions[(i + rotation) % factions.len()].clone()
            };
            (p.clone(), faction)
        })
        .collect();
    let setup_started = Instant::now();
    let mut game = ti4_training::rollout::setup_game_with_decider_factory(
        content,
        &players,
        &assignments,
        sources,
        seed,
        map,
        |baselines| factory(&assignments, baselines),
    )?;
    let setup_time = setup_started.elapsed();
    let mut phases: BTreeMap<String, (usize, Duration, Duration)> = BTreeMap::new();
    let target = game.state.round.saturating_add(horizon.rounds);
    let mut steps = 0;
    while game.state.round < target && !game.state.finished && steps < horizon.steps {
        let phase = format!("{:?}", game.state.phase);
        let before = gaps.borrow().wrapper;
        let started = Instant::now();
        let result = game.step();
        let wall = started.elapsed();
        let wrapper = gaps
            .borrow()
            .wrapper
            .checked_sub(before)
            .expect("wrapper total is monotonic");
        let entry = phases.entry(phase).or_default();
        entry.0 += 1;
        entry.1 += wall;
        entry.2 += wall
            .checked_sub(wrapper)
            .expect("wrapper is nested in step");
        if let Some(error) = result.error {
            return Err(format!("step {steps}: {error:?}"));
        }
        steps += 1;
    }
    if !game.state.finished && game.state.round < target {
        return Err(format!("step limit {steps}"));
    }
    gaps.borrow_mut().phases = phases;
    gaps.borrow_mut().setup = setup_time;
    // The profiler does not consume opening metrics or the strategy snapshot. Their absence
    // removes audit-only work after stepping; state/event hashes check the trajectory separately.
    Ok((
        game.events,
        game.state.clone(),
        assignments,
        BTreeMap::new(),
        game.state,
    ))
}

#[expect(
    clippy::too_many_lines,
    reason = "keep the reference workload setup and measurement sequence together"
)]
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

    println!("all-seat engine cost");
    println!("  candidate  {bundle_path}");
    println!("  benchmark  {opponent_path}");
    println!(
        "  games      seeds {seed_base}..{}, all 6 rotations",
        seed_base + seeds
    );
    println!("  rounds     {rounds}   max steps {max_steps}");
    println!();

    let mut played: Vec<(Duration, Duration, usize)> = Vec::new();
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

                let gaps = Rc::new(RefCell::new(Gaps::default()));
                let started = Instant::now();
                let audited = profile_audit(
                    &gaps,
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
                            // will not find it. Every seat is timed.
                            let actor = if index == candidate_seat { &cand } else { &opp };
                            let (decider, _status) =
                                ti4_mlp::bot::MlpBot::sharing(actor, vocab.clone(), row, stream)
                                    .at_temperature(0.001)
                                    .from_setup(baseline)
                                    .seat();
                            deciders.insert(
                                player.clone(),
                                Box::new(Timing {
                                    inner: decider,
                                    trace,
                                    who: format!("seat{index} {faction}"),
                                    spent: Rc::clone(&s),
                                    calls: Rc::clone(&c),
                                    by_head: Rc::clone(&h),
                                    gaps: Rc::clone(&gaps),
                                }),
                            );
                        }
                        Ok(deciders)
                    },
                );
                let wall = started.elapsed();
                let tail = gaps.borrow().last.map_or(Duration::ZERO, |t| t.elapsed());
                println!(
                    "PHASES {seed}/{rotation}/{candidate_seat} setup={:?} {:?}",
                    gaps.borrow().setup,
                    gaps.borrow().phases
                );
                println!(
                    "GAPS {seed}/{rotation}/{candidate_seat} {:?} tail={tail:?}",
                    gaps.borrow().by_next_phase
                );
                println!(
                    "HEADGAPS {seed}/{rotation}/{candidate_seat} {:?}",
                    gaps.borrow().by_next_head
                );
                println!(
                    "CHOICES {seed}/{rotation}/{candidate_seat} {:x}",
                    gaps.borrow().digest.clone().finalize()
                );

                let (events, _setup, _assignments, _openings, final_state) = audited
                    .unwrap_or_else(|error| {
                        refuse(&format!(
                            "{seed}/{rotation}/seat{candidate_seat} after {wall:?}: {error}"
                        ))
                    });

                println!(
                    "STATE {seed}/{rotation}/{candidate_seat} {:x}",
                    Sha256::digest(serde_json::to_vec(&final_state).expect("state serializes"))
                );
                println!(
                    "EVENTS {seed}/{rotation}/{candidate_seat} {:x}",
                    Sha256::digest(serde_json::to_vec(&events).expect("events serialize"))
                );
                if !final_state.finished && final_state.round < rounds + 1 {
                    refuse("game did not reach requested horizon");
                }
                for (head, (count, time)) in by_head.borrow().iter() {
                    let slot = head_totals
                        .entry(head.clone())
                        .or_insert((0, Duration::ZERO));
                    slot.0 += count;
                    slot.1 += *time;
                }
                println!(
                    "GAME {seed}/{rotation}/{candidate_seat} wall={wall:?} policy={:?} wrapper={:?} calls={}",
                    *spent.borrow(),
                    gaps.borrow().wrapper,
                    *calls.borrow()
                );
                played.push((wall, *spent.borrow(), *calls.borrow()));
            }
        }
    }
    if played.is_empty() {
        refuse("no game finished");
    }
    let wall: Duration = played.iter().map(|g| g.0).sum();
    let policy: Duration = played.iter().map(|g| g.1).sum();
    let calls: usize = played.iter().map(|g| g.2).sum();
    println!(
        "TOTAL games={} wall={wall:?} policy={policy:?} residual={:?} calls={calls}",
        played.len(),
        wall.checked_sub(policy)
            .expect("deciders are nested in game wall")
    );
    println!("HEADS {head_totals:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::choice::SeededRandom;

    #[test]
    fn timing_every_seat_preserves_each_seeded_answer_and_counts_every_call() {
        let spent = Rc::new(RefCell::new(Duration::ZERO));
        let calls = Rc::new(RefCell::new(0));
        let heads = Rc::new(RefCell::new(BTreeMap::new()));
        let gaps = Rc::new(RefCell::new(Gaps::default()));
        for seed in 0..6 {
            let mut original = SeededRandom::new(seed);
            let mut timed = Timing {
                inner: Box::new(SeededRandom::new(seed)),
                trace: false,
                who: String::new(),
                spent: Rc::clone(&spent),
                calls: Rc::clone(&calls),
                by_head: Rc::clone(&heads),
                gaps: Rc::clone(&gaps),
            };
            let choice = Choice::new(
                PlayerId::new(format!("seat{seed}")),
                "test",
                vec![
                    ChoiceOption::labelled("a", "action", "A"),
                    ChoiceOption::labelled("b", "action", "B"),
                    ChoiceOption::decline(),
                ],
            );
            for _ in 0..100 {
                assert_eq!(original.choose(&choice), timed.choose(&choice));
            }
            let empty = Choice::new(choice.player.clone(), "empty", vec![]);
            assert_eq!(original.choose(&empty), timed.choose(&empty));
        }
        assert_eq!(*calls.borrow(), 606);
        assert_eq!(heads.borrow().values().map(|row| row.0).sum::<usize>(), 606);
        assert!(gaps.borrow().wrapper >= *spent.borrow());
    }
}
