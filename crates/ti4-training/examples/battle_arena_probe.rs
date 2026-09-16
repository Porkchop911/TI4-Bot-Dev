//! ARENA-001 probe: does a pitched battle run outside a full game?
//!
//! The first executable unit of `plans/BATTLE_ARENA_AGENT_HANDOFF_2026-09-16.md`, cut down to the
//! one assumption nothing else can be built on: that a minimal valid engine context plus two
//! fleets in a system drives the real retained `CombatWindow` to an outcome, reproducibly, with a
//! declared combat policy answering casualties and sustains.
//!
//! Deliberately not here yet: the scenario schema, split families, seed panels, upgrades, faction
//! effects, cards, and any dataset. Those follow once this shape is proven. Nothing is written to
//! disk.
//!
//! No second rules implementation: the engine resolves the fight. This only supplies the position
//! and answers the choices.
//!
//! ```text
//! cargo run --release -p ti4-training --example battle_arena_probe
//! ```

use std::collections::BTreeMap;

use rayon::prelude::*;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Table};
use ti4_model::POK;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;

/// A declared, deterministic combat policy.
///
/// Labels are outcomes *under this policy*, never optimal-play odds, so the policy is explicit and
/// versioned rather than implied by option order. Two rules, both by unit facts:
///
/// * sustain whenever a ship can, so a hit is absorbed rather than taken;
/// * assign a casualty to the cheapest hull, damaged ships first.
///
/// Ties break on the unit name, so the same position always answers the same way whatever order
/// the engine happens to offer.
/// Which hull a policy spends when it must lose one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CasualtyRule {
    /// Spend already-damaged hulls first, then the cheapest.
    ///
    /// Intuitive and wrong in the common case: a sustained dreadnought still fights at full
    /// strength, so spending it throws away the best ship on the board to keep a cruiser.
    DamagedFirst,
    /// Keep damaged hulls, spend the cheapest fresh ship.
    CheapestFresh,
}

#[derive(Debug, Clone)]
struct DeclaredCombatPolicy {
    version: &'static str,
    rule: CasualtyRule,
    /// What the engine actually asked this policy, so a run cannot claim a policy mattered when
    /// no sustain or casualty choice ever reached it.
    ///
    /// Owned rather than shared: a policy is cloned into each rayon worker, and an `Rc` cannot
    /// cross that boundary. Per-chunk counts merge into the panel tally like every other figure,
    /// which also keeps the hot path free of a lock.
    asked: BTreeMap<String, usize>,
}

impl DeclaredCombatPolicy {
    fn damaged_first() -> Self {
        Self {
            version: "sustain-always-casualty-damaged-first-v1",
            rule: CasualtyRule::DamagedFirst,
            asked: BTreeMap::new(),
        }
    }

    fn cheapest_fresh() -> Self {
        Self {
            version: "sustain-always-casualty-cheapest-fresh-v1",
            rule: CasualtyRule::CheapestFresh,
            asked: BTreeMap::new(),
        }
    }

    /// Casualty order by base type, so an upgraded ship ranks with its base hull.
    pub(crate) fn hull_rank(unit: &str) -> usize {
        let base = ti4_content::units::unit_type(ContentStore::embedded(), unit, POK)
            .map_or(unit, |kind| kind.base_type());
        ti4_training::battle_arena::hull_rank(base)
    }

    fn unit_of(option: &ChoiceOption) -> String {
        option
            .payload
            .get("unit")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_owned()
    }

    fn damaged(option: &ChoiceOption) -> bool {
        option
            .payload
            .get("damaged")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    }
}

impl Decider for DeclaredCombatPolicy {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        // Record every kind offered, not just the ones this policy has a rule for.
        for kind in choice
            .options
            .iter()
            .map(|option| option.kind.clone())
            .collect::<std::collections::BTreeSet<_>>()
        {
            *self.asked.entry(kind).or_default() += 1;
        }
        let sustain = choice
            .options
            .iter()
            .filter(|option| option.kind == ti4_engine::combat::SUSTAIN_KIND)
            .min_by_key(|option| {
                (
                    Self::hull_rank(&Self::unit_of(option)),
                    Self::unit_of(option),
                )
            });
        if let Some(option) = sustain {
            return Ok(option.clone());
        }
        let casualty = choice
            .options
            .iter()
            .filter(|option| option.kind == ti4_engine::combat::CASUALTY_KIND)
            .min_by_key(|option| {
                let damaged = Self::damaged(option);
                // The only difference between the two policies, so a comparison isolates it.
                let first = match self.rule {
                    CasualtyRule::DamagedFirst => !damaged,
                    CasualtyRule::CheapestFresh => damaged,
                };
                (
                    first,
                    Self::hull_rank(&Self::unit_of(option)),
                    Self::unit_of(option),
                )
            });
        if let Some(option) = casualty {
            return Ok(option.clone());
        }
        // Anything else -- retreat offers, ability prompts -- takes the first option, which the
        // pitched-battle mode is supposed to avoid reaching. The report names them if they occur.
        choice
            .options
            .first()
            .cloned()
            .ok_or_else(|| IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
    }
}

