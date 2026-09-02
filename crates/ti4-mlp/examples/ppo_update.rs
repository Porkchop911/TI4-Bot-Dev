//! M10-034: one PPO update from MLP self-play, end to end.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example ppo_update -- [--updates 1] [--device cuda]
//! ```
//!
//! §6.3's unit of work: 16 game seeds × six rotations of self-play, the behaviour
//! log-probabilities, returns and values stored **before** optimisation, then four epochs of the
//! clipped surrogate over 4,096-decision minibatches with the advantage frozen throughout.
//!
//! # Rollouts stay on the CPU
//!
//! §7.1 admits no CUDA inference backend, so every action is selected by the deterministic CPU
//! path. `--device cuda` places the trained model on the device; self-play runs from a CPU
//! inference *copy*, and the training actor is never moved — moving a tensor that requires a
//! gradient replaces it with a non-leaf view, and the gradients then land on the leaves left
//! behind.
//!
//! Games are played in parallel across rayon workers, one owned actor copy per worker.
//!
//! This exists to be measured as much as to run: every estimate of what M10-038's 30,000 updates
//! would cost has so far been an extrapolation, and one real update replaces all of them.

#![allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::arc_with_non_send_sync,
    reason = "a driver: the phases read in the order they run"
)]

use rayon::prelude::*;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::Decider;
use ti4_mlp::bundle::CriticMode;
use ti4_mlp::ppo::{Batch, Settings, Step};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;
/// §6.3: "Each update is 16 game seeds × six rotations."
const SEEDS_PER_UPDATE: u64 = 16;

/// Recorded decisions one seat may contribute from one game before the game is refused.
///
/// Two orders of magnitude above a healthy seat-game (~43 decisions), so it catches a game that has
/// stopped progressing and never a long one. See the refusal in `play_one` for why a step limit
/// alone was not enough.
const MAX_DECISIONS_PER_SEAT: usize = 4_000;
/// Rounds per self-play game, by stage.
///
/// Stage 2 pays for victory points and needs the four-round horizon §6.1 defines. Stage 1 pays for
/// the opening, which is decided in round one — playing three more rounds would add three rounds of
/// noise to a signal that is already complete, and cost four times the compute to do it.
const fn rounds_for(stage: ti4_training::reward::Stage) -> u32 {
    match stage {
        ti4_training::reward::Stage::One => 1,
        ti4_training::reward::Stage::Two => 4,
    }
}
/// §6.3's pilot seed base, so a run is reproducible from its update number alone.
/// Where a run's self-play seeds start. `--seed-base` moves it.
///
/// A run consumes `SEEDS_PER_UPDATE` seeds per update, so a later run that starts here replays the
/// same maps and the same openings an earlier one already trained on. Fresh weights on stale seeds
/// measure how well the policy does on games it has seen, which is not the question.
const SEED_BASE: u64 = 650_000_000;

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

/// One self-play game, recorded as PPO steps with §6.1's shaped per-decision returns.
///
/// Records what a seat did, and never changes it.
///
/// Exists so a wasted activation can be charged to the decision that made it. The engine's event
/// log carries names without an owner and cannot say whose activation it was; the seat's own
/// decision stream can.
struct Watching {
    inner: Box<dyn Decider>,
    log: std::rc::Rc<std::cell::RefCell<Vec<ti4_mlp::positive_corpus::Note>>>,
}

impl Watching {
    fn record(&self, choice: &Choice, chosen: &ti4_engine::choice::ChoiceOption) {
        // Forced decisions are absent from `MlpBot::record` too, so the indices of this log and the
        // recorded PPO steps line up. Counting them here and not there would shift every charge
        // after the first forced decision onto the wrong decision.
        if choice.options.len() < 2 {
            return;
        }
        let head = ti4_mlp::Actor::resolve_head(ti4_policy::learned::decision_head(choice));
        self.log.borrow_mut().push(ti4_mlp::positive_corpus::Note {
            head: head.to_owned(),
            chosen: chosen.id.clone(),
            declined: chosen.is_decline(),
        });
    }
}

