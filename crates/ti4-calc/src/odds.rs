//! Battle odds from rollouts of [`arena`]: the request and response shapes, the validation of a
//! request, and the rollout loops. `ti4-host` gives them to the views.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{POK, SourceSet};
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;

use crate::arena::{self, GroundSide, Side};

pub const MAX_PLACEMENTS: usize = 256;
pub const DEFAULT_SIMULATIONS: usize = 2000;
pub const MAX_SIMULATIONS: usize = 50_000;

/// Why a request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalcError(pub String);

impl CalcError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CalcError {}

/// Units count input, accepting either `{"unit_id": count}` or `[["unit_id", count]]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum UnitCounts {
    Map(BTreeMap<String, usize>),
    List(Vec<(String, usize)>),
}

impl UnitCounts {
    #[must_use]
    pub fn into_entries(self) -> Vec<(String, usize)> {
        match self {
            Self::Map(map) => map.into_iter().collect(),
            Self::List(list) => list,
        }
    }
}

/// A participant in space combat, specified either by a player ID or custom side parameters.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ParticipantInput {
    Player(PlayerId),
    Side(SideInput),
}

/// Fleet configuration for one participant in space combat.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SideInput {
    /// Player ID whose fleet/guns at the combat system should be included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<PlayerId>,
    /// Faction identifier (e.g. "sol", "letnev", "jolnar"). If omitted and `player` is given, read from state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<String>,
    /// Ships to field as `{"unit_id": count}` or `[["unit_id", count]]`. Added to units from state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<UnitCounts>,
    /// Ships that start damaged as `{"unit_id": count}` or `[["unit_id", count]]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damaged: Option<UnitCounts>,
    /// Units firing space cannon before combat as `{"unit_id": count}` or `[["unit_id", count]]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guns: Option<UnitCounts>,
}

/// Outcome statistics from [`arena`] rollouts. The rates are fractions of the rollouts, the
/// survivors and the fielded units are by unit id.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BattleResponse {
    pub simulations: usize,
    pub attacker_win_rate: f64,
    pub defender_win_rate: f64,
    pub mutual_destruction_rate: f64,
    /// The rollouts that did not end.
    pub unresolved_rate: f64,
    pub average_rounds: f64,
    pub attacker_expected_survivors: BTreeMap<String, f64>,
    pub defender_expected_survivors: BTreeMap<String, f64>,
    pub attacker_fielded: BTreeMap<String, usize>,
    pub defender_fielded: BTreeMap<String, usize>,
}

/// One side of a ground combat on a planet: its ground forces by unit id, and for the defender
/// the units that fire as defense guns before the combat (`space_cannon` of a PDS).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GroundOddsSide {
    /// Faction identifier (e.g. "sol"): its unit upgrades and abilities apply.
    pub faction: String,
    /// Ground forces as `{"unit_id": count}`.
    pub units: BTreeMap<String, usize>,
    /// Of `units`, those that start damaged.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub damaged: BTreeMap<String, usize>,
    /// The defender only: units on the planet that fire as defense guns.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub guns: BTreeMap<String, usize>,
}

/// The two sides of a ground combat, and the ships of the attacker that bombard.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GroundOddsRequest {
    pub attacker: GroundOddsSide,
    pub defender: GroundOddsSide,
    /// Legal bombarding ships that fire Harrow after rounds (never before landing).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub harrow: BTreeMap<String, usize>,
    /// Number of rollouts (defaults to 2000, at most 50000).
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub simulations: Option<usize>,
}

/// Outcome of the rollouts of a ground combat. The rates are fractions of the rollouts and sum
/// to 1: the attacker takes the planet, the defender holds it with ground forces left, or both
/// sides are destroyed (the defender keeps the planet, LRR 49.5d). The survivors and the fielded
/// ground forces are by unit id.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GroundOddsResponse {
    pub simulations: usize,
    pub attacker_win_rate: f64,
    /// Includes the rollouts that did not end ([`GroundOddsResponse::unresolved_rate`]).
    pub defender_win_rate: f64,
    pub mutual_destruction_rate: f64,
    /// The rollouts that did not end: the invasion fails.
    pub unresolved_rate: f64,
    /// Rounds of ground combat, 0 when no round is fought.
    pub average_rounds: f64,
    pub attacker_expected_survivors: BTreeMap<String, f64>,
    pub defender_expected_survivors: BTreeMap<String, f64>,
    pub attacker_fielded: BTreeMap<String, usize>,
    pub defender_fielded: BTreeMap<String, usize>,
    /// What the fielded ground forces of each side roll in a combat round, by unit id.
    pub attacker_rolls: BTreeMap<String, GroundRoll>,
    pub defender_rolls: BTreeMap<String, GroundRoll>,
}

