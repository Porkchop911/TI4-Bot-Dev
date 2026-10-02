//! Power map measurement: per faction, per system, immediate defence and one-activation projection,
//! taken at the start of each round's action phase; plus a calibration sample of the Lanchester
//! matchup against the battle arena.
//!
//! Same games as `clearance_eval` / `economy_audit` (held-out pool, fixed seeds, greedy), so rows
//! join to an economy audit of the same seeds on `(seed, rotation, faction)`.
//!
//! Writes
//! - `<out>.systems.tsv`: one row per (game, round, faction, system) where the faction has force,
//!   projection or planets: every layer's descriptors and the system's planet values;
//! - `<out>.calibration.tsv`: sampled matchups -- the Lanchester log ratio after opening fire and
//!   the arena's win rate over `--fights` simulated fights -- for space and for ground.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example power_audit -- \
//!   --bundle <checkpoint> --seeds 200 --diplomacy --rounds 4 --out power
//! ```

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Observed, SeatObservation};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId, SystemId};
use ti4_policy::fleet_strength::{self, Descriptors};
use ti4_policy::power_map::{self, SystemPower};
use ti4_training::battle_arena;

static OPPORTUNITY_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static OPPORTUNITY_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static VERIFY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CHECK_CAPACITY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CAP_CHECKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CAP_VIOLATIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Capacity-consuming units in a space area beyond what the owner's ships and space-dock fighter
/// support allow (LRR 16.3), counting Fighter II as exempt (it is charged to the fleet pool).
fn capacity_violations(seen: &Observed<'_>) -> Vec<String> {
    let content = seen.content();
    let sources = seen.sources();
    let kind = |id: &str| ti4_content::units::unit_type(content, id, sources);
    let mut out = Vec::new();
    let Some(galaxy) = seen.galaxy() else {
        return out;
    };
    for id in galaxy.system_ids() {
        let system = SystemId::new(id);
        let state = seen.system(&system);
        for player in seen.players() {
            let mut capacity = 0_i64;
            let mut fighters = 0_i64;
            let mut carried = 0_i64;
            let mut listing = Vec::new();
            for unit in state.units_of(player) {
                let Some(k) = kind(unit.type_id.as_str()) else {
                    continue;
                };
                listing.push(unit.type_id.to_string());
                capacity += k.capacity();
                if k.is_fighter() {
                    if k.required_technology().is_none() {
                        fighters += k.capacity_cost();
                    }
                } else if k.consumes_capacity() {
                    carried += k.capacity_cost();
                }
            }
            let support: i64 = state
                .planet_units
                .values()
                .flatten()
                .filter(|u| &u.owner == player)
                .filter_map(|u| kind(u.type_id.as_str()))
                .map(|k| k.fighter_support())
                .sum();
            let excess = carried + fighters - fighters.min(support) - capacity;
            if excess > 0 {
                out.push(format!(
                    "{player} in {id}: space {:?}, capacity {capacity}, fighter support {support}, excess {excess}",
                    listing
                ));
            }
        }
    }
    out
}
static V_CHECKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static V_MISMATCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static V_HITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static V_PARTIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static V_FULL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static V_REFRESHED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static V_FROZEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static V_CACHED_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static V_FRESH_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn bump(counter: &std::sync::atomic::AtomicU64, by: u64) {
    counter.fetch_add(by, std::sync::atomic::Ordering::Relaxed);
}

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

fn d(x: &Descriptors) -> String {
    format!(
        "{:.4}\t{:.3}\t{:.0}\t{:.4}\t{:.4}\t{:.0}",
        x.firepower, x.durability, x.fighters, x.barrage, x.cannon, x.capacity
    )
}

const D_COLS: [&str; 6] = ["fp", "hp", "fighters", "barrage", "cannon", "capacity"];