impl Decider for Watching {
    fn choose(
        &mut self,
        choice: &Choice,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        let chosen = self.inner.choose(choice)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &ti4_engine::choice::SeatObservation<'_>,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        let chosen = self.inner.choose_seeing(choice, seen)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }
}

/// Everything a game needs is passed in rather than captured, because this runs on a rayon worker:
/// `tch::Tensor` is `Send` but **not** `Sync`, so the actor cannot be shared by reference across
/// threads and each worker owns its own inference copy.
#[expect(
    clippy::too_many_arguments,
    reason = "a game's inputs; bundling them into a struct would move the list, not shorten it"
)]
fn play_one(
    actor: &std::rc::Rc<ti4_mlp::Actor>,
    content: &ContentStore,
    players: &[PlayerId],
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    pool: &Arc<ti4_sim::MapPool>,
    reward: &ti4_training::reward::Reward,
    critic_mode: ti4_mlp::bundle::CriticMode,
    rounds: u32,
    seed: u64,
    rotation: usize,
    temperature: f64,
    waste_penalty: f64,
) -> Result<Played, String> {
    let seated: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            (
                player.clone(),
                FactionId::new(FACTIONS[(index + rotation) % FACTIONS.len()]),
            )
        })
        .collect();

    // Deciders are built by a factory so each seat gets the **exact** post-deployment baseline the
    // rollout will score its final progress against. Constructing them earlier cannot supply that,
    // and a shaped return measured against a different baseline is not the return §6.1 defines
    // (F-M10-034-D1).
    //
    // The handles are `Rc`, which is exactly why they are created, filled and drained inside this
    // function: nothing thread-local ever crosses back to the caller.
    let mut handles: BTreeMap<PlayerId, _> = BTreeMap::new();
    let mut watched: BTreeMap<
        PlayerId,
        std::rc::Rc<std::cell::RefCell<Vec<ti4_mlp::positive_corpus::Note>>>,
    > = BTreeMap::new();
    let rollout = ti4_training::rollout::play_with_decider_factory(
        content,
        players,
        &seated,
        DEFAULT,
        seed,
        ti4_training::rollout::Horizon {
            rounds,
            steps: 10_000,
        },
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        &ti4_training::rollout::OpeningMap::PythonPool {
            pool: Arc::clone(pool),
            tile_seed_offset: TILE_SEED_OFFSET,
        },
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
                // The seats share this worker's one actor. Inference never mutates it, and a
                // per-seat `inference_copy` meant 96 games x 6 seats of deep tensor copies per
                // update — gigabytes of allocation to produce identical read-only weights.
                // Reading the *bundle* here, which the first version did, was worse still: an
                // SHA-256 over ~17 MB and a reparse of 1.1 MB of slots.json per seat per game.
                let bot = ti4_mlp::bot::MlpBot::sharing(actor, vocabulary.clone(), row, stream)
                    .at_temperature(temperature)
                    .recording_ppo(critic_mode)
                    .from_setup(baseline);
                if handles.insert(player.clone(), bot.ppo_records()).is_some() {
                    return Err(format!("{player} was seated twice"));
                }
                let (decider, _status) = bot.seat();
                let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                watched.insert(player.clone(), std::rc::Rc::clone(&log));
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
    );
    if let Some(error) = &rollout.error {
        return Err(format!("self-play game {seed}/{rotation} failed: {error}"));
    }

    // The returns, matched to the seat that earned them. A missing handle is refused rather than
    // skipped: silently dropping a seat shrinks the batch and nothing downstream would notice
    // (F-M10-034-D4).
    let mut steps: Vec<Step> = Vec::new();
    let mut outcomes: Vec<SeatOutcome> = Vec::new();
    let mut wasted = 0usize;
    for seat in &rollout.seats {
        outcomes.push(SeatOutcome {
            faction: seat.faction.to_string(),
            cleared: seat.episode.cleared,
            victory_points: seat.episode.final_progress.victory_points,
        });
        let handle = handles.get(&seat.player).ok_or_else(|| {
            format!(
                "seed {seed} rotation {rotation}: {} has no recording handle",
                seat.player
            )
        })?;
        let mut recorded = handle.borrow_mut();
        // A single seat cannot contribute more decisions than a sane game has.
        //
        // Each recorded decision stores the sparse feature vector of *every* legal option, which is
        // what the importance ratio needs and what makes memory scale with steps x options. A game
        // that stops progressing therefore does not merely run long: it allocates. One at
        // temperature 0.25 reached 53 GB on a 96 GB machine, at which point every engine step was a
        // page fault and the update that would have taken 1.5 seconds had not finished 38 minutes
        // later. It never reached the step limit, so nothing named it and nothing refused it.
        //
        // Refusing here turns that into a fast, named failure with the seed and seat attached,
        // *before* the machine starts swapping. The ceiling is far above any real game -- a whole
        // healthy update is ~25,000 decisions across 96 games and six seats, so ~43 per seat-game.
        if recorded.len() > MAX_DECISIONS_PER_SEAT {
            return Err(format!(
                "seed {seed} rotation {rotation} {}: {} recorded decisions exceeds the {} ceiling;                  the game was not progressing",
                seat.player,
                recorded.len(),
                MAX_DECISIONS_PER_SEAT
            ));
        }
        // §6.1's shaped per-decision return. Each recorded decision carries the progress measured
        // **at** that decision against the seat's own setup baseline, so `returns` can telescope
        // them into a return-to-go per decision.
        //
        // The first version built a one-step episode from the final progress and gave every
        // decision in the game the same number. The advantage is `return − V(s)`, so with a
        // constant return the only thing separating decisions was the critic, and the within-game
        // credit assignment §6.1's shaping exists for was gone. It trained; the objective was wrong.
        let episode = ti4_training::reward::Episode {
            steps: recorded.iter().map(|record| record.progress).collect(),
            final_progress: seat.episode.final_progress,
            cleared: seat.episode.cleared,
            shortfall: seat.episode.shortfall,
            traded_goods: seat.episode.traded_goods,
        };
        let per_decision = ti4_training::reward::returns(&episode, reward);
        if per_decision.len() != recorded.len() {
            return Err(format!(
                "seed {seed} rotation {rotation} {}: {} returns for {} decisions",
                seat.player,
                per_decision.len(),
                recorded.len()
            ));
        }
        for (record, value) in recorded.iter_mut().zip(&per_decision) {
            record.step.return_to_go = *value;
        }

        // Charge each wasted activation to the activation decision that made it.
        //
        // Nothing in the stage-1 reward objects to a tactical action that activates a system and
        // then neither moves, builds, nor lands: clearance is all it prices, and a seat that has
        // already met the bar is free to spend its last turn on nothing. Measured, the champion
        // does exactly that in 60.6% of its games -- roughly a fifth of a seat's ~4.6 actions.
        //
        // The charge lands on the activation itself rather than on the episode. Spreading it over
        // fifty decisions would put 98% of the gradient on decisions that were not the mistake,
        // which is the credit-assignment failure this project has already paid for once.
        // Counted always, charged only when a penalty is set. Gating the *count* on the penalty
        // made the zero-penalty control report 0.000 wasted activations, which is the one arm whose
        // whole purpose is to say what PPO does to waste when nothing objects to it.
        if let Some(notes) = watched.get(&seat.player) {
            let notes = notes.borrow();
            if notes.len() == recorded.len() {
                for index in ti4_mlp::positive_corpus::wasted_activation_indices(&notes) {
                    wasted += 1;
                    if waste_penalty > 0.0
                        && let Some(record) = recorded.get_mut(index)
                    {
                        record.step.return_to_go -= waste_penalty;
                    }
                }
            } else {
                // The two logs must describe the same decisions. If they do not, the indices mean
                // nothing and charging by them would penalise arbitrary decisions.
                return Err(format!(
                    "seed {seed} rotation {rotation} {}: {} notes for {} recorded decisions",
                    seat.player,
                    notes.len(),
                    recorded.len()
                ));
            }
        }
        steps.extend(recorded.drain(..).map(|record| record.step));
    }
    Ok((steps, outcomes, wasted))
}

