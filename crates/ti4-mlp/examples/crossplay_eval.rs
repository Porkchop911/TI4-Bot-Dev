//! Evaluate a candidate against **frozen** opponents, one seat at a time.
//!
//! # Why self-play cannot be the metric
//!
//! Stage 1's bar is absolute: each seat independently clears it or does not, and six seats can all
//! clear or all fail. Victory points are not like that. If six copies of one policy play each other,
//! exactly one of them wins every game, and the mean victory points per seat is pinned by how the
//! points are distributed rather than by how well anyone played. A symmetric self-play VP number
//! cannot distinguish a brilliant policy from a terrible one, because it measures the table against
//! itself.
//!
//! So the candidate occupies **one** seat and the other five are a frozen benchmark. Everything the
//! candidate scores is then scored against a fixed standard, and improvement is measurable.
//!
//! # Every seat, not one
//!
//! The candidate plays each seat index in turn, so across a run it holds every faction and every
//! position in the initiative order. A candidate evaluated only in seat 0 would be measured on
//! whichever factions the rotation happened to put there.
//!
//! # What is reported
//!
//! - **VP**: the candidate's own victory points.
//! - **margin**: its VP minus the best opponent's. Positive means it beat the table, and it is the
//!   quantity a zero-sum objective actually cares about; raw VP can rise for everyone at once.
//! - **win**: strictly more points than every opponent. Ties are not wins.
//! - **cleared** and **waste**: the stage-1 gates, carried forward so a stage-2 run cannot quietly
//!   trade the opening away. They are constraints, not preferences.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example crossplay_eval -- \
//!   --bundle out/checkpoints/stage2/epoch-8 \
//!   --opponent out/champions/best-94.97_r2-epoch22 \
//!   --seeds 60 --rounds 4
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

fn number<T: std::str::FromStr>(flag: &str, fallback: T) -> T {
    argument(flag).map_or(fallback, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{flag} expects a number")))
    })
}

/// Records a seat's decisions without changing them.
struct Watching {
    inner: Box<dyn Decider>,
    log: Rc<RefCell<Vec<ti4_mlp::positive_corpus::Note>>>,
}