/// The base ship types a fleet is composed from, in a fixed order.
///
/// War suns are deliberately absent from the pilot alphabet: one costs 12 and three dice at 3,
/// so it dominates every matchup it appears in and would swamp a bounded panel. It belongs in a
/// later stratum with its own coverage report.
const SHIP_TYPES: [&str; 7] = [
    "fighter",
    "destroyer",
    "cruiser",
    "carrier",
    "dreadnought",
    "warsun",
    "flagship",
];

/// Most of each ship type a player has in their supply, in `SHIP_TYPES` order. Fighters are
/// bounded by `--max-fighters` and capacity instead.
const SUPPLY: [usize; 7] = [usize::MAX, 8, 8, 4, 6, 2, 1];

/// A faction and upgrade state, which together resolve each base type to a concrete unit.
///
/// Only the factions whose ships or dice actually differ are listed. The other ~25 resolve to the
/// generic ships with no dice shift, so enumerating them would be the same scenario repeated under
/// different names -- which would also corrupt any split that treats them as distinct families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Profile {
    faction: &'static str,
    upgraded: bool,
}

impl Profile {
    /// Faction, why it is here, and whether it shifts dice as well as ships.
    const CATALOGUE: [(&'static str, &'static str); 6] = [
        ("sol", "carrier carries 6; flagship carries 12"),
        ("letnev", "flagship repairs itself every combat round"),
        ("xxcha", "flagship has space cannon 3x5"),
        (
            "hacan",
            "generic ships; flagship is weak -- its trade-good bonus never applies (0 trade goods)",
        ),
        (
            "jolnar",
            "Fragile -1; flagship turns each 9 or 10 into 2 extra hits",
        ),
        (
            "l1z1x",
            "dreadnought carries 2; flagship forces hits onto non-fighters",
        ),
    ];

    fn label(self) -> String {
        format!(
            "{}{}",
            self.faction,
            if self.upgraded { "+upgrades" } else { "" }
        )
    }

    /// The unit this profile fields for a base type: the faction's own version where it has one,
    /// upgraded where the profile says so.
    fn unit_for(self, content: &ContentStore, base: &str) -> String {
        let own = ti4_content::units::faction_unit(content, self.faction, base, POK)
            .map(|unit| unit.id().to_owned());
        let id = own.unwrap_or_else(|| base.to_owned());
        if !self.upgraded {
            return id;
        }
        ti4_content::units::unit_type(content, &id, POK)
            .and_then(|unit| unit.upgrades_to().map(ToOwned::to_owned))
            .unwrap_or(id)
    }
}

/// One side of a matchup: how many of each base type, before profiles resolve them.
type Composition = [usize; SHIP_TYPES.len()];

/// Capacity a composition provides, and what its fighters consume.
///
/// Combat does not enforce capacity -- only production does -- so an illegal fleet would resolve
/// happily and be labelled as if it were a real position. The generator enforces it instead.
fn fighters_fit(content: &ContentStore, profile: Profile, composition: &Composition) -> bool {
    let mut capacity = 0i64;
    for (index, base) in SHIP_TYPES.iter().enumerate() {
        if *base == "fighter" {
            continue;
        }
        let id = profile.unit_for(content, base);
        let provided =
            ti4_content::units::unit_type(content, &id, POK).map_or(0, |unit| unit.capacity());
        capacity += provided * i64::try_from(composition[index]).unwrap_or(0);
    }
    i64::try_from(composition[0]).unwrap_or(i64::MAX) <= capacity
}