/// One played game: the decisions it contributed and what each seat ended with.
type Played = (Vec<Step>, Vec<SeatOutcome>, usize);

/// What one seat's game produced, beyond the decisions it contributed to the batch.
///
/// Taken from the training games themselves rather than a separate evaluation pass: those games are
/// already being played, already sampled from the current policy, and 96 games an update is 9,600
/// per hundred-update report. A dedicated eval would cost compute and see fewer games.
#[derive(Clone)]
struct SeatOutcome {
    faction: String,
    cleared: bool,
    victory_points: i64,
}

/// Faction-level totals accumulated across a reporting window.
#[derive(Clone, Copy, Default)]
struct FactionTally {
    games: usize,
    cleared: usize,
    victory_points: i64,
}

impl FactionTally {
    #[expect(
        clippy::cast_precision_loss,
        reason = "game counts are exact in f64 far beyond any run length"
    )]
    fn clearance(self) -> f64 {
        if self.games == 0 {
            return 0.0;
        }
        self.cleared as f64 / self.games as f64 * 100.0
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "game counts and points are exact in f64"
    )]
    fn mean_points(self) -> f64 {
        if self.games == 0 {
            return 0.0;
        }
        self.victory_points as f64 / self.games as f64
    }

    fn add(&mut self, outcome: &SeatOutcome) {
        self.games += 1;
        self.cleared += usize::from(outcome.cleared);
        self.victory_points += outcome.victory_points;
    }
}

/// Print a window's stage-one clearance and victory points per faction, against the window before.
///
/// The deltas are what make this readable as progress rather than as a snapshot; the first report
/// has nothing to compare against and says so instead of printing a zero, which would read as "no
/// movement" rather than "no baseline".
fn report(
    update: usize,
    window: &BTreeMap<String, FactionTally>,
    previous: Option<&BTreeMap<String, FactionTally>>,
    reported_at: usize,
) {
    let first = reported_at + 1;
    let seats: usize = window.values().map(|tally| tally.games).sum();
    println!(
        "\n  ===== report after update {update} (updates {first}-{update}, {seats} seat-games) ====="
    );
    println!("  faction      games   stage-1 clearance        mean VP");

    let mut table = FactionTally::default();
    let mut previous_table = FactionTally::default();
    for (faction, tally) in window {
        table.games += tally.games;
        table.cleared += tally.cleared;
        table.victory_points += tally.victory_points;
        let before = previous.and_then(|earlier| earlier.get(faction)).copied();
        if let Some(before) = before {
            previous_table.games += before.games;
            previous_table.cleared += before.cleared;
            previous_table.victory_points += before.victory_points;
        }
        print_row(faction, *tally, before);
    }
    println!("  {:-<58}", "");
    print_row(
        "table",
        table,
        (previous_table.games > 0).then_some(previous_table),
    );
}