/// Shared by the six deciders of one game.
struct GameLog {
    seed: u64,
    rotation: usize,
    fights: u32,
    snapshotted: BTreeSet<u32>,
    systems: Vec<String>,
    calibration: Vec<String>,
    opportunity: Vec<String>,
    /// Capacity violations seen so far: (seat-in-system text) -> the active seat when first seen.
    violations: BTreeMap<String, Option<PlayerId>>,
    /// Support for the Throne, owner to holder, as last seen at any decision.
    support: BTreeMap<PlayerId, PlayerId>,
}

struct Watch {
    inner: Box<dyn Decider>,
    log: Rc<RefCell<GameLog>>,
    cache: power_map::PowerCache,
}

impl Drop for Watch {
    fn drop(&mut self) {
        let s = self.cache.stats;
        bump(&V_HITS, s.hits);
        bump(&V_PARTIAL, s.partial);
        bump(&V_FULL, s.full);
        bump(&V_REFRESHED, s.systems_refreshed);
        bump(&V_FROZEN, s.frozen);
    }
}

fn planet_value(
    content: &ContentStore,
    sources: ti4_model::content_types::SourceSet,
    id: &str,
) -> i64 {
    ti4_content::galaxy::planet(content, id, sources).map_or(0, |p| p.resources() + p.influence())
}