/// Every legal composition with at most `max_ships` non-fighter ships.
///
/// Fighters are bounded by the capacity the rest of the fleet provides, which is what keeps this
/// enumerable: 20 fighters needs five carriers, so the large counts only appear where they are
/// legal rather than across the whole product.
fn compositions(
    content: &ContentStore,
    profile: Profile,
    max_ships: usize,
    max_fighters: usize,
) -> Vec<Composition> {
    let mut out = Vec::new();
    let mut current: Composition = [0; SHIP_TYPES.len()];
    walk(
        content,
        profile,
        max_ships,
        max_fighters,
        1,
        0,
        &mut current,
        &mut out,
    );
    out
}

/// Every hull count within supply and the ship cap, then every fighter count that fits.
#[expect(clippy::too_many_arguments, reason = "a private recursive helper")]
fn walk(
    content: &ContentStore,
    profile: Profile,
    max_ships: usize,
    max_fighters: usize,
    index: usize,
    used: usize,
    current: &mut Composition,
    out: &mut Vec<Composition>,
) {
    if index == SHIP_TYPES.len() {
        if used == 0 {
            return; // a fleet of fighters alone has no capacity to carry them
        }
        for fighters in 0..=max_fighters {
            current[0] = fighters;
            if !fighters_fit(content, profile, current) {
                break; // capacity only shrinks as fighters grow
            }
            out.push(*current);
        }
        current[0] = 0;
        return;
    }
    for count in 0..=SUPPLY[index].min(max_ships - used) {
        current[index] = count;
        walk(
            content,
            profile,
            max_ships,
            max_fighters,
            index + 1,
            used + count,
            current,
            out,
        );
    }
    current[index] = 0;
}

/// A matchup, named so a split can be assigned before any dice are rolled.
#[derive(Debug, Clone)]
struct Scenario {
    attacker: Composition,
    defender: Composition,
    attacker_profile: Profile,
    defender_profile: Profile,
}

impl Scenario {
    /// The family a scenario belongs to: profiles plus both compositions, order-independent.
    ///
    /// Side swaps and relabels land in the same family, so a mirror of a training matchup cannot
    /// appear in the test split wearing a different name.
    fn family(&self) -> String {
        let mut sides = [
            format!("{}:{:?}", self.attacker_profile.label(), self.attacker),
            format!("{}:{:?}", self.defender_profile.label(), self.defender),
        ];
        sides.sort();
        sides.join(" vs ")
    }

    fn fleet(&self, content: &ContentStore, attacker: bool) -> Vec<(String, usize)> {
        let (composition, profile) = if attacker {
            (&self.attacker, self.attacker_profile)
        } else {
            (&self.defender, self.defender_profile)
        };
        SHIP_TYPES
            .iter()
            .enumerate()
            .filter(|(index, _)| composition[*index] > 0)
            .map(|(index, base)| (profile.unit_for(content, base), composition[index]))
            .collect()
    }
}

/// One labelled example: a position, and how it turned out over the seed panel.
///
/// The label is a rate rather than a single outcome. Dice decide one fight; the quantity a
/// predictor should learn is the probability, so the repetitions are averaged here and the model
/// is handed the result.
#[derive(Debug, Clone, serde::Serialize)]
struct Row {
    family: String,
    split: &'static str,
    attacker_faction: &'static str,
    attacker_upgraded: bool,
    defender_faction: &'static str,
    defender_upgraded: bool,
    /// Factual fleet counts by base ship type; derived strength is left to the model.
    attacker_units: BTreeMap<String, usize>,
    defender_units: BTreeMap<String, usize>,
    seeds: u64,
    attacker_wins: usize,
    defender_wins: usize,
    mutual: usize,
    /// The labels: outcome rates over the seed panel, summing to 1.
    ///
    /// Three fields rather than one, because a defender win and mutual destruction are different
    /// results and a single attacker rate cannot distinguish them.
    attacker_win_rate: f64,
    defender_win_rate: f64,
    mutual_rate: f64,
    mean_rounds: f64,
    /// Fights that errored, kept separate so a failure is never read as a draw.
    errors: usize,
}

/// Deterministic 80/10/10 split on the family, so a matchup and its mirror share a side.
fn split_of(family: &str) -> &'static str {
    let mut hash: u64 = 1469598103934665603;
    for byte in family.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    match hash % 10 {
        0 => "test",
        1 => "val",
        _ => "train",
    }
}