fn print_row(name: &str, tally: FactionTally, previous: Option<FactionTally>) {
    let clearance = tally.clearance();
    let points = tally.mean_points();
    match previous {
        Some(before) => println!(
            "  {:<10} {:>6}   {:>6.2}%  ({:+.2})   {:>6.3}  ({:+.3})",
            name,
            tally.games,
            clearance,
            clearance - before.clearance(),
            points,
            points - before.mean_points(),
        ),
        None => println!(
            "  {:<10} {:>6}   {:>6.2}%      (--)   {:>6.3}      (--)",
            name, tally.games, clearance, points
        ),
    }
}

/// Write a checkpoint and verify it loads back to the weights that were trained.
///
/// A multi-day run with no resume (M10-035 is not built) would otherwise keep every update's work
/// in one process's memory. Publishing at each report bounds what a crash costs to one window.
fn publish(
    actor: &ti4_mlp::Actor,
    destination: &std::path::Path,
    slots_text: &str,
    critic_mode: ti4_mlp::bundle::CriticMode,
    provenance: &ti4_mlp::bundle::Provenance,
    expected: &[u32],
) {
    let cpu = actor.inference_copy().to_device(ti4_tensor::Device::Cpu);
    let bundle = ti4_mlp::bundle::write(destination, &cpu, slots_text, critic_mode, provenance)
        .unwrap_or_else(|error| refuse(&format!("writing the checkpoint: {error}")));
    let reloaded = ti4_mlp::bundle::read(&bundle.directory)
        .unwrap_or_else(|error| refuse(&format!("the checkpoint does not load: {error}")));
    let fingerprint = ti4_mlp::ppo::parameter_fingerprint(&reloaded.actor, reloaded.critic_mode)
        .unwrap_or_else(|error| refuse(&format!("fingerprinting the reload: {error}")));
    if fingerprint != expected {
        refuse("the reloaded checkpoint does not match the weights that were trained");
    }
    println!(
        "  checkpoint  {} (reloaded, identical)",
        bundle.directory.display()
    );
}

/// How often to report, allowing the cadence to loosen as a run gets longer.
///
/// Written `50:500,500` — every 50 updates until update 500, every 500 after that. Early windows
/// are where a run either starts moving or does not, and that is worth watching closely; ten hours
/// later the same cadence would be 700 reports nobody reads.
///
/// Each report resets the window, so a delta always compares consecutive windows. When the cadence
/// changes, the first long window is compared against the last short one — rates and means stay
/// comparable, the sample sizes do not, which is why every report prints its own span and seat-game
/// count rather than leaving the reader to assume they match.
struct Cadence {
    /// `(every, until)`, in order. The last segment's `until` is open.
    segments: Vec<(usize, Option<usize>)>,
}

impl Cadence {
    fn parse(text: &str) -> Result<Self, String> {
        let mut segments = Vec::new();
        for (position, piece) in text.split(',').enumerate() {
            let piece = piece.trim();
            let (every, until) = match piece.split_once(':') {
                Some((every, until)) => (
                    every.trim(),
                    Some(until.trim().parse::<usize>().map_err(|_| {
                        format!("segment {position} of --report-every: '{until}' is not a number")
                    })?),
                ),
                None => (piece, None),
            };
            let every: usize = every.parse().map_err(|_| {
                format!("segment {position} of --report-every: '{every}' is not a number")
            })?;
            if every == 0 {
                return Err("--report-every cannot report every 0 updates".to_owned());
            }
            segments.push((every, until));
        }
        if segments.is_empty() {
            return Err("--report-every is empty".to_owned());
        }
        // A bounded segment after an unbounded one can never be reached.
        if let Some(position) = segments
            .iter()
            .position(|(_, until)| until.is_none())
            .filter(|position| *position + 1 < segments.len())
        {
            return Err(format!(
                "--report-every segment {position} is unbounded, so the segments after it are dead"
            ));
        }
        Ok(Self { segments })
    }

    /// The interval in force at `done`, and whether a report falls on it.
    fn interval(&self, done: usize) -> usize {
        self.segments
            .iter()
            .find(|(_, until)| until.is_none_or(|until| done <= until))
            .or_else(|| self.segments.last())
            .map_or(1, |(every, _)| *every)
    }

    fn due(&self, done: usize) -> bool {
        done.is_multiple_of(self.interval(done))
    }
}

#[cfg(test)]
mod cadence_tests {
    use super::Cadence;

    #[test]
    fn a_loosening_cadence_reports_where_it_says_it_will() {
        let cadence = Cadence::parse("50:500,500").expect("a valid cadence");
        let due: Vec<usize> = (1..=2_000).filter(|done| cadence.due(*done)).collect();
        let mut expected: Vec<usize> = (1..=10).map(|n| n * 50).collect();
        expected.extend([1_000, 1_500, 2_000]);
        assert_eq!(due, expected);
    }