fn snapshot(log: &mut GameLog, seen: &Observed<'_>) {
    let content = seen.content();
    let sources = seen.sources();
    let round = seen.round();
    let players: Vec<PlayerId> = seen.players().into_iter().cloned().collect();
    let faction_of: BTreeMap<PlayerId, String> = players
        .iter()
        .map(|p| {
            (
                p.clone(),
                seen.seat(p)
                    .map(|s| s.faction.to_string())
                    .unwrap_or_default(),
            )
        })
        .collect();
    let maps: BTreeMap<PlayerId, BTreeMap<SystemId, SystemPower>> = players
        .iter()
        .map(|p| (p.clone(), power_map::power_map(seen, p)))
        .collect();
    for player in &players {
        let clock = std::time::Instant::now();
        let value = power_map::opportunity(seen, player);
        OPPORTUNITY_NANOS.fetch_add(
            u64::try_from(clock.elapsed().as_nanos()).unwrap_or(0),
            std::sync::atomic::Ordering::Relaxed,
        );
        OPPORTUNITY_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        log.opportunity.push(format!(
            "{}	{}	{round}	{}	{value:.4}",
            log.seed, log.rotation, faction_of[player]
        ));
    }
    let Some(galaxy) = seen.galaxy() else { return };
    for id in galaxy.system_ids() {
        let system = SystemId::new(id);
        let state = seen.system(&system);
        let planets: Vec<&str> = ti4_content::galaxy::system(content, id, sources)
            .map(|s| s.planets())
            .unwrap_or_default();
        let total_value: i64 = planets
            .iter()
            .map(|p| planet_value(content, sources, p))
            .sum();
        for player in &players {
            let Some(power) = maps.get(player).and_then(|m| m.get(&system)) else {
                continue;
            };
            let own_value: i64 = state
                .planet_control
                .iter()
                .filter(|(_, owner)| *owner == player)
                .map(|(planet, _)| planet_value(content, sources, planet.as_str()))
                .sum();
            let active = power.space_projection.durability > 0.0
                || power.ground_projection.durability > 0.0
                || power.space_defence.durability > 0.0
                || own_value > 0;
            if !active {
                continue;
            }
            log.systems.push(format!(
                "{}\t{}\t{round}\t{}\t{id}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.4}\t{}\t{}\t{}",
                log.seed,
                log.rotation,
                faction_of[player],
                planets.len(),
                total_value,
                own_value,
                d(&power.space_defence),
                d(&power.ground_defence),
                d(&power.space_projection),
                d(&power.ground_projection),
                i64::from(power.shielded),
                power.bombardment,
                power.arriving_ships,
                i64::from(power.can_activate),
                power.space_projection_units.count(),
            ));
        }

        // Calibration: every attacker with projection here against each defender with ships here.
        for attacker in &players {
            let Some(att) = maps.get(attacker).and_then(|m| m.get(&system)) else {
                continue;
            };
            for defender in players.iter().filter(|p| *p != attacker) {
                let Some(def) = maps.get(defender).and_then(|m| m.get(&system)) else {
                    continue;
                };
                let (fa, fd) = (&faction_of[attacker], &faction_of[defender]);
                if att.space_projection_units.count() > 0 && def.space_defence_units.count() > 0 {
                    let ratio = fleet_strength::matchup(&att.space_projection, &def.space_defence);
                    let win = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let a = battle_arena::Side::of(
                            content,
                            &att.space_projection_units.units,
                            &att.space_projection_units.damaged,
                            fa,
                            true,
                        );
                        let b = battle_arena::Side::of(
                            content,
                            &def.space_defence_units.units,
                            &def.space_defence_units.damaged,
                            fd,
                            true,
                        )
                        .with_guns(content, &def.guns);
                        let wins = (0..log.fights)
                            .filter(|i| {
                                battle_arena::fight_outcome(
                                    &a,
                                    &b,
                                    battle_arena::fnv(&format!(
                                        "{}:{}:{id}:{i}",
                                        log.seed, log.rotation
                                    )),
                                    true,
                                )
                                .winner
                                    == Some("a")
                            })
                            .count();
                        f64::from(u32::try_from(wins).unwrap_or(0)) / f64::from(log.fights)
                    }));
                    if let Ok(win) = win {
                        log.calibration.push(format!(
                            "space\t{}\t{}\t{round}\t{fa}\t{fd}\t{id}\t{ratio:.5}\t{win:.4}\t{}\t{}\t{}",
                            log.seed, log.rotation, d(&att.space_projection), d(&def.space_defence),
                            att.space_projection_units.count()
                        ));
                    }
                }
                // Ground: attacker's projected ground forces against the defender's forces here
                // (summed over the defender's planets), bombardment unless shielded.
                if att.ground_projection_units.count() > 0 && def.ground_defence_units.count() > 0 {
                    let mut own = att.ground_projection;
                    own.cannon = if def.shielded { 0.0 } else { att.bombardment };
                    let enemy = def.ground_defence;
                    let ratio = fleet_strength::matchup(&own, &enemy);
                    let win = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mut a = battle_arena::GroundSide::of(
                            content,
                            &att.ground_projection_units.units,
                            &att.ground_projection_units.damaged,
                            fa,
                        );
                        if !def.shielded {
                            a = a.with_bombardment(
                                content,
                                &att.space_projection_units.units,
                                true,
                                false,
                            );
                        }
                        let b = battle_arena::GroundSide::of(
                            content,
                            &def.ground_defence_units.units,
                            &def.ground_defence_units.damaged,
                            fd,
                        )
                        .with_defense_guns(content, &def.guns);
                        let wins = (0..log.fights)
                            .filter(|i| {
                                battle_arena::ground_fight(
                                    &a,
                                    &b,
                                    battle_arena::fnv(&format!(
                                        "g{}:{}:{id}:{i}",
                                        log.seed, log.rotation
                                    )),
                                )
                                .winner
                                    == Some("a")
                            })
                            .count();
                        f64::from(u32::try_from(wins).unwrap_or(0)) / f64::from(log.fights)
                    }));
                    if let Ok(win) = win {
                        log.calibration.push(format!(
                            "ground\t{}\t{}\t{round}\t{fa}\t{fd}\t{id}\t{ratio:.5}\t{win:.4}\t{}\t{}\t{}",
                            log.seed, log.rotation, d(&own), d(&enemy), att.ground_projection_units.count()
                        ));
                    }
                }
            }
        }
    }
}