impl Watching {
    fn record(&self, choice: &Choice, chosen: &ChoiceOption) {
        if choice.options.len() < 2 {
            return;
        }
        // The RAW head, not `Actor::resolve_head`. The actor has fourteen heads and `scoring` is
        // not among them, so resolving maps every scoring decision onto the `other` catch-all and
        // the census reads zero offers -- which is what it did read, and the zero meant "no such
        // head" rather than "never offered". The names the waste detector needs (activation,
        // movement, production, landing) are all first-class, so they resolve to themselves and
        // this is strictly more information.
        let head = ti4_policy::learned::decision_head(choice);
        self.log.borrow_mut().push(ti4_mlp::positive_corpus::Note {
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

/// One candidate seat-game.
struct Seated {
    faction: String,
    vp: i64,
    /// The best any opponent managed.
    best_opponent: i64,
    cleared: bool,
    wasteful: bool,
    /// Times this seat was offered the chance to score an objective, and times it declined.
    /// A policy that never scores and a policy that is never offered the chance are different
    /// failures with different fixes, and only these two counts tell them apart.
    scoring_offered: usize,
    scoring_declined: usize,
}

#[derive(Default)]
struct Tally {
    games: usize,
    vp: i64,
    margin: i64,
    wins: usize,
    cleared: usize,
    wasteful: usize,
    scoring_offered: usize,
    scoring_declined: usize,
}

impl Tally {
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    fn report(&self, name: &str) {
        if self.games == 0 {
            return;
        }
        let n = self.games as f64;
        println!(
            "  {name:<10} {:>6}   {:>6.3}   {:>+7.3}   {:>6.1}%   {:>6.2}%   {:>6.2}%   {:>6.2}   {:>7}",
            self.games,
            self.vp as f64 / n,
            self.margin as f64 / n,
            self.wins as f64 / n * 100.0,
            self.cleared as f64 / n * 100.0,
            self.wasteful as f64 / n * 100.0,
            self.scoring_offered as f64 / n,
            if self.scoring_offered == 0 {
                "  (none)".to_owned()
            } else {
                format!(
                    "{:>6.2}%",
                    self.scoring_declined as f64 / self.scoring_offered as f64 * 100.0
                )
            }
        );
    }
}

#[expect(clippy::too_many_arguments, reason = "one call site, all required")]
fn play(
    content: &'static ContentStore,
    factions: &[FactionId],
    pool: &Arc<ti4_sim::MapPool>,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    candidate: &Rc<ti4_mlp::Actor>,
    opponent: &Rc<ti4_mlp::Actor>,
    seed: u64,
    rotation: usize,
    candidate_seat: usize,
    rounds: u32,
    temperature: f64,
) -> Result<Seated, String> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&log);

    // `play_with_decider_factory`, not the audit path. The audit path measures the opening at the
    // end of the *horizon*, so under four rounds "cleared" becomes "ever reached the bar" rather
    // than "the opening cleared" -- the two differ by about sixteen points, and it is a documented
    // finding in plans/M10-034_CLEARANCE_MEASUREMENT_FINDING.md. This path runs round one on its
    // own and measures the opening where the opening ends, and its episodes carry final victory
    // points as well.
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
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
    let me = players[candidate_seat].clone();

    let rollout = ti4_training::rollout::play_with_decider_factory(
        content,
        &players,
        &seated,
        DEFAULT,
        seed,
        ti4_training::rollout::Horizon {
            rounds,
            steps: 400_000,
        },
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        &ti4_training::rollout::OpeningMap::PythonPool {
            pool: Arc::clone(pool),
            tile_seed_offset: TILE_SEED_OFFSET,
        },
        |baselines| {
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let faction = &seated[player];
                let row = ti4_mlp::FactionRow::of(faction.as_str())
                    .map_err(|error| format!("{player}: {error}"))?;
                let baseline = baselines
                    .get(player)
                    .copied()
                    .ok_or_else(|| format!("{player} has no baseline"))?;
                let is_candidate = index == candidate_seat;
                // The stream does not depend on which side occupies the seat, so the same map
                // and the same draws are faced by candidate and benchmark alike.
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                let actor = if is_candidate { candidate } else { opponent };
                // The benchmark is always greedy; only the candidate moves, or the comparison
                // would change two things at once.
                let (decider, _status) =
                    ti4_mlp::bot::MlpBot::sharing(actor, vocabulary.clone(), row, stream)
                        .at_temperature(if is_candidate { temperature } else { 0.001 })
                        .from_setup(baseline)
                        .seat();
                if is_candidate {
                    deciders.insert(
                        player.clone(),
                        Box::new(Watching {
                            inner: decider,
                            log: Rc::clone(&captured),
                        }),
                    );
                } else {
                    deciders.insert(player.clone(), decider);
                }
            }
            Ok(deciders)
        },
    );
    if let Some(error) = &rollout.error {
        return Err(format!("game {seed}/{rotation}: {error}"));
    }

    let mut vp = 0i64;
    let mut best_opponent = i64::MIN;
    let mut faction = String::new();
    let mut cleared = false;
    for seat in &rollout.seats {
        let points = seat.episode.final_progress.victory_points;
        if seat.player == me {
            vp = points;
            faction = seat.faction.to_string();
            cleared = seat.episode.cleared;
        } else {
            best_opponent = best_opponent.max(points);
        }
    }
    if best_opponent == i64::MIN || faction.is_empty() {
        return Err("the candidate was never seated".to_owned());
    }

    let notes = log.borrow();
    let scoring_offered = notes.iter().filter(|note| note.head == "scoring").count();
    let scoring_declined = notes
        .iter()
        .filter(|note| note.head == "scoring" && note.declined)
        .count();

    Ok(Seated {
        faction,
        vp,
        best_opponent,
        cleared,
        wasteful: ti4_mlp::positive_corpus::wasted_activations(&notes) > 0,
        scoring_offered,
        scoring_declined,
    })
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let opponent_path = argument("--opponent").unwrap_or_else(|| {
        refuse("--opponent is required: this measures against a fixed standard")
    });
    let seeds: u64 = number("--seeds", 60);
    let seed_base: u64 = number("--seed-base", 900_000_000);
    let rounds: u32 = number("--rounds", 4);
    // Greedy by default, because argmax is the only scale-invariant reading of a policy. But the
    // candidate's temperature is exposed on purpose: the stage-1 champion scores 0.040 VP at argmax
    // and about 1.8 when the same weights are sampled at 2.5, so "what does this policy score"
    // has no single answer and the flag is how that gap is measured rather than assumed away.
    let temperature: f64 = number("--temperature", 0.001);

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();

    let candidate_bundle = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let opponent_bundle = ti4_mlp::bundle::read(std::path::Path::new(&opponent_path))
        .unwrap_or_else(|error| refuse(&format!("reading {opponent_path}: {error}")));
    let vocabulary = candidate_bundle.vocabulary;

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

    println!("cross-play evaluation");
    println!("  candidate  {bundle_path}");
    println!("  benchmark  {opponent_path} (frozen, five seats)");
    println!("  maps       out/pools/full_np8_12_holdout.json (Validation)");
    println!(
        "  games      {seeds} seeds x {} rotations x {} candidate seats = {}",
        FACTIONS.len(),
        FACTIONS.len(),
        seeds as usize * FACTIONS.len() * FACTIONS.len()
    );
    println!("  rounds     {rounds}");
    println!("  candidate temperature {temperature} (the benchmark is always greedy)");
    println!();

    let started = std::time::Instant::now();
    let jobs: Vec<(u64, usize, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| {
            (0..FACTIONS.len()).flat_map(move |rotation| {
                (0..FACTIONS.len()).map(move |seat| (seed, rotation, seat))
            })
        })
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers).max(1);