/// The combat roll of a kind of ground force in the rollouts: each unit rolls `dice` dice, and a
/// die hits when its result plus `modifier` is at least `hits_on`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GroundRoll {
    /// The printed combat value of the unit.
    pub hits_on: i64,
    pub dice: u32,
    /// Added to each die: -1 for Fragile, +1 for Unrelenting, else 0.
    pub modifier: i64,
}

/// One side of a space combat: the ships and guns of `participant`, read from `state` at
/// `system` when a player is named, plus the units the request adds. Returns the side and the
/// ships it fields by unit id.
///
/// # Errors
///
/// Returns a [`CalcError`] if the player, the state or a unit id is missing or unknown.
pub fn resolve_side(
    participant: &ParticipantInput,
    state: Option<&GameState>,
    system: Option<&SystemId>,
    galaxy: Option<&Galaxy>,
    content: &ContentStore,
    sources: SourceSet,
) -> Result<(Side, BTreeMap<String, usize>), CalcError> {
    let mut input = match participant {
        ParticipantInput::Player(pid) => SideInput {
            player: Some(pid.clone()),
            ..Default::default()
        },
        ParticipantInput::Side(side) => side.clone(),
    };
    if let Some(pid) = input.player.take() {
        let game_state =
            state.ok_or_else(|| CalcError::new("state is required when player is specified"))?;
        let gathered = gather_side(content, sources, game_state, system, galaxy, &pid)?;
        input = SideInput {
            player: None,
            faction: input.faction.or(gathered.faction),
            units: joined(gathered.units, input.units),
            damaged: joined(gathered.damaged, input.damaged),
            guns: joined(gathered.guns, input.guns),
        };
    }
    build_side(content, sources, &input)
}

/// Both counts as one list; an id in both is summed where the list is read.
fn joined(first: Option<UnitCounts>, second: Option<UnitCounts>) -> Option<UnitCounts> {
    match (first, second) {
        (None, None) => None,
        (first, second) => Some(UnitCounts::List(
            first
                .into_iter()
                .chain(second)
                .flat_map(UnitCounts::into_entries)
                .collect(),
        )),
    }
}