/// The constructed position every fight starts from, built once per worker.
///
/// `fixtures::game` runs the full `start_game` path and `plain_systems` walks the whole galaxy to
/// take one system. Paying that per fight is serial work that threads only duplicate, so it is paid
/// once and each fight clones it.
struct Base {
    state: GameState,
    system: SystemId,
    attacker: PlayerId,
    defender: PlayerId,
}

impl Base {
    fn new() -> Self {
        Self {
            state: ti4_engine::fixtures::game(&["a", "b"]),
            system: SystemId::new(ti4_engine::fixtures::plain_systems(1)[0].clone()),
            attacker: PlayerId::new("a"),
            defender: PlayerId::new("b"),
        }
    }
}

/// A scenario with both fleets already resolved to concrete unit ids.
///
/// Resolution walks the content store per base type, so doing it once per scenario rather than
/// once per dice repetition is most of the cost of a panel.
#[derive(Debug, Clone)]
struct Resolved {
    attacker: Vec<(String, usize)>,
    defender: Vec<(String, usize)>,
    attacker_faction: &'static str,
    defender_faction: &'static str,
}

impl Resolved {
    fn of(content: &ContentStore, scenario: &Scenario) -> Self {
        Self {
            attacker: scenario.fleet(content, true),
            defender: scenario.fleet(content, false),
            attacker_faction: scenario.attacker_profile.faction,
            defender_faction: scenario.defender_profile.faction,
        }
    }
}

/// What one chunk of scenarios produced, merged in chunk order so the panel is deterministic.
#[derive(Default)]
struct Tally {
    rows: Vec<Row>,
    /// Choice kinds the declared policy was actually asked, merged from every worker.
    asked: BTreeMap<String, usize>,
    wins: BTreeMap<String, usize>,
    rounds: BTreeMap<u32, usize>,
    failures: BTreeMap<String, usize>,
    contaminated: usize,
    played: usize,
}

impl Tally {
    fn merge(&mut self, other: &Self) {
        self.rows.extend(other.rows.iter().cloned());
        for (key, count) in &other.asked {
            *self.asked.entry(key.clone()).or_default() += count;
        }
        for (key, count) in &other.wins {
            *self.wins.entry(key.clone()).or_default() += count;
        }
        for (key, count) in &other.rounds {
            *self.rounds.entry(*key).or_default() += count;
        }
        for (key, count) in &other.failures {
            *self.failures.entry(key.clone()).or_default() += count;
        }
        self.contaminated += other.contaminated;
        self.played += other.played;
    }
}

/// Fleet counts by concrete unit id, for the factual half of a row.
fn count_units(fleet: &[(String, usize)]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (kind, count) in fleet {
        *out.entry(kind.clone()).or_insert(0) += count;
    }
    out
}

/// The lean simulator lives in `ti4_training::battle_arena`; this adapts the probe's scenarios.
mod lean {
    use super::Resolved;
    use ti4_content::ContentStore;
    pub use ti4_training::battle_arena::{Side, fight};

    /// Both sides of a resolved scenario, starting undamaged.
    pub fn sides(content: &ContentStore, resolved: &Resolved, effects: bool) -> (Side, Side) {
        (
            Side::of(
                content,
                &resolved.attacker,
                &[],
                resolved.attacker_faction,
                effects,
            ),
            Side::of(
                content,
                &resolved.defender,
                &[],
                resolved.defender_faction,
                effects,
            ),
        )
    }
}

/// Ships that survived, by type and damage state.
type Survivors = BTreeMap<(String, bool), usize>;

/// What one fight produced. A failure is a failure, never a draw.
#[derive(Debug, PartialEq, Eq)]
struct Fight {
    winner: Option<String>,
    rounds: u32,
    /// The fleets as the engine saw them at the opening bell, by type and damage.
    attacker_start: Survivors,
    defender_start: Survivors,
    attacker_left: Survivors,
    defender_left: Survivors,
    /// Ships in the system belonging to nobody in this fight: contamination, if any.
    intruders: usize,
    error: Option<String>,
}

fn survivors(
    state: &ti4_model::state::GameState,
    system: &SystemId,
    owner: &PlayerId,
) -> Survivors {
    let mut counts: Survivors = BTreeMap::new();
    for unit in state
        .system_state(system)
        .units
        .iter()
        .filter(|unit| &unit.owner == owner)
    {
        *counts
            .entry((unit.type_id.to_string(), unit.sustained_damage))
            .or_default() += 1;
    }
    counts
}