    let harvest: Vec<Result<Vec<Seated>, String>> = jobs
        .chunks(per_worker)
        .map(|chunk| {
            (
                candidate_bundle.actor.inference_copy(),
                opponent_bundle.actor.inference_copy(),
                chunk.to_vec(),
            )
        })
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(candidate, opponent, chunk)| {
            let candidate = Rc::new(candidate);
            let opponent = Rc::new(opponent);
            let mut rows = Vec::new();
            for (seed, rotation, seat) in chunk {
                rows.push(play(
                    content,
                    &factions,
                    &pool,
                    &vocabulary,
                    &candidate,
                    &opponent,
                    seed,
                    rotation,
                    seat,
                    rounds,
                    temperature,
                )?);
            }
            Ok(rows)
        })
        .collect();

    let mut by_faction: BTreeMap<String, Tally> = BTreeMap::new();
    let mut all = Tally::default();
    for chunk in harvest {
        for row in chunk.unwrap_or_else(|error| refuse(&error)) {
            let margin = row.vp - row.best_opponent;
            for tally in [by_faction.entry(row.faction.clone()).or_default(), &mut all] {
                tally.games += 1;
                tally.vp += row.vp;
                tally.margin += margin;
                tally.wins += usize::from(margin > 0);
                tally.cleared += usize::from(row.cleared);
                tally.wasteful += usize::from(row.wasteful);
                tally.scoring_offered += row.scoring_offered;
                tally.scoring_declined += row.scoring_declined;
            }
        }
    }

    println!(
        "  faction     games       VP    margin      win   cleared    waste   offers   declined"
    );
    for (faction, tally) in &by_faction {
        tally.report(faction);
    }
    all.report("ALL");
    println!();
    println!("  measured in {:.1?}", started.elapsed());
    println!();
    println!("  margin is VP minus the best opponent's. Its null value is NEGATIVE, not zero: the");
    println!(
        "  candidate is one draw and the best opponent is the maximum of five, so an identical"
    );
    println!("  policy scores below zero. Run candidate == benchmark to get the null for this");
    println!("  horizon and compare against that, never against zero.");
}