impl Decider for Watch {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.log
            .borrow_mut()
            .support
            .clone_from(seen.support_holders());
        if CHECK_CAPACITY.load(std::sync::atomic::Ordering::Relaxed)
            && seen.active_system().is_none()
        {
            bump(&CAP_CHECKS, 1);
            // A violation that is still there after the active seat has changed outlived a turn end,
            // where enforcement runs across the whole table.
            let now: Vec<String> = capacity_violations(seen.observed())
                .into_iter()
                .filter(|_| !choice.prompt.contains("over capacity"))
                .collect();
            let active = seen.active_player().cloned();
            let mut found = Vec::new();
            {
                let mut log = self.log.borrow_mut();
                let keys: Vec<String> = now
                    .iter()
                    .map(|v| v.split(':').next().unwrap_or("").to_owned())
                    .collect();
                log.violations.retain(|key, _| keys.contains(key));
                for (key, text) in keys.iter().zip(&now) {
                    let first = log
                        .violations
                        .entry(key.clone())
                        .or_insert_with(|| active.clone());
                    if first.is_some() && active.is_some() && *first != active {
                        found.push(format!(
                            "{text} (first seen in {:?}'s turn, now {:?}'s)",
                            first, active
                        ));
                    }
                }
            }
            if !found.is_empty() {
                bump(&CAP_VIOLATIONS, 1);
                if CAP_VIOLATIONS.load(std::sync::atomic::Ordering::Relaxed) <= 25 {
                    let log = self.log.borrow();
                    eprintln!(
                        "CAPACITY seed {} rot {} round {} phase {:?} prompt {:?}: {}",
                        log.seed,
                        log.rotation,
                        seen.round(),
                        seen.phase(),
                        choice.prompt,
                        found.join(" | ")
                    );
                }
            }
        }
        if VERIFY.load(std::sync::atomic::Ordering::Relaxed) && seen.active_system().is_none() {
            let clock = std::time::Instant::now();
            let cached = self.cache.summary(seen.observed(), &choice.player);
            bump(
                &V_CACHED_NANOS,
                u64::try_from(clock.elapsed().as_nanos()).unwrap_or(0),
            );
            let clock = std::time::Instant::now();
            let fresh = power_map::summary(seen.observed(), &choice.player);
            bump(
                &V_FRESH_NANOS,
                u64::try_from(clock.elapsed().as_nanos()).unwrap_or(0),
            );
            bump(&V_CHECKS, 1);
            let close = |a: f64, b: f64| (a - b).abs() <= 1e-9;
            if !(close(cached.opportunity, fresh.opportunity)
                && close(cached.best_target, fresh.best_target)
                && close(cached.reach_systems, fresh.reach_systems))
            {
                bump(&V_MISMATCH, 1);
                if V_MISMATCH.load(std::sync::atomic::Ordering::Relaxed) <= 40 {
                    eprintln!(
                        "MISMATCH round {} phase {:?} active {:?} prompt {:?} subtype {:?} cached {:.3} fresh {:.3}",
                        seen.round(),
                        seen.phase(),
                        seen.active_system(),
                        choice.prompt,
                        choice.context.as_ref().map(|c| c.subtype.clone()),
                        cached.opportunity,
                        fresh.opportunity
                    );
                    for line in self
                        .cache
                        .differences(seen.observed(), &choice.player)
                        .iter()
                        .take(4)
                    {
                        eprintln!("    {line}");
                    }
                }
            }
        }
        if seen.phase() == ti4_model::state::Phase::Action {
            let mut log = self.log.borrow_mut();
            if log.snapshotted.insert(seen.round()) {
                snapshot(&mut log, seen.observed());
            }
        }
        self.inner.choose_seeing(choice, seen)
    }

    fn stage_scores(&mut self, scores: Vec<(Option<f64>, Option<f64>)>) {
        self.inner.stage_scores(scores);
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one measurement, as in clearance_eval"
)]
fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let seeds: u64 =
        argument("--seeds").map_or(100, |v| v.parse().unwrap_or_else(|_| refuse("--seeds")));
    let seed_base: u64 = argument("--seed-base").map_or(900_000_000, |v| {
        v.parse().unwrap_or_else(|_| refuse("--seed-base"))
    });
    let fights: u32 =
        argument("--fights").map_or(100, |v| v.parse().unwrap_or_else(|_| refuse("--fights")));
    let out = argument("--out").unwrap_or_else(|| refuse("--out is required"));
    let diplomacy = std::env::args().any(|a| a == "--diplomacy");
    VERIFY.store(
        std::env::args().any(|a| a == "--verify-cache"),
        std::sync::atomic::Ordering::Relaxed,
    );
    CHECK_CAPACITY.store(
        std::env::args().any(|a| a == "--check-capacity"),
        std::sync::atomic::Ordering::Relaxed,
    );
    let rounds: u32 =
        argument("--rounds").map_or(1, |v| v.parse().unwrap_or_else(|_| refuse("--rounds")));
    let temperature = 0.001;

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Validation],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|i| PlayerId::new(format!("seat{i}")))
        .collect();
    let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers);
    let chunks: Vec<(ti4_mlp::Actor, Vec<(u64, usize)>)> = jobs
        .chunks(per_worker)
        .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
        .collect();

    let started = std::time::Instant::now();
    let harvest: Vec<Result<(Vec<String>, Vec<String>, Vec<String>, Vec<String>), String>> = chunks
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let mut systems = Vec::new();
            let mut calibration = Vec::new();
            let mut outcomes = Vec::new();
            let mut opportunity_rows = Vec::new();
            for (seed, rotation) in chunk {
                let seated: BTreeMap<PlayerId, FactionId> = players
                    .iter()
                    .enumerate()
                    .map(|(index, player)| {
                        (
                            player.clone(),
                            ti4_training::rollout::seated_faction(
                                &FACTIONS.map(FactionId::new),
                                seed,
                                rotation,
                                index,
                            ),
                        )
                    })
                    .collect();
                let log = Rc::new(RefCell::new(GameLog {
                    seed,
                    rotation,
                    fights,
                    snapshotted: BTreeSet::new(),
                    systems: Vec::new(),
                    calibration: Vec::new(),
                    support: BTreeMap::new(),
                    opportunity: Vec::new(),
                    violations: BTreeMap::new(),
                }));
                let (rollout, _) =
                    ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
                        content,
                        &players,
                        &seated,
                        DEFAULT,
                        seed,
                        ti4_training::rollout::Horizon {
                            rounds,
                            steps: 10_000,
                        },
                        ti4_engine::opening::DEFAULT_REQUIREMENT,
                        &ti4_training::rollout::OpeningMap::PythonPool {
                            pool: Arc::clone(&pool),
                            tile_seed_offset: TILE_SEED_OFFSET,
                        },
                        ti4_training::rollout::SimulationCapabilities { diplomacy },
                        false,
                        |baselines| {
                            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> =
                                BTreeMap::new();
                            for (index, player) in players.iter().enumerate() {
                                let row = ti4_mlp::FactionRow::of(seated[player].as_str())
                                    .map_err(|e| format!("{player}: {e}"))?;
                                let baseline = baselines
                                    .get(player)
                                    .copied()
                                    .ok_or_else(|| format!("{player} has no setup baseline"))?;
                                let stream = seed
                                    .wrapping_mul(1_000_003)
                                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                                let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
                                    &local,
                                    vocabulary.clone(),
                                    row,
                                    stream,
                                )
                                .at_temperature(temperature)
                                .from_setup(baseline)
                                .seat();
                                deciders.insert(
                                    player.clone(),
                                    Box::new(Watch {
                                        inner: decider,
                                        log: Rc::clone(&log),
                                        cache: power_map::PowerCache::default(),
                                    }),
                                );
                            }
                            Ok(deciders)
                        },
                    );
                if let Some(error) = &rollout.error {
                    return Err(format!("game {seed}/{rotation} failed: {error}"));
                }
                let mut log = log.borrow_mut();
                systems.append(&mut log.systems);
                calibration.append(&mut log.calibration);
                opportunity_rows.append(&mut log.opportunity);
                for seat in &rollout.seats {
                    let held = log.support.values().filter(|h| **h == seat.player).count();
                    outcomes.push(format!(
                        "{seed}\t{rotation}\t{}\t{}\t{held}\t{}",
                        seat.faction,
                        seat.episode.final_progress.victory_points,
                        i64::from(seat.episode.cleared)
                    ));
                }
            }
            Ok((systems, calibration, outcomes, opportunity_rows))
        })
        .collect();

    let layer_cols = |prefix: &str| {
        D_COLS
            .iter()
            .map(|c| format!("{prefix}_{c}"))
            .collect::<Vec<_>>()
            .join("\t")
    };
    let mut systems = format!(
        "seed\trotation\tround\tfaction\tsystem\tplanets\tvalue_total\tvalue_own\t{}\t{}\t{}\t{}\tshielded\tbombardment\tarriving_ships\tcan_activate\tprojection_units\n",
        layer_cols("sdef"),
        layer_cols("gdef"),
        layer_cols("sproj"),
        layer_cols("gproj")
    );
    let mut calibration = format!(
        "kind\tseed\trotation\tround\tattacker\tdefender\tsystem\tlanchester_log_ratio\tarena_win\t{}\t{}\tattacker_units\n",
        layer_cols("att"),
        layer_cols("def")
    );
    let mut opportunity = String::from("seed\trotation\tround\tfaction\topportunity\n");
    let mut outcomes = String::from("seed\trotation\tfaction\tvp\tsupport_held\tcleared\n");
    for chunk in harvest {
        let (s, c, o, q) = chunk.unwrap_or_else(|error| refuse(&error));
        for line in q {
            opportunity.push_str(&line);
            opportunity.push('\n');
        }
        for line in o {
            outcomes.push_str(&line);
            outcomes.push('\n');
        }
        for line in s {
            systems.push_str(&line);
            systems.push('\n');
        }
        for line in c {
            calibration.push_str(&line);
            calibration.push('\n');
        }
    }
    let sp = format!("{out}.systems.tsv");
    let cp = format!("{out}.calibration.tsv");
    std::fs::write(&sp, systems).unwrap_or_else(|e| refuse(&format!("{sp}: {e}")));
    std::fs::write(&cp, calibration).unwrap_or_else(|e| refuse(&format!("{cp}: {e}")));
    let qp = format!("{out}.opportunity.tsv");
    std::fs::write(&qp, opportunity).unwrap_or_else(|e| refuse(&format!("{qp}: {e}")));
    let calls = OPPORTUNITY_CALLS
        .load(std::sync::atomic::Ordering::Relaxed)
        .max(1);
    println!(
        "  opportunity(): {calls} calls, {:.3} ms each (per thread)",
        OPPORTUNITY_NANOS.load(std::sync::atomic::Ordering::Relaxed) as f64 / calls as f64 / 1e6
    );
    if VERIFY.load(std::sync::atomic::Ordering::Relaxed) {
        let load = |c: &std::sync::atomic::AtomicU64| c.load(std::sync::atomic::Ordering::Relaxed);
        let checks = load(&V_CHECKS).max(1);
        println!(
            "  cache verify: {} checks, {} mismatches | hits {} partial {} full {} frozen {} | systems refreshed per partial {:.1} | cached {:.3} ms vs fresh {:.3} ms per decision",
            load(&V_CHECKS),
            load(&V_MISMATCH),
            load(&V_HITS),
            load(&V_PARTIAL),
            load(&V_FULL),
            load(&V_FROZEN),
            load(&V_REFRESHED) as f64 / load(&V_PARTIAL).max(1) as f64,
            load(&V_CACHED_NANOS) as f64 / checks as f64 / 1e6,
            load(&V_FRESH_NANOS) as f64 / checks as f64 / 1e6,
        );
    }
    if CHECK_CAPACITY.load(std::sync::atomic::Ordering::Relaxed) {
        println!(
            "  capacity check: {} decisions between actions, {} with a seat over capacity",
            CAP_CHECKS.load(std::sync::atomic::Ordering::Relaxed),
            CAP_VIOLATIONS.load(std::sync::atomic::Ordering::Relaxed)
        );
    }
    let op = format!("{out}.outcomes.tsv");
    std::fs::write(&op, outcomes).unwrap_or_else(|e| refuse(&format!("{op}: {e}")));
    println!(
        "  wrote {sp} and {cp} in {:.1}s",
        started.elapsed().as_secs_f64()
    );
}