/// One pitched battle: place the fleets, drive the retained window, report what is left.
fn fight(
    content: &'static ContentStore,
    base: &Base,
    resolved: &Resolved,
    dice_seed: u64,
    policy: &mut DeclaredCombatPolicy,
) -> Fight {
    let attacker = base.attacker.clone();
    let defender = base.defender.clone();
    // Cloned before a single ship is placed, so no fleet can carry into the next fight.
    let mut state = base.state.clone();
    let system = base.system.clone();
    // The seat's faction, not just the unit id: abilities are the second channel through which a
    // faction changes a fight (Sardakk +1, Jol-Nar -1), and placing its ships without its faction
    // would report those dice as ordinary.
    if let Some(seat) = state.player_mut(&attacker) {
        seat.faction = ti4_model::id::FactionId::new(resolved.attacker_faction);
    }
    if let Some(seat) = state.player_mut(&defender) {
        seat.faction = ti4_model::id::FactionId::new(resolved.defender_faction);
    }
    for (kind, count) in &resolved.attacker {
        ti4_engine::fixtures::put(&mut state, &system, kind, &attacker, *count);
    }
    for (kind, count) in &resolved.defender {
        ti4_engine::fixtures::put(&mut state, &system, kind, &defender, *count);
    }
    // LRR 13: the active seat is the attacker. Without this the roles fall back to seating order.
    state.active = Some(attacker.clone());

    // The declared policy answers for both sides. `combat::resolve` owns the retained window,
    // drives it to completion and consumes the scoring pause the synchronous API cannot service,
    // so the engine resolves the fight and this only supplies the position and the answers.
    // The position as it stands before a die is rolled, and anyone else's ships in it.
    let attacker_start = survivors(&state, &system, &attacker);
    let defender_start = survivors(&state, &system, &defender);
    let intruders = state
        .system_state(&system)
        .units
        .iter()
        .filter(|unit| unit.owner != attacker && unit.owner != defender)
        .count();

    // Both seats answer with the same declared policy. Each gets its own copy, and what they
    // were asked is folded back so the accounting survives the fight.
    let mut answers = Table::new();
    let attacker_policy = policy.clone();
    let defender_policy = policy.clone();
    answers.seat(attacker.clone(), Box::new(attacker_policy));
    answers.seat(defender.clone(), Box::new(defender_policy));

    let mut dice = ti4_engine::dice::Dice::new();
    // Only the dice stream varies between repetitions of one scenario.
    let mut rng = ti4_engine::rng::GameRng::new(dice_seed);
    let resolved = ti4_engine::combat::resolve(
        &mut state,
        content,
        POK,
        &mut answers,
        &mut dice,
        &mut rng,
        &system,
    );

    match resolved {
        Ok(outcome) => Fight {
            winner: outcome.winner.as_ref().map(ToString::to_string),
            rounds: outcome.rounds,
            attacker_start,
            defender_start,
            attacker_left: survivors(&state, &system, &attacker),
            defender_left: survivors(&state, &system, &defender),
            intruders,
            error: None,
        },
        // An unresolved fight or an illegal choice is a typed failure, never a draw.
        Err(failure) => Fight {
            winner: None,
            rounds: 0,
            attacker_start,
            defender_start,
            attacker_left: survivors(&state, &system, &attacker),
            defender_left: survivors(&state, &system, &defender),
            intruders,
            error: Some(failure.to_string()),
        },
    }
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + 1))
        .cloned()
}