/// What `state` says of the side of `player` at `system`: its faction, its ships there and which
/// of them are damaged, and its space cannon on the planets there and in adjacent systems (those
/// adjacent only with `galaxy`). With no system, the faction alone. The side names no player.
///
/// # Errors
///
/// Returns a [`CalcError`] if the player is not in `state`.
pub fn gather_side(
    content: &ContentStore,
    sources: SourceSet,
    state: &GameState,
    system: Option<&SystemId>,
    galaxy: Option<&Galaxy>,
    player: &PlayerId,
) -> Result<SideInput, CalcError> {
    let seat = state
        .player(player)
        .ok_or_else(|| CalcError::new(format!("player {player} not found in state")))?;
    let mut fleet: BTreeMap<String, usize> = BTreeMap::new();
    let mut damaged: BTreeMap<String, usize> = BTreeMap::new();
    let mut guns: BTreeMap<String, usize> = BTreeMap::new();

    if let Some(sys) = system {
        let board = state.system_state(sys);
        for unit in &board.units {
            if &unit.owner == player
                && let Some(kind) =
                    ti4_content::units::unit_type(content, unit.type_id.as_str(), sources)
                && kind.is_ship()
            {
                *fleet.entry(unit.type_id.to_string()).or_default() += 1;
                if unit.sustained_damage {
                    *damaged.entry(unit.type_id.to_string()).or_default() += 1;
                }
            }
        }
        for unit in board.planet_units.values().flatten() {
            if &unit.owner == player
                && let Some(kind) =
                    ti4_content::units::unit_type(content, unit.type_id.as_str(), sources)
                && kind.has_space_cannon()
            {
                *guns.entry(unit.type_id.to_string()).or_default() += 1;
            }
        }
        if let Some(g) = galaxy {
            for adjacent_id in g.adjacent(sys.as_str()) {
                let adj_sys = SystemId::new(adjacent_id);
                let adj_board = state.system_state(&adj_sys);
                let adj_units = adj_board
                    .planet_units
                    .values()
                    .flatten()
                    .chain(&adj_board.units);
                for unit in adj_units {
                    if &unit.owner == player {
                        let reaches = matches!(
                            unit.type_id.as_str(),
                            "pds2" | "xxcha_mech" | "xxcha_flagship"
                        ) || ti4_content::units::unit_type(
                            content,
                            unit.type_id.as_str(),
                            sources,
                        )
                        .and_then(|k| k.record().text("ability"))
                        .is_some_and(|ability| {
                            let a = ability.to_ascii_lowercase();
                            a.contains("space cannon against ships that are")
                                && a.contains("adjacent")
                        });
                        if reaches {
                            *guns.entry(unit.type_id.to_string()).or_default() += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(SideInput {
        player: None,
        faction: Some(seat.faction.as_str().to_owned()),
        units: Some(UnitCounts::Map(fleet)),
        damaged: Some(UnitCounts::Map(damaged)),
        guns: Some(UnitCounts::Map(guns)),
    })
}

/// The side of a space combat that `input` gives in full, and the ships it fields by unit id. A
/// side with no faction fights as the generic one.
///
/// # Errors
///
/// Returns a [`CalcError`] if the side names a player (that needs the state) or a unit id is
/// unknown.
pub fn build_side(
    content: &ContentStore,
    sources: SourceSet,
    input: &SideInput,
) -> Result<(Side, BTreeMap<String, usize>), CalcError> {
    if input.player.is_some() {
        return Err(CalcError::new("state is required when player is specified"));
    }
    let tally = |counts: &Option<UnitCounts>| {
        let mut tally: BTreeMap<String, usize> = BTreeMap::new();
        for (id, count) in counts
            .clone()
            .map(UnitCounts::into_entries)
            .unwrap_or_default()
        {
            *tally.entry(id).or_default() += count;
        }
        tally
    };
    let fleet = tally(&input.units);
    let damaged = tally(&input.damaged);
    let guns = tally(&input.guns);
    let faction = input.faction.as_deref().unwrap_or("generic");

    // Validate unit IDs
    for (id, &count) in &fleet {
        if count > 0 && ti4_content::units::unit_type(content, id, sources).is_none() {
            return Err(CalcError::new(format!("unknown unit id in fleet: {id}")));
        }
    }
    for (id, &count) in &damaged {
        if count > 0 && ti4_content::units::unit_type(content, id, sources).is_none() {
            return Err(CalcError::new(format!("unknown unit id in damaged: {id}")));
        }
    }
    for (id, &count) in &guns {
        if count > 0 && ti4_content::units::unit_type(content, id, sources).is_none() {
            return Err(CalcError::new(format!("unknown unit id in guns: {id}")));
        }
    }

    let fleet_vec: Vec<(String, usize)> =
        fleet.clone().into_iter().filter(|(_, n)| *n > 0).collect();
    let damaged_vec: Vec<(String, usize)> = damaged.into_iter().filter(|(_, n)| *n > 0).collect();
    let guns_vec: Vec<(String, usize)> = guns.into_iter().filter(|(_, n)| *n > 0).collect();

    let side =
        Side::of(content, &fleet_vec, &damaged_vec, faction, true).with_guns(content, &guns_vec);

    Ok((side, fleet))
}

/// How the rollouts of a space combat run. Each field is as in the request of the advisor.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpaceOptions {
    /// Number of rollout simulations (defaults to 2000, capped at 50000).
    pub simulations: Option<usize>,
    /// Whether attacker's space cannon fires before combat (defaults to true).
    pub attacker_cannon: Option<bool>,
    /// Whether combat is already in progress (bypasses space cannon and round-1 AFB).
    pub in_progress: Option<bool>,
    /// Optional seed for deterministic simulations.
    pub seed: Option<u64>,
}

/// Space combat odds from [`arena`] rollouts. Each side is what [`resolve_side`] returns.
///
/// # Errors
///
/// Returns a [`CalcError`] if the number of simulations is out of range.
#[allow(clippy::cast_precision_loss)]
pub fn space_odds(
    attacker: (Side, BTreeMap<String, usize>),
    defender: (Side, BTreeMap<String, usize>),
    options: SpaceOptions,
) -> Result<BattleResponse, CalcError> {
    let (attacker_side, attacker_fielded) = attacker;
    let (defender_side, defender_fielded) = defender;

    let simulations = match options.simulations {
        Some(0) => return Err(CalcError::new("simulations must be at least 1")),
        Some(n) if n > MAX_SIMULATIONS => {
            return Err(CalcError::new(format!(
                "simulations cannot exceed {MAX_SIMULATIONS}"
            )));
        }
        Some(n) => n,
        None => DEFAULT_SIMULATIONS,
    };

    let attacker_cannon = options.attacker_cannon.unwrap_or(true);
    let in_progress = options.in_progress.unwrap_or(false);
    let base_seed = options.seed.unwrap_or(0);

    let mut attacker_wins = 0usize;
    let mut defender_wins = 0usize;
    let mut mutual_destruction = 0usize;
    let mut unresolved = 0usize;
    let mut total_rounds = 0u64;

    let mut attacker_survivors = vec![0u64; attacker_side.names().len()];
    let mut defender_survivors = vec![0u64; defender_side.names().len()];

    for i in 0..simulations {
        let seed = base_seed.wrapping_add(i as u64);
        let outcome = if in_progress {
            arena::fight_in_progress(&attacker_side, &defender_side, seed)
        } else {
            arena::fight_outcome(&attacker_side, &defender_side, seed, attacker_cannon)
        };

        if outcome.unresolved {
            unresolved += 1;
        }
        match outcome.winner {
            Some("a") => attacker_wins += 1,
            Some("b") => defender_wins += 1,
            _ => mutual_destruction += 1,
        }
        total_rounds += u64::from(outcome.rounds);

        for (idx, &count) in outcome.attacker_left.iter().enumerate() {
            attacker_survivors[idx] += count as u64;
        }
        for (idx, &count) in outcome.defender_left.iter().enumerate() {
            defender_survivors[idx] += count as u64;
        }
    }

    let sim_f64 = simulations as f64;
    let mut attacker_expected_survivors = BTreeMap::new();
    for (idx, name) in attacker_side.names().iter().enumerate() {
        attacker_expected_survivors.insert(name.clone(), attacker_survivors[idx] as f64 / sim_f64);
    }
    let mut defender_expected_survivors = BTreeMap::new();
    for (idx, name) in defender_side.names().iter().enumerate() {
        defender_expected_survivors.insert(name.clone(), defender_survivors[idx] as f64 / sim_f64);
    }

    Ok(BattleResponse {
        simulations,
        attacker_win_rate: attacker_wins as f64 / sim_f64,
        defender_win_rate: defender_wins as f64 / sim_f64,
        mutual_destruction_rate: mutual_destruction as f64 / sim_f64,
        unresolved_rate: unresolved as f64 / sim_f64,
        average_rounds: total_rounds as f64 / sim_f64,
        attacker_expected_survivors,
        defender_expected_survivors,
        attacker_fielded,
        defender_fielded,
    })
}

/// Space combat odds of two sides given in full, with no state: each is built by [`build_side`].
/// A side may field and fire at most [`MAX_PLACEMENTS`] units.
///
/// # Errors
///
/// Returns a [`CalcError`] if a side names a player, has too many units or an unknown unit id,
/// or if the number of simulations is out of range.
pub fn space_odds_of(
    content: &ContentStore,
    sources: SourceSet,
    attacker: &SideInput,
    defender: &SideInput,
    options: SpaceOptions,
) -> Result<BattleResponse, CalcError> {
    for side in [attacker, defender] {
        for counts in [&side.units, &side.damaged, &side.guns] {
            let total = counts
                .clone()
                .map(UnitCounts::into_entries)
                .unwrap_or_default()
                .into_iter()
                .try_fold(0usize, |sum, (_, count)| sum.checked_add(count));
            if total.is_none_or(|total| total > MAX_PLACEMENTS) {
                return Err(CalcError::new(format!(
                    "a side has more than {MAX_PLACEMENTS} units"
                )));
            }
        }
    }
    space_odds(
        build_side(content, sources, attacker)?,
        build_side(content, sources, defender)?,
        options,
    )
}

/// Ground combat odds from [`arena`] rollouts.
///
/// # Errors
///
/// Returns a [`CalcError`] if a count, a unit id or the number of simulations is invalid.
#[allow(clippy::cast_precision_loss)]
pub fn ground_odds(
    content: &ContentStore,
    request: &GroundOddsRequest,
) -> Result<GroundOddsResponse, CalcError> {
    let n = request.simulations.unwrap_or(DEFAULT_SIMULATIONS);
    if n == 0 || n > MAX_SIMULATIONS {
        return Err(CalcError::new("simulations must be between 1 and 50000"));
    }
    if !request.attacker.guns.is_empty() {
        return Err(CalcError::new(
            "only the defender has standing defense guns",
        ));
    }
    if request.harrow.len() > MAX_PLACEMENTS
        || request.harrow.values().any(|count| *count > MAX_PLACEMENTS)
        || request.harrow.values().sum::<usize>() > MAX_PLACEMENTS
    {
        return Err(CalcError::new("too many Harrow units"));
    }
    for id in request.harrow.keys() {
        if !ti4_content::units::unit_type(content, id, POK)
            .is_some_and(|unit| unit.has_bombardment() && unit.bombard_dice() > 0)
        {
            return Err(CalcError::new(format!("invalid Harrow unit: {id}")));
        }
    }
    let resolve = |side: &GroundOddsSide| -> Result<GroundSide, CalcError> {
        if side.units.len() > MAX_PLACEMENTS
            || side.guns.len() > MAX_PLACEMENTS
            || side.units.values().any(|count| *count > MAX_PLACEMENTS)
            || side.guns.values().any(|count| *count > MAX_PLACEMENTS)
            || side.units.values().sum::<usize>() > MAX_PLACEMENTS
            || side.guns.values().sum::<usize>() > MAX_PLACEMENTS
        {
            return Err(CalcError::new("too many ground units"));
        }
        for (id, count) in &side.units {
            if *count > 0
                && !ti4_content::units::unit_type(content, id, POK)
                    .is_some_and(|unit| unit.is_ground_force())
            {
                return Err(CalcError::new(format!("invalid ground force: {id}")));
            }
            if side.damaged.get(id).copied().unwrap_or(0) > *count {
                return Err(CalcError::new("damaged count exceeds force count"));
            }
        }
        if side.damaged.keys().any(|id| !side.units.contains_key(id)) {
            return Err(CalcError::new("damaged unit is not fielded"));
        }
        for id in side.guns.keys() {
            if !ti4_content::units::unit_type(content, id, POK)
                .is_some_and(|unit| unit.space_cannon_hits_on().is_some())
            {
                return Err(CalcError::new(format!("invalid defense gun: {id}")));
            }
        }
        let forces: Vec<_> = side.units.iter().map(|(id, n)| (id.clone(), *n)).collect();
        let damaged: Vec<_> = side
            .damaged
            .iter()
            .map(|(id, n)| (id.clone(), *n))
            .collect();
        let guns: Vec<_> = side.guns.iter().map(|(id, n)| (id.clone(), *n)).collect();
        Ok(GroundSide::of(content, &forces, &damaged, &side.faction)
            .with_defense_guns(content, &guns))
    };
    let harrow: Vec<_> = request
        .harrow
        .iter()
        .map(|(id, count)| (id.clone(), *count))
        .collect();
    let attacker = resolve(&request.attacker)?.with_bombardment(content, &harrow, false, true);
    let defender = resolve(&request.defender)?;
    let mut wins = 0usize;
    let mut holds = 0usize;
    let mut mutual = 0usize;
    let mut unresolved = 0usize;
    let mut total_rounds = 0u64;
    let mut attacker_left = vec![0u64; attacker.names().len()];
    let mut defender_left = vec![0u64; defender.names().len()];
    for seed in 0..n {
        let outcome = arena::ground_fight(&attacker, &defender, seed as u64);
        let defender_stands = outcome.defender_left.iter().any(|count| *count > 0);
        if outcome.winner == Some("a") {
            wins += 1;
        } else if defender_stands {
            holds += 1;
        } else {
            mutual += 1;
        }
        if outcome.unresolved {
            unresolved += 1;
        }
        total_rounds += u64::from(outcome.rounds);
        for (sum, count) in attacker_left.iter_mut().zip(&outcome.attacker_left) {
            *sum += *count as u64;
        }
        for (sum, count) in defender_left.iter_mut().zip(&outcome.defender_left) {
            *sum += *count as u64;
        }
    }
    let rate = |count: usize| count as f64 / n as f64;
    let expected = |side: &GroundSide, sums: &[u64]| -> BTreeMap<String, f64> {
        side.names()
            .iter()
            .zip(sums)
            .map(|(name, sum)| (name.clone(), *sum as f64 / n as f64))
            .collect()
    };
    let fielded = |side: &GroundSide| -> BTreeMap<String, usize> {
        side.names().iter().cloned().zip(side.fielded()).collect()
    };
    let rolls = |side: &GroundSide| -> BTreeMap<String, GroundRoll> {
        side.names()
            .iter()
            .zip(side.rolls())
            .filter_map(|(name, roll)| {
                roll.map(|(hits_on, dice, modifier)| {
                    (
                        name.clone(),
                        GroundRoll {
                            hits_on,
                            dice,
                            modifier,
                        },
                    )
                })
            })
            .collect()
    };
    Ok(GroundOddsResponse {
        simulations: n,
        attacker_win_rate: rate(wins),
        defender_win_rate: rate(holds),
        mutual_destruction_rate: rate(mutual),
        unresolved_rate: rate(unresolved),
        average_rounds: total_rounds as f64 / n as f64,
        attacker_expected_survivors: expected(&attacker, &attacker_left),
        defender_expected_survivors: expected(&defender, &defender_left),
        attacker_fielded: fielded(&attacker),
        defender_fielded: fielded(&defender),
        attacker_rolls: rolls(&attacker),
        defender_rolls: rolls(&defender),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn odds(
        state: Option<&GameState>,
        system: Option<&SystemId>,
        attacker: &ParticipantInput,
        defender: &ParticipantInput,
        options: SpaceOptions,
    ) -> Result<BattleResponse, CalcError> {
        let content = ContentStore::embedded();
        let attacker = resolve_side(attacker, state, system, None, content, POK)?;
        let defender = resolve_side(defender, state, system, None, content, POK)?;
        space_odds(attacker, defender, options)
    }

    fn side(faction: &str, units: UnitCounts) -> ParticipantInput {
        ParticipantInput::Side(SideInput {
            faction: Some(faction.to_owned()),
            units: Some(units),
            ..Default::default()
        })
    }

    #[test]
    fn direct_fleet_battle_simulation_computes_odds() {
        let attacker = side(
            "sol",
            UnitCounts::List(vec![("carrier".to_owned(), 1), ("fighter".to_owned(), 4)]),
        );
        let defender = side(
            "letnev",
            UnitCounts::Map([("dreadnought".to_owned(), 1)].into_iter().collect()),
        );
        let options = SpaceOptions {
            simulations: Some(100),
            seed: Some(42),
            ..Default::default()
        };

        let response = odds(None, None, &attacker, &defender, options).expect("battle succeeds");
        assert_eq!(response.simulations, 100);
        let total_rate = response.attacker_win_rate
            + response.defender_win_rate
            + response.mutual_destruction_rate;
        assert!((total_rate - 1.0).abs() < 1e-6);
        assert_eq!(response.attacker_fielded.get("carrier"), Some(&1));
        assert_eq!(response.attacker_fielded.get("fighter"), Some(&4));
        assert_eq!(response.defender_fielded.get("dreadnought"), Some(&1));
        assert!(response.average_rounds > 0.0);
    }

    #[test]
    fn battle_from_game_state_snapshot_resolves_fleets() {
        let mut state = ti4_engine::fixtures::game(&["a", "b"]);
        let sys = SystemId::new("18");
        let a_player = PlayerId::new("a");
        let b_player = PlayerId::new("b");

        for (unit, owner) in [
            ("cruiser", &a_player),
            ("cruiser", &a_player),
            ("destroyer", &b_player),
        ] {
            state
                .system_mut(&sys)
                .units
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new(unit),
                    owner.clone(),
                ));
        }

        let options = SpaceOptions {
            simulations: Some(50),
            seed: Some(123),
            ..Default::default()
        };
        let response = odds(
            Some(&state),
            Some(&sys),
            &ParticipantInput::Player(a_player),
            &ParticipantInput::Player(b_player),
            options,
        )
        .expect("state battle succeeds");
        assert_eq!(response.simulations, 50);
        assert_eq!(response.attacker_fielded.get("cruiser"), Some(&2));
        assert_eq!(response.defender_fielded.get("destroyer"), Some(&1));
        assert!(response.attacker_win_rate > response.defender_win_rate);
    }

    #[test]
    fn a_gathered_side_builds_what_resolve_side_gives() {
        use ti4_engine::fixtures::{hub_from, plain_systems, put, put_on_planet};
        use ti4_model::id::PlanetId;

        let content = ContentStore::embedded();
        let hub = hub_from(&plain_systems(7));
        let mut state = ti4_engine::fixtures::game(&["a", "b"]);
        let sys = SystemId::new(hub.centre.clone());
        let next = SystemId::new(hub.outer[0].clone());
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        put(&mut state, &sys, "cruiser", &a, 2);
        put(&mut state, &sys, "dreadnought", &b, 2);
        put(&mut state, &sys, "fighter", &b, 3);
        state.system_mut(&sys).units[2].sustained_damage = true;
        put_on_planet(&mut state, &sys, &PlanetId::new("here"), "pds", &b, 1);
        put_on_planet(&mut state, &next, &PlanetId::new("there"), "pds2", &b, 1);
        let options = SpaceOptions {
            simulations: Some(200),
            seed: Some(7),
            ..Default::default()
        };

        for (player, extra) in [(&a, None), (&b, Some(("carrier", 1)))] {
            let participant = ParticipantInput::Side(SideInput {
                player: Some(player.clone()),
                units: extra
                    .map(|(id, n): (&str, usize)| UnitCounts::List(vec![(id.to_owned(), n)])),
                ..Default::default()
            });
            let resolved = resolve_side(
                &participant,
                Some(&state),
                Some(&sys),
                Some(&hub.galaxy),
                content,
                POK,
            )
            .expect("resolves");
            let gathered = gather_side(content, POK, &state, Some(&sys), Some(&hub.galaxy), player)
                .expect("gathers");
            assert_eq!(gathered.player, None);
            let gathered = SideInput {
                units: joined(gathered.units, participant_units(&participant)),
                ..gathered
            };
            let built = build_side(content, POK, &gathered).expect("builds");
            assert_eq!(built.1, resolved.1);
            assert_eq!(built.0.names(), resolved.0.names());
            assert_eq!(built.0.fielded(), resolved.0.fielded());
            let foe = side(
                "letnev",
                UnitCounts::List(vec![("destroyer".to_owned(), 3)]),
            );
            let foe = resolve_side(&foe, None, None, None, content, POK).expect("foe");
            assert_eq!(
                space_odds(built, foe.clone(), options),
                space_odds(resolved, foe, options),
            );
        }

        let gathered =
            gather_side(content, POK, &state, Some(&sys), Some(&hub.galaxy), &b).expect("gathers");
        let count = |counts: &Option<UnitCounts>, id: &str| match counts {
            Some(UnitCounts::Map(map)) => map.get(id).copied().unwrap_or(0),
            _ => 0,
        };
        assert_eq!(count(&gathered.units, "dreadnought"), 2);
        assert_eq!(count(&gathered.damaged, "dreadnought"), 1);
        assert_eq!(count(&gathered.guns, "pds"), 1);
        assert_eq!(count(&gathered.guns, "pds2"), 1, "a pds next door reaches");
    }

    fn participant_units(participant: &ParticipantInput) -> Option<UnitCounts> {
        match participant {
            ParticipantInput::Side(side) => side.units.clone(),
            ParticipantInput::Player(_) => None,
        }
    }

    #[test]
    fn sides_without_state_give_the_odds_of_sides_with_it() {
        let content = ContentStore::embedded();
        let attacker = SideInput {
            faction: Some("sol".to_owned()),
            units: Some(UnitCounts::Map(
                [("carrier".to_owned(), 1), ("fighter".to_owned(), 3)]
                    .into_iter()
                    .collect(),
            )),
            ..Default::default()
        };
        let defender = SideInput {
            faction: Some("letnev".to_owned()),
            units: Some(UnitCounts::List(vec![("dreadnought".to_owned(), 1)])),
            damaged: Some(UnitCounts::List(vec![("dreadnought".to_owned(), 1)])),
            guns: Some(UnitCounts::List(vec![("pds".to_owned(), 1)])),
            ..Default::default()
        };
        let options = SpaceOptions {
            simulations: Some(300),
            seed: Some(11),
            ..Default::default()
        };
        let direct = space_odds_of(content, POK, &attacker, &defender, options).expect("odds");
        let resolved = odds(
            None,
            None,
            &ParticipantInput::Side(attacker.clone()),
            &ParticipantInput::Side(defender.clone()),
            options,
        )
        .expect("odds");
        assert_eq!(direct, resolved);
        assert_eq!(direct.defender_fielded.get("dreadnought"), Some(&1));

        let json = serde_json::to_value(&defender).expect("serializes");
        assert_eq!(
            serde_json::from_value::<SideInput>(json).expect("reads back"),
            defender
        );

        let named = SideInput {
            player: Some(PlayerId::new("a")),
            ..attacker.clone()
        };
        assert!(space_odds_of(content, POK, &named, &defender, options).is_err());
        let swarm = SideInput {
            units: Some(UnitCounts::List(vec![(
                "fighter".to_owned(),
                MAX_PLACEMENTS + 1,
            )])),
            ..attacker
        };
        assert!(space_odds_of(content, POK, &swarm, &defender, options).is_err());
    }

    #[test]
    fn battle_validation_rejects_invalid_inputs() {
        let fighter = || UnitCounts::List(vec![("fighter".to_owned(), 1)]);
        let attacker = side("sol", fighter());
        let defender = side("letnev", fighter());
        let simulations = |n| SpaceOptions {
            simulations: Some(n),
            ..Default::default()
        };

        // 0 simulations rejected
        assert!(odds(None, None, &attacker, &defender, simulations(0)).is_err());

        // Simulations exceeding MAX_SIMULATIONS rejected
        assert!(
            odds(
                None,
                None,
                &attacker,
                &defender,
                simulations(MAX_SIMULATIONS + 1)
            )
            .is_err()
        );

        // Unknown unit ID rejected
        let bad_unit = side(
            "sol",
            UnitCounts::List(vec![("nonexistent_ship".to_owned(), 1)]),
        );
        assert!(odds(None, None, &bad_unit, &defender, simulations(10)).is_err());

        // Player specified without state rejected
        let no_state = ParticipantInput::Player(PlayerId::new("missing_player"));
        assert!(odds(None, None, &no_state, &defender, simulations(10)).is_err());
    }

    #[test]
    fn ground_odds_counts_the_wins_of_the_invader() {
        let force = |faction: &str, count| GroundOddsSide {
            faction: faction.to_owned(),
            units: [("infantry".to_owned(), count)].into_iter().collect(),
            damaged: BTreeMap::new(),
            guns: BTreeMap::new(),
        };
        let mut request = GroundOddsRequest {
            attacker: force("sol", 4),
            defender: force("letnev", 1),
            harrow: BTreeMap::new(),
            simulations: Some(200),
        };
        let content = ContentStore::embedded();
        let response = ground_odds(content, &request).expect("ground odds");
        assert_eq!(response.simulations, 200);
        assert!(response.attacker_win_rate > 0.5);

        request.simulations = Some(0);
        assert!(ground_odds(content, &request).is_err());
    }

    #[test]
    fn ground_odds_split_every_rollout_and_count_what_is_left() {
        let content = ContentStore::embedded();
        let side = |faction: &str, units: &[(&str, usize)]| GroundOddsSide {
            faction: faction.to_owned(),
            units: units
                .iter()
                .map(|(id, count)| ((*id).to_owned(), *count))
                .collect(),
            damaged: BTreeMap::new(),
            guns: BTreeMap::new(),
        };
        let request = GroundOddsRequest {
            attacker: side("sol", &[("infantry", 2), ("sol_mech", 1)]),
            defender: side("letnev", &[("infantry", 2)]),
            harrow: BTreeMap::new(),
            simulations: Some(500),
        };
        let response = ground_odds(content, &request).expect("ground odds");

        // The attacker's rate is the share of `ground_fight` that it wins, seed by seed.
        let attacker = GroundSide::of(
            content,
            &[("infantry".to_owned(), 2), ("sol_mech".to_owned(), 1)],
            &[],
            "sol",
        )
        .with_bombardment(content, &[], false, true);
        let defender = GroundSide::of(content, &[("infantry".to_owned(), 2)], &[], "letnev");
        let wins = (0..500u64)
            .filter(|seed| arena::ground_fight(&attacker, &defender, *seed).winner == Some("a"))
            .count();
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let expected_rate = wins as f64 / 500.0;
        assert!((response.attacker_win_rate - expected_rate).abs() < f64::EPSILON);

        let total = response.attacker_win_rate
            + response.defender_win_rate
            + response.mutual_destruction_rate;
        assert!((total - 1.0).abs() < 1e-9);
        assert!(response.mutual_destruction_rate > 0.0);
        assert!(response.defender_win_rate > 0.0);
        assert!(response.unresolved_rate <= response.defender_win_rate);
        assert!(response.average_rounds >= 1.0);

        assert_eq!(
            response.attacker_fielded,
            [("infantry".to_owned(), 2), ("sol_mech".to_owned(), 1)]
                .into_iter()
                .collect()
        );
        assert_eq!(
            response.defender_fielded,
            [("infantry".to_owned(), 2)].into_iter().collect()
        );
        for (fielded, survivors) in [
            (
                &response.attacker_fielded,
                &response.attacker_expected_survivors,
            ),
            (
                &response.defender_fielded,
                &response.defender_expected_survivors,
            ),
        ] {
            assert_eq!(
                fielded.keys().collect::<Vec<_>>(),
                survivors.keys().collect::<Vec<_>>()
            );
            for (id, left) in survivors {
                #[expect(clippy::cast_precision_loss, reason = "small counts")]
                let most = fielded[id] as f64;
                assert!((0.0..=most).contains(left), "{id}: {left}");
            }
        }
        assert!(response.attacker_expected_survivors["infantry"] > 0.0);
        assert_eq!(
            response.attacker_rolls["infantry"],
            GroundRoll {
                hits_on: 8,
                dice: 1,
                modifier: 0
            }
        );
        assert_eq!(response.attacker_rolls["sol_mech"].dice, 1);
    }

    #[test]
    fn ground_odds_say_what_a_fragile_side_rolls() {
        let content = ContentStore::embedded();
        let side = |faction: &str, units: &[(&str, usize)]| GroundOddsSide {
            faction: faction.to_owned(),
            units: units
                .iter()
                .map(|(id, count)| ((*id).to_owned(), *count))
                .collect(),
            damaged: BTreeMap::new(),
            guns: BTreeMap::new(),
        };
        let request = GroundOddsRequest {
            attacker: side("jolnar", &[("infantry", 2), ("jolnar_mech", 1)]),
            defender: side("sardakk", &[("infantry", 1)]),
            harrow: BTreeMap::new(),
            simulations: Some(20),
        };
        let response = ground_odds(content, &request).expect("ground odds");
        // Shield Paling: the infantry of Jol-Nar ignore Fragile while the mech is on the planet.
        assert_eq!(response.attacker_rolls["infantry"].modifier, 0);
        assert_eq!(response.attacker_rolls["jolnar_mech"].modifier, -1);
        assert_eq!(response.defender_rolls["infantry"].modifier, 1);
    }

    #[test]
    fn a_ground_combat_with_no_defender_is_won_in_no_round() {
        let content = ContentStore::embedded();
        let request = GroundOddsRequest {
            attacker: GroundOddsSide {
                faction: "sol".to_owned(),
                units: [("infantry".to_owned(), 1)].into_iter().collect(),
                damaged: BTreeMap::new(),
                guns: BTreeMap::new(),
            },
            defender: GroundOddsSide {
                faction: "letnev".to_owned(),
                units: BTreeMap::new(),
                damaged: BTreeMap::new(),
                guns: BTreeMap::new(),
            },
            harrow: BTreeMap::new(),
            simulations: Some(10),
        };
        let response = ground_odds(content, &request).expect("ground odds");
        assert!((response.attacker_win_rate - 1.0).abs() < f64::EPSILON);
        assert!(response.average_rounds.abs() < f64::EPSILON);
        assert!((response.attacker_expected_survivors["infantry"] - 1.0).abs() < f64::EPSILON);
        assert!(response.defender_expected_survivors.is_empty());
    }
}