    #[test]
    fn a_bare_interval_still_means_every_n() {
        let cadence = Cadence::parse("100").expect("a valid cadence");
        assert_eq!(
            (1..=350)
                .filter(|done| cadence.due(*done))
                .collect::<Vec<_>>(),
            vec![100, 200, 300]
        );
    }

    #[test]
    fn a_cadence_that_could_never_fire_is_refused() {
        // Zero would divide by zero; a segment after an unbounded one is unreachable. Both are
        // operator typos that would otherwise show up only as silence hours into a run.
        assert!(Cadence::parse("0").is_err());
        assert!(Cadence::parse("500,50:100").is_err());
        assert!(Cadence::parse("50:x,500").is_err());
    }
}

fn main() {
    let updates: usize = argument("--updates")
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let optimizer_device = match argument("--device").as_deref() {
        None | Some("cpu") => ti4_tensor::OptimizerDevice::Cpu,
        Some("cuda") => ti4_tensor::OptimizerDevice::Cuda,
        Some(other) => refuse(&format!("--device {other}: expected cpu or cuda")),
    };
    let device = optimizer_device
        .resolve()
        .unwrap_or_else(|error| refuse(&format!("--device cuda: {error}")));

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();

    let bundle_path = argument("--bundle").unwrap_or_else(|| {
        ti4_mlp::bundle::latest_complete(std::path::Path::new("out/checkpoints/mlp-critic"))
            .unwrap_or_else(|error| refuse(&format!("scanning for a bundle: {error}")))
            .map_or_else(
                || refuse("no complete bundle under out/checkpoints/mlp-critic"),
                |path| path.display().to_string(),
            )
    });
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let mut actor = loaded.actor;
    let critic_mode = loaded.critic_mode;

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

    let out = argument("--out").unwrap_or_else(|| "out/checkpoints/mlp-ppo".to_owned());
    let cadence = Cadence::parse(&argument("--report-every").unwrap_or_else(|| "100".to_owned()))
        .unwrap_or_else(|error| refuse(&error));
    // The entropy schedule. Coefficients are scaled from 1.0 at the first update down to this
    // multiplier at the last, linearly.
    //
    // A constant bonus is right early, when the policy does not yet know which move is best, and
    // is pure cost once it does: paying to keep probability mass off the chosen move puts a floor
    // under the error rate, and an opening needs about four consecutive correct decisions. The
    // champion trained at a constant bonus ranks 3.5 points better than it samples, which is that
    // floor measured.
    //
    // Defaults to 1.0, which is no schedule at all.
    let entropy_final: f64 = argument("--entropy-final").map_or(1.0, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--entropy-final expects a number"))
    });
    let mut settings = Settings::default();
    settings.movement_entropy = argument("--movement-entropy").map_or(settings.entropy, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--movement-entropy expects a number"))
    });
    // Exposed for the temperature sweep, which needs it as a control rather than as a tuning knob.
    // Dividing the logits by `T` before the softmax also divides the gradient with respect to those
    // logits by `T`: `d/ds log softmax(s / T)_a = (1/T) (e_a - p)`. So a temperature change is
    // silently also an effective-learning-rate change, 4x at 0.25 and 0.4x at 2.5, and a sweep that
    // does not hold that fixed cannot say which of the two it measured.
    settings.learning_rate = argument("--learning-rate").map_or(settings.learning_rate, |value| {
        value
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite() && *parsed > 0.0)
            .unwrap_or_else(|| refuse("--learning-rate expects a positive number"))
    });
    // Subtracted from the return-to-go of every activation that did nothing. Zero restores the
    // reward exactly as it was, so a run can be compared against the history.
    let waste_penalty: f64 = argument("--waste-penalty").map_or(0.0, |value| {
        value
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite() && *parsed >= 0.0)
            .unwrap_or_else(|| refuse("--waste-penalty expects a non-negative number"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(SEED_BASE, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base expects an unsigned integer"))
    });
    // Sampling temperature for self-play. One knob for both acting and the recorded behaviour
    // probabilities -- `MlpBot` has a single `probabilities()` call -- so the PPO importance ratio
    // is computed against the same distribution the action was drawn from. Setting one and not the
    // other would silently corrupt every ratio in the batch.
    let temperature: f64 = argument("--temperature").map_or(1.0, |value| {
        value
            .parse()
            .ok()
            .filter(|t: &f64| t.is_finite() && *t > 0.0)
            .unwrap_or_else(|| refuse("--temperature must be a positive number"))
    });

    let stage = match argument("--stage").as_deref() {
        None | Some("2") => ti4_training::reward::Stage::Two,
        Some("1") => ti4_training::reward::Stage::One,
        Some(other) => refuse(&format!("--stage {other}: expected 1 or 2")),
    };
    let rounds = argument("--rounds").map_or_else(
        || rounds_for(stage),
        |value| {
            value
                .parse()
                .unwrap_or_else(|_| refuse("--rounds expects a positive integer"))
        },
    );
    if rounds == 0 {
        refuse("--rounds 0 plays no game");
    }
    // The Stage-1 potential's coefficients, overridable so an experiment on them is recorded in
    // the command line and the log rather than as an edit to a default nobody can see afterwards.
    //
    // They are pre-registered values: changing one changes what the policy is being paid for, and
    // the run that results is not comparable to runs at the registered settings.
    let weight = |name: &str, registered: f64| -> f64 {
        argument(name).map_or(registered, |value| {
            value
                .parse()
                .unwrap_or_else(|_| refuse(&format!("{name} expects a number")))
        })
    };
    let mut reward = ti4_training::reward::Reward::for_stage(stage);
    reward.expansion_weight = weight("--expansion-weight", reward.expansion_weight);
    reward.unit_weight = weight("--unit-weight", reward.unit_weight);
    reward.conjunctive_weight = weight("--conjunctive-weight", reward.conjunctive_weight);
    reward.clear_bonus = weight("--clear-bonus", reward.clear_bonus);
    // The Stage-2 coefficients that price the opening. Same pre-registration argument as above:
    // a run at other values is a different experiment and the command line has to say so.
    //
    // `r1_bonus` reaches only round-one decisions, by construction -- it is credited at the last
    // round-one slot precisely so a round-three decision is not paid for something it could not
    // affect. `clearance_weight` is the one that reaches every decision, because it is credited at
    // the final slot and every return is a suffix sum. Raising the first alone sharpens round one
    // and leaves the rest of the game indifferent to whether the opening held.
    reward.r1_bonus = weight("--r1-bonus", reward.r1_bonus);
    reward.r1_shaping = weight("--r1-shaping", reward.r1_shaping);
    reward.clearance_weight = weight("--clearance-weight", reward.clearance_weight);
    reward.high_vp_bonus = weight("--high-vp-bonus", reward.high_vp_bonus);
    // `returns` gates both terminal bonuses on `> 0.0`, so a negative value here would be read as
    // "off" and the run would silently not be the experiment its command line describes.
    for (name, value) in [
        ("--clearance-weight", reward.clearance_weight),
        ("--high-vp-bonus", reward.high_vp_bonus),
    ] {
        if value < 0.0 {
            refuse(&format!(
                "{name} {value} is negative; the reward reads any value at or below zero as off"
            ));
        }
    }
    reward
        .validate()
        .unwrap_or_else(|error| refuse(&format!("the reward is not self-consistent: {error}")));

    println!("M10-034 PPO update");
    println!("  bundle      {bundle_path}");
    println!("  seeds       {seed_base}.. ({SEEDS_PER_UPDATE} per update)");
    println!("  critic mode {critic_mode:?}");
    if matches!(stage, ti4_training::reward::Stage::One) {
        println!(
            "  potential   expansion {} | unit {} | conjunctive {} | clear bonus {}",
            reward.expansion_weight,
            reward.unit_weight,
            reward.conjunctive_weight,
            reward.clear_bonus
        );
    }
    if matches!(stage, ti4_training::reward::Stage::Two) {
        println!(
            "  opening     r1 bonus {} | r1 shaping {} | clearance {} | high-VP {}",
            reward.r1_bonus, reward.r1_shaping, reward.clearance_weight, reward.high_vp_bonus
        );
        println!(
            "  points      vp {} | objective {} | secret {}",
            reward.vp_weight, reward.objective_weight, reward.secret_weight
        );
    }
    println!(
        "  reward      {stage:?} ({}) | {rounds} round(s) per game",
        match stage {
            ti4_training::reward::Stage::One => "the opening bar",
            ti4_training::reward::Stage::Two => "victory points",
        }
    );
    println!("  optimiser   {device:?}   (rollouts always CPU, §7.1)");
    println!(
        "  ppo         clip {} | {} epochs | minibatch {} | value {} | entropy {}/{} (movement {}), x{entropy_final} by the end",
        settings.clip_epsilon,
        settings.epochs,
        settings.minibatch,
        settings.value_coefficient,
        settings.entropy,
        settings.strategy_entropy,
        settings.movement_entropy
    );
    // Recorded in the header because run-030's log did not name it, and the temperature is the
    // whole difference between that run and the one before it. A log that cannot say what it was
    // run at cannot be compared against another.
    println!("  sampling    temperature {temperature} (acting and recorded behaviour)");
    println!("  adam        learning rate {}", settings.learning_rate);
    println!("  waste       penalty {waste_penalty} per wasted activation");
    println!(
        "  update      {SEEDS_PER_UPDATE} seeds x {} rotations\n",
        FACTIONS.len()
    );

    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();

    // F-M10-034-D3: **once**, for the whole run. Constructing this inside the loop discarded the
    // moments and the step counter on every update after the first, which turns Adam into a
    // sequence of first steps — and Adam's bias correction is a function of `t`, so the first step
    // is the one that behaves least like Adam. Nothing in the telemetry would have shown it.
    actor = actor.to_device(device);
    let mut optimizer = ti4_mlp::ppo::Adam::new(&mut actor, critic_mode, settings)
        .unwrap_or_else(|error| refuse(&format!("optimiser: {error}")));

    // F-M10-034-D6. Loss telemetry is not evidence that an update happened: a broken optimiser
    // still produces a full, plausible table of losses, and the vacuous tests this milestone kept
    // producing failed in exactly that way. Parameters and Adam state are fingerprinted before and
    // after, and the run refuses if either stayed put.
    let before_parameters = ti4_mlp::ppo::parameter_fingerprint(&actor, critic_mode)
        .unwrap_or_else(|error| refuse(&format!("fingerprinting parameters: {error}")));
    let before_state = optimizer
        .state_fingerprint()
        .unwrap_or_else(|error| refuse(&format!("fingerprinting Adam: {error}")));

    // §4.4: weights are stored on CPU, so a checkpoint from a CUDA run loads on a CPU-only machine.
    // Read once here rather than per publish: it is 1.1 MB of JSON and does not change.
    let slots_text = std::fs::read_to_string(std::path::Path::new(&bundle_path).join("slots.json"))
        .unwrap_or_else(|error| refuse(&format!("reading slots.json: {error}")));

    // Stage-one clearance and victory points accumulate across a reporting window and are compared
    // against the window before it. Per update the numbers are noise -- 96 games, six seats -- but a
    // hundred updates is 9,600 seat-games, which is enough to read a trend from.
    let mut window: BTreeMap<String, FactionTally> = BTreeMap::new();
    let mut previous: Option<BTreeMap<String, FactionTally>> = None;
    let mut reported_at = 0usize;

    for update in 0..updates {
        // ---- rollout, on CPU ----
        //
        // §7.1 pins inference to the CPU, so self-play needs CPU weights. It takes a *copy* rather
        // than moving the training actor: `Adam::new` established the parameters as leaf tensors,
        // and `to_device` on a tensor that requires a gradient returns a non-leaf view of the move.
        // Backward then populates `.grad` on the leaves that were left behind, Adam sees none, and
        // the update silently applies nothing. On CPU the move is a no-op so the bug is invisible;
        // on CUDA it is fatal, which is how it was found.
        //
        // One transfer per update, not one per seat: the per-seat copies are made from this.
        let inference = actor.inference_copy().to_device(ti4_tensor::Device::Cpu);
        let rolled = Instant::now();
        let mut steps: Vec<Step> = Vec::new();
        let mut seated_decisions = 0usize;
        let mut games = 0usize;

        // §6.3's unit of work as a job list. Self-play is embarrassingly parallel — every game is
        // an independent seed — and once the optimizer stopped being launch-bound it was 87% of an
        // update's wall time.
        //
        // Work is split into one chunk per rayon thread rather than one job per thread, because
        // each chunk carries an owned `Actor` copy: `tch::Tensor` is `Send` but not `Sync`, so the
        // actor cannot be borrowed across threads. Per-job copies would allocate 96 actors instead
        // of one per core.
        let base = seed_base + SEEDS_PER_UPDATE * update as u64;
        let jobs: Vec<(u64, usize)> = (base..base + SEEDS_PER_UPDATE)
            .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
            .collect();
        let workers = rayon::current_num_threads().max(1);
        let per_worker = jobs.len().div_ceil(workers);
        let chunks: Vec<(ti4_mlp::Actor, Vec<(u64, usize)>)> = jobs
            .chunks(per_worker)
            .map(|chunk| (inference.inference_copy(), chunk.to_vec()))
            .collect();

        // Collected in chunk order and flattened in job order, so the batch a given update sees does
        // not depend on which worker finished first. Determinism here is not decoration: §6.3's
        // shuffle is seeded, and a batch assembled in scheduling order would make every downstream
        // fingerprint irreproducible.
        let harvest: Vec<Result<Vec<Played>, String>> = chunks
            .into_par_iter()
            .map(|(local, chunk)| {
                // One handle per worker, shared by every game it plays and every seat in them.
                let local = std::rc::Rc::new(local);
                chunk
                    .iter()
                    .map(|(seed, rotation)| {
                        play_one(
                            &local,
                            content,
                            &players,
                            &vocabulary,
                            &pool,
                            &reward,
                            critic_mode,
                            rounds,
                            *seed,
                            *rotation,
                            temperature,
                            waste_penalty,
                        )
                    })
                    .collect()
            })
            .collect();

        let mut wasted_activations = 0usize;
        for chunk in harvest {
            for (game, outcomes, wasted) in chunk.unwrap_or_else(|error| refuse(&error)) {
                games += 1;
                wasted_activations += wasted;
                seated_decisions += game.len();
                steps.extend(game);
                for outcome in &outcomes {
                    window
                        .entry(outcome.faction.clone())
                        .or_default()
                        .add(outcome);
                }
            }
        }
        let rollout_time = rolled.elapsed();
        if steps.is_empty() {
            refuse("self-play recorded no decisions");
        }
        // F-M10-034-D4, the global half. Each seat's returns were already checked against its own
        // decisions; this checks that every seat's decisions reached the batch. A seat lost between
        // the two — by a filter, a drain, a shadowed accumulator — shrinks the batch toward
        // whichever seats survived, and every number downstream stays plausible.
        if steps.len() != seated_decisions {
            refuse(&format!(
                "{seated_decisions} decisions were recorded across seats but {} reached the batch",
                steps.len()
            ));
        }

        // ---- optimise ----
        let batch = Batch::freeze(steps, critic_mode)
            .unwrap_or_else(|error| refuse(&format!("freezing: {error}")));
        if batch.steps().len() != seated_decisions {
            refuse(&format!(
                "freezing changed the decision count from {seated_decisions} to {}",
                batch.steps().len()
            ));
        }
        let optimised = Instant::now();
        // Linear in the update index, so the last update trains at `entropy_final` times the
        // configured bonus. Applied to every head at once: the three coefficients express a
        // considered ratio between heads, and annealing them apart would change that ratio as a
        // side effect of a schedule that is not about it.
        #[expect(clippy::cast_precision_loss, reason = "update counts are exact in f64")]
        let progress = if updates > 1 {
            update as f64 / (updates - 1) as f64
        } else {
            1.0
        };
        let scale = entropy_final.mul_add(progress, 1.0 - progress);
        let settings = Settings {
            entropy: settings.entropy * scale,
            strategy_entropy: settings.strategy_entropy * scale,
            movement_entropy: settings.movement_entropy * scale,
            ..settings
        };

        let stats = ti4_mlp::ppo::update(
            &mut actor,
            &batch,
            critic_mode,
            settings,
            seed_base ^ update as u64,
            &mut optimizer,
        )
        .unwrap_or_else(|error| refuse(&format!("update: {error}")));
        let optimise_time = optimised.elapsed();

        let last = stats.last().unwrap_or_else(|| refuse("no epoch ran"));
        println!(
            "  update {:>3}  games {games}  decisions {:>7}  rollout {:>6.1?}  optimise {:>6.1?}  total {:>6.1?}",
            update,
            batch.len(),
            rollout_time,
            optimise_time,
            rollout_time + optimise_time
        );
        println!(
            "              actor loss {:>9.5}  critic {:>9.5}  |log r| {:>7.5}  clipped {:>6.2}%",
            last.actor_loss,
            last.critic_loss,
            last.kl,
            last.clipped_fraction * 100.0
        );
        let worst = last
            .entropy
            .iter()
            .min_by(|left, right| left.1.total_cmp(right.1));
        if let Some((head, entropy)) = worst {
            println!("              lowest-entropy head {head} at {entropy:.4}");
        }
        // Wasted activations per seat-game. This is the quantity the penalty exists to move, so it
        // is reported every update whether or not a penalty is charged -- a run with the penalty at
        // zero still measures it, which is what makes the comparison possible.
        {
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let per_seat = wasted_activations as f64 / (games * FACTIONS.len()).max(1) as f64;
            println!(
                "              wasted activations {wasted_activations} ({per_seat:.3} per seat-game)"
            );
        }

        // Non-vacuity: an update that moved nothing is not an update, however plausible its
        // telemetry.
        if matches!(critic_mode, CriticMode::BatchMean) && last.critic_loss != 0.0 {
            refuse("batch_mean mode reported a critic loss");
        }

        // ---- the periodic report ----
        let done = update + 1;
        if cadence.due(done) || done == updates {
            report(done, &window, previous.as_ref(), reported_at);
            reported_at = done;
            let fingerprint = ti4_mlp::ppo::parameter_fingerprint(&actor, critic_mode)
                .unwrap_or_else(|error| refuse(&format!("fingerprinting parameters: {error}")));
            publish(
                &actor,
                &std::path::Path::new(&out).join(format!("checkpoint-{}", optimizer.steps())),
                &slots_text,
                critic_mode,
                &ti4_mlp::bundle::Provenance {
                    source: format!("M10-034 PPO, {done} update(s) from {bundle_path}"),
                    git_commit: std::env::var("GIT_COMMIT")
                        .unwrap_or_else(|_| "unrecorded".to_owned()),
                    update: u64::try_from(optimizer.steps()).unwrap_or(0),
                },
                &fingerprint,
            );
            previous = Some(std::mem::take(&mut window));
        }
    }

    let after_parameters = ti4_mlp::ppo::parameter_fingerprint(&actor, critic_mode)
        .unwrap_or_else(|error| refuse(&format!("fingerprinting parameters: {error}")));
    let after_state = optimizer
        .state_fingerprint()
        .unwrap_or_else(|error| refuse(&format!("fingerprinting Adam: {error}")));
    if after_parameters == before_parameters {
        refuse(
            "the run moved no parameter; the losses above describe an update that never applied",
        );
    }
    if after_state == before_state {
        refuse("Adam's moments and step cursor did not advance");
    }
    println!("\n  parameters  moved");
    println!("  adam state  advanced, {} steps", optimizer.steps());

    println!(
        "\n  done. Rollouts are CPU-bound and sequential here; the optimiser honoured --device."
    );
}