fn main() {
    let content = ContentStore::embedded();
    let max_ships: usize = argument("--max-ships").map_or(6, |value| {
        value.parse().expect("--max-ships expects a number")
    });
    let seeds: u64 =
        argument("--seeds").map_or(32, |value| value.parse().expect("--seeds expects a number"));
    // A refusal, not a clamp: a panel that would exceed this is reported and not run, so the size
    // is a decision rather than an accident.
    let cap: usize = argument("--max-fights").map_or(2_000_000, |value| {
        value.parse().expect("--max-fights expects a number")
    });
    let profiles_wanted: Vec<String> = argument("--profiles")
        .map(|value| value.split(',').map(ToOwned::to_owned).collect())
        .unwrap_or_else(|| vec!["hacan".to_owned()]);
    let upgrades = std::env::args().any(|arg| arg == "--upgrades");

    let profiles: Vec<Profile> = Profile::CATALOGUE
        .iter()
        .filter(|(faction, _)| profiles_wanted.iter().any(|want| want == faction))
        .flat_map(|(faction, _)| {
            let mut out = vec![Profile {
                faction,
                upgraded: false,
            }];
            if upgrades {
                out.push(Profile {
                    faction,
                    upgraded: true,
                });
            }
            out
        })
        .collect();
    assert!(!profiles.is_empty(), "no profile matched --profiles");

    println!("battle arena generator (ARENA-001)");
    println!("  max non-fighter ships per side {max_ships}");
    println!("  dice seeds per scenario        {seeds}");
    println!(
        "  profiles                       {:?}",
        profiles.iter().map(|p| p.label()).collect::<Vec<_>>()
    );
    println!();
    println!("  profile alphabet:");
    for (faction, why) in Profile::CATALOGUE {
        println!("    {faction:<9} {why}");
    }
    println!();

    // Compositions are profile-dependent, because capacity is: a sol carrier carries 6, so it
    // legalises fighters an ordinary carrier could not.
    let max_fighters: usize = argument("--max-fighters").map_or(16, |value| {
        value.parse().expect("--max-fighters expects a number")
    });
    let sides: Vec<Vec<Composition>> = profiles
        .iter()
        .map(|profile| compositions(content, *profile, max_ships, max_fighters))
        .collect();
    let fleets: usize = sides.iter().map(Vec::len).sum();
    let planned = fleets * fleets;
    println!(
        "  fleets      {fleets}  (max {max_ships} ships, {max_fighters} fighters, supply limits)"
    );
    if planned.saturating_mul(seeds as usize) > cap {
        println!(
            "  REFUSED before building: {planned} scenarios x {seeds} seeds exceeds --max-fights {cap}."
        );
        return;
    }
    let mut scenarios: Vec<Scenario> = Vec::with_capacity(planned);
    for (attacker_index, attacker_profile) in profiles.iter().enumerate() {
        let attacker_side = &sides[attacker_index];
        for (defender_index, defender_profile) in profiles.iter().enumerate() {
            let defender_side = &sides[defender_index];
            for attacker in attacker_side {
                for defender in defender_side {
                    // Both role assignments are played. The attacker is the active seat
                    // (LRR 13) and fires first, so a matchup and its mirror are different
                    // scenarios. Collapsing them handed the attacker the lexicographically
                    // smaller fleet in every pair and produced a panel that was 75% defender
                    // wins -- an artefact of the enumeration, not a fact about fleets.
                    // `family()` still groups the pair, so both stay in one split.
                    scenarios.push(Scenario {
                        attacker: *attacker,
                        defender: *defender,
                        attacker_profile: *attacker_profile,
                        defender_profile: *defender_profile,
                    });
                }
            }
        }
    }

    let families: std::collections::BTreeSet<String> =
        scenarios.iter().map(Scenario::family).collect();
    let fights = scenarios.len() * seeds as usize;
    println!("  scenarios   {}", scenarios.len());
    println!(
        "  families    {}  (a matchup and its mirror share one family, so splits cannot leak)",
        families.len()
    );
    println!("  fights      {fights}");
    if fights > cap {
        println!();
        println!("  REFUSED: {fights} fights exceeds --max-fights {cap}.");
        println!("  Lower --max-ships or --seeds, or raise the cap deliberately.");
        return;
    }

    // Where labelled rows go. Absent, the panel only reports -- the probe's original behaviour.
    let emit_path = argument("--emit");
    let emit = emit_path.is_some();

    let policy = match argument("--policy").as_deref() {
        Some("damaged-first") => DeclaredCombatPolicy::damaged_first(),
        Some("cheapest-fresh") | None => DeclaredCombatPolicy::cheapest_fresh(),
        Some(other) => panic!("--policy expects damaged-first or cheapest-fresh, got {other}"),
    };
    println!("  policy      {}", policy.version);
    println!();

    let use_lean = std::env::args().any(|arg| arg == "--lean");
    let lean_effects = !std::env::args().any(|arg| arg == "--no-effects");
    println!("  simulator   {}", if use_lean { "lean" } else { "engine" });

    if let Some(sample) = argument("--compare").map(|v| v.parse::<usize>().expect("--compare N")) {
        let reps: u64 = argument("--compare-reps").map_or(2000, |v| v.parse().expect("reps"));
        let step = (scenarios.len() / sample.max(1)).max(1);
        let picked: Vec<&Scenario> = scenarios.iter().step_by(step).take(sample).collect();
        let started = std::time::Instant::now();
        let rows: Vec<(String, [f64; 3], [f64; 3])> = picked
            .par_iter()
            .map_init(
                || (Base::new(), policy.clone()),
                |(base, policy), scenario| {
                    let resolved = Resolved::of(content, scenario);
                    // Effects off: the engine models none of them, so this isolates the shared rules.
                    let (a, d) = lean::sides(content, &resolved, false);
                    let mut engine = [0f64; 3];
                    let mut quick = [0f64; 3];
                    let slot = |w: Option<&str>| match w {
                        Some("a") => 0,
                        Some("b") => 1,
                        _ => 2,
                    };
                    for seed in 0..reps {
                        let e = fight(content, base, &resolved, seed, policy);
                        engine[slot(e.winner.as_deref())] += 1.0;
                        let (w, _) = lean::fight(&a, &d, seed.wrapping_add(1 << 40));
                        quick[slot(w)] += 1.0;
                    }
                    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
                    let n = reps as f64;
                    (
                        scenario.family(),
                        engine.map(|x| x / n),
                        quick.map(|x| x / n),
                    )
                },
            )
            .collect();
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let n = reps as f64;
        let mut worst: Vec<(f64, &(String, [f64; 3], [f64; 3]))> = rows
            .iter()
            .map(|row| {
                let p = (row.1[0] + row.2[0]) / 2.0;
                let se = (p * (1.0 - p) * 2.0 / n).sqrt().max(1e-9);
                ((row.1[0] - row.2[0]) / se, row)
            })
            .collect();
        worst.sort_by(|x, y| y.0.abs().total_cmp(&x.0.abs()));
        let beyond = worst.iter().filter(|(z, _)| z.abs() > 3.0).count();
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let mean_gap =
            rows.iter().map(|r| (r.1[0] - r.2[0]).abs()).sum::<f64>() / rows.len() as f64;
        println!(
            "  compare     {} scenarios x {reps} reps, {:.1?}",
            rows.len(),
            started.elapsed()
        );
        println!("  mean |attacker rate gap| {mean_gap:.4}");
        println!(
            "  |z| > 3                  {beyond} of {}  (expect ~{:.1} by chance)",
            rows.len(),
            rows.len() as f64 * 0.0027
        );
        for (z, row) in worst.iter().take(8) {
            println!(
                "    z {z:+6.2}  engine a/d/m {:.3}/{:.3}/{:.3}  lean {:.3}/{:.3}/{:.3}  {}",
                row.1[0], row.1[1], row.1[2], row.2[0], row.2[1], row.2[2], row.0
            );
        }
        return;
    }

    let started = std::time::Instant::now();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = scenarios.len().div_ceil(workers).max(1);
    println!("  workers     {workers}");
    println!();

    // One chunk per worker; chunks are merged in order, so the panel does not depend on which
    // thread finished first.
    let chunks: Vec<Tally> = scenarios
        .par_chunks(per_worker)
        .map(|chunk| {
            let mut tally = Tally::default();
            // One policy per worker: its accounting is per-chunk and merged below.
            let mut policy = policy.clone();
            // One constructed game per worker, cloned per fight.
            let base = Base::new();
            for scenario in chunk {
                let resolved = Resolved::of(content, scenario);
                let lean_sides = use_lean.then(|| lean::sides(content, &resolved, lean_effects));
                // Per-scenario accumulation, so the row can carry a rate over the seed panel.
                let (mut a_wins, mut d_wins, mut mutual, mut errors, mut round_total) =
                    (0usize, 0usize, 0usize, 0usize, 0u64);
                for seed in 0..seeds {
                    let result = match &lean_sides {
                        Some((a, d)) => {
                            let (winner, rounds) = lean::fight(a, d, seed);
                            Fight {
                                winner: winner.map(ToOwned::to_owned),
                                rounds,
                                attacker_start: Survivors::new(),
                                defender_start: Survivors::new(),
                                attacker_left: Survivors::new(),
                                defender_left: Survivors::new(),
                                intruders: 0,
                                error: None,
                            }
                        }
                        None => fight(content, &base, &resolved, seed, &mut policy),
                    };
                    tally.played += 1;
                    if result.intruders > 0 {
                        tally.contaminated += 1;
                    }
                    if let Some(error) = &result.error {
                        // A failure ledger, keyed by message: never folded into a draw.
                        *tally.failures.entry(error.clone()).or_default() += 1;
                        errors += 1;
                        continue;
                    }
                    *tally.rounds.entry(result.rounds).or_default() += 1;
                    round_total += u64::from(result.rounds);
                    let outcome = match result.winner.as_deref() {
                        Some("a") => "attacker",
                        Some("b") => "defender",
                        _ => "mutual",
                    };
                    match outcome {
                        "attacker" => a_wins += 1,
                        "defender" => d_wins += 1,
                        _ => mutual += 1,
                    }
                    *tally.wins.entry(outcome.to_owned()).or_default() += 1;
                }
                if emit {
                    let decided = a_wins + d_wins + mutual;
                    let family = scenario.family();
                    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
                    let (a_rate, d_rate, m_rate, mean_rounds) = if decided == 0 {
                        (f64::NAN, f64::NAN, f64::NAN, f64::NAN)
                    } else {
                        (
                            a_wins as f64 / decided as f64,
                            d_wins as f64 / decided as f64,
                            mutual as f64 / decided as f64,
                            round_total as f64 / decided as f64,
                        )
                    };
                    tally.rows.push(Row {
                        split: split_of(&family),
                        family,
                        attacker_faction: scenario.attacker_profile.faction,
                        attacker_upgraded: scenario.attacker_profile.upgraded,
                        defender_faction: scenario.defender_profile.faction,
                        defender_upgraded: scenario.defender_profile.upgraded,
                        attacker_units: count_units(&resolved.attacker),
                        defender_units: count_units(&resolved.defender),
                        seeds,
                        attacker_wins: a_wins,
                        defender_wins: d_wins,
                        mutual,
                        attacker_win_rate: a_rate,
                        defender_win_rate: d_rate,
                        mutual_rate: m_rate,
                        mean_rounds,
                        errors,
                    });
                }
            }
            tally.asked = policy.asked.clone();
            tally
        })
        .collect();

    let mut total = Tally::default();
    for chunk in &chunks {
        total.merge(chunk);
    }
    let (wins, round_counts, failures, contaminated, played) = (
        total.wins,
        total.rounds,
        total.failures,
        total.contaminated,
        total.played,
    );

    let elapsed = started.elapsed();
    println!("  fights played          {played}");
    println!("  contaminated positions {contaminated}");
    println!("  outcomes               {wins:?}");
    println!("  choice kinds answered  {:?}", total.asked);
    println!("  rounds                 {round_counts:?}");
    if failures.is_empty() {
        println!("  failures               none");
    } else {
        println!("  failures               {failures:?}");
    }
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let rate = played as f64 / elapsed.as_secs_f64();
    println!("  measured in {elapsed:.2?}  ({rate:.0} fights/s)");

    if let Some(path) = emit_path {
        use std::io::Write as _;
        let rows = total.rows;
        let mut file =
            std::io::BufWriter::new(std::fs::File::create(&path).expect("--emit path is writable"));
        for row in &rows {
            serde_json::to_writer(&mut file, row).expect("row serialises");
            file.write_all(b"\n").expect("row is written");
        }
        file.flush().expect("rows are flushed");

        // The manifest: labels are conditional on the casualty policy and the seed panel, and a
        // corpus that does not say so invites being read as game value.
        let manifest = serde_json::json!({
            "rows": rows.len(),
            "seeds_per_scenario": seeds,
            "casualty_policy": policy.version,
            "max_ships": max_ships,
            "profiles": profiles.iter().map(|p| p.label()).collect::<Vec<_>>(),
            "upgrades": upgrades,
            "label": "attacker_win_rate / defender_win_rate / mutual_rate over seeds, summing to 1, under the named casualty policy",
            "stratify": "34% of cap-3 rows are foregone conclusions (rate 0 or 1); evaluate stratified by contestedness or the easy rows dominate the score",
            "split": "deterministic FNV-1a over family(); mirrors share a split",
            "caveat": "scenarios are enumerated uniformly and do not match play frequencies",
        });
        let manifest_path = format!("{path}.manifest.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest).expect("manifest serialises"),
        )
        .expect("manifest is written");

        let mut per_split: BTreeMap<&str, usize> = BTreeMap::new();
        for row in &rows {
            *per_split.entry(row.split).or_default() += 1;
        }
        println!();
        println!("  rows written           {}  -> {path}", rows.len());
        println!("  split                  {per_split:?}");
        println!("  manifest               {manifest_path}");
    }
}
