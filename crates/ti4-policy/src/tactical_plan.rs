//! Candidate fleets for a tactical action (activation rework, Phase 2).
//!
//! For one destination, a small, varied menu of fleets the seat could actually send together:
//! every move and load is checked against a shared ledger (Gravity Drive and the Ionian Fuel
//! Refinery once each, fleet supply at the destination, capacity per hull, each cargo unit once).
//! Moves and loads name units by what they are (origin, unit, damage, boost, cargo source), never
//! by option index, so a plan survives earlier moves reshuffling the engine's lists.
//!
//! The strategies only steer the search; none of them excludes a fleet for missing a win
//! threshold. Pricing uses the arena predictor where it covers the fight, and the strength index
//! only to order and shortlist hulls. See `plans/ACTIVATION_REWORK_PLAN_2026-09-17.md`.

use std::collections::{BTreeMap, BTreeSet};

use ti4_engine::choice::Observed;
use ti4_engine::transit::CargoSource;
use ti4_model::id::{PlanetId, PlayerId, SystemId};

use crate::battle::{self, BattlePredictor, BattleQuery};
use crate::fleet_strength::{self, Descriptors};

/// Bumped whenever the menu a position produces can change.
pub const GENERATOR_VERSION: u32 = 1;

/// Most predictor calls one destination's efficient search may make.
const EFFICIENT_EVALUATIONS: usize = 8;

/// Enemy fleets within this many hexes count as a threat to a system left empty.
const THREAT_DISTANCE: i32 = 2;

/// One ship moving into the destination.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HullMove {
    pub origin: SystemId,
    pub unit: String,
    pub damaged: bool,
    pub gravity_drive: bool,
    pub ionian: bool,
}

/// One unit carried by a moving ship.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Load {
    /// Index into [`Package::moves`] of the ship carrying it.
    pub hull: usize,
    pub unit: String,
    pub damaged: bool,
    pub galvanized: bool,
    /// `None` for the origin's space area.
    pub planet: Option<PlanetId>,
}

/// How a candidate was built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Strategy {
    /// Move nothing: activate to produce or reinforce.
    Hold,
    /// The cheapest ships found that are favoured against the defence (or, with no fight, the
    /// cheapest transport for the planets).
    Light,
    /// The best predicted win against own losses among a bounded search.
    Efficient,
    /// Everything that can go together.
    Strong,
    /// Efficient (or light) ships plus as many ground forces as they carry, sized for the planets.
    Capture,
    /// Efficient, without ships whose origin an enemy fleet could then reach unopposed.
    OriginPreserving,
}

impl Strategy {
    pub const ALL: [Self; 6] = [
        Self::Hold,
        Self::Light,
        Self::Efficient,
        Self::Strong,
        Self::Capture,
        Self::OriginPreserving,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Light => "light",
            Self::Efficient => "efficient",
            Self::Strong => "strong",
            Self::Capture => "capture",
            Self::OriginPreserving => "origin-preserving",
        }
    }
}

/// What a candidate is expected to do. Every probability is conditional and labelled as such.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PackageFacts {
    /// A space fight follows (enemy ships, or enemy guns that can fire).
    pub fight: bool,
    /// The fight is outside what the predictor covers.
    pub unsupported: bool,
    pub space_win: Option<f64>,
    pub space_loss: Option<f64>,
    /// Expected resources lost, in tens.
    pub own_cost_lost: Option<f64>,
    pub enemy_cost_lost: Option<f64>,
    /// The hardest defended planet taken if every carried ground force landed there, with nothing
    /// lost in space and before bombardment. `None` when nothing is carried or nothing is defended.
    pub ground_take_if_all_land: Option<f64>,
    /// Carried ground forces times the chance of losing the space fight.
    pub cargo_at_risk: f64,
    /// `ln(S_own / S_enemy)` after opening fire, the strength index's matchup comparison.
    pub strength_ratio: f64,
    pub hulls: usize,
    /// Resource cost of the moving ships, in tens.
    pub cost: f64,
    pub capacity_used: usize,
    pub ground_carried: usize,
    pub boosts_used: usize,
    /// The strongest enemy fleet (firepower x durability) that could reach a system this package
    /// empties of ships, in tens; zero when nothing is left exposed.
    pub origin_exposed: f64,
}

/// A commitment written out for comparison: the moves, and each load with the move carrying it.
type Commitment = (
    Vec<HullMove>,
    Vec<(HullMove, String, bool, bool, Option<PlanetId>)>,
);

/// When an assembly may stop adding ships.
type Enough<'x> = dyn Fn(&Setting<'_, '_>, &[HullMove], &[Load]) -> bool + 'x;

/// One executable candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub system: SystemId,
    pub strategy: Strategy,
    pub moves: Vec<HullMove>,
    pub loads: Vec<Load>,
    pub facts: PackageFacts,
}

impl Package {
    /// The commitment itself, for deduplication: strategy and facts left out.
    fn commitment(&self) -> Commitment {
        let mut moves = self.moves.clone();
        moves.sort();
        let mut loads: Vec<_> = self
            .loads
            .iter()
            .map(|load| {
                (
                    self.moves[load.hull].clone(),
                    load.unit.clone(),
                    load.damaged,
                    load.galvanized,
                    load.planet.clone(),
                )
            })
            .collect();
        loads.sort();
        (moves, loads)
    }
}

#[derive(Debug, Clone)]
struct Hull {
    moving: HullMove,
    capacity: usize,
    fighter: bool,
    cost: f64,
    /// Firepower x durability of this ship alone.
    weight: f64,
}

#[derive(Debug, Clone)]
struct Cargo {
    unit: String,
    damaged: bool,
    galvanized: bool,
    planet: Option<PlanetId>,
    fighter: bool,
}

/// Everything the strategies share for one destination.
struct Setting<'s, 'a> {
    seen: &'s Observed<'a>,
    player: &'s PlayerId,
    system: SystemId,
    faction: String,
    hulls: Vec<Hull>,
    cargo: BTreeMap<SystemId, Vec<Cargo>>,
    /// Non-fighter ships that may still arrive under fleet supply.
    supply_room: usize,
    /// The seat's ships already at the destination.
    present: Vec<(String, usize)>,
    present_damaged: Vec<(String, usize)>,
    enemy: Descriptors,
    fight: bool,
    /// Planets here the seat does not control, and the enemy ground forces on each.
    targets: Vec<(PlanetId, usize)>,
    threat: BTreeMap<SystemId, f64>,
    ships_at: BTreeMap<SystemId, usize>,
    predictor: Option<&'s BattlePredictor>,
}

fn tally(entries: impl IntoIterator<Item = String>) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for id in entries {
        *counts.entry(id).or_default() += 1;
    }
    counts.into_iter().collect()
}

impl Setting<'_, '_> {
    fn unit(&self, id: &str) -> Option<ti4_content::units::UnitType<'_>> {
        ti4_content::units::unit_type(self.seen.content(), id, self.seen.sources())
    }

    /// The seat's fleet at the destination if `moves` and the fighters in `loads` arrived.
    fn arriving(&self, moves: &[HullMove], loads: &[Load]) -> Vec<(String, bool)> {
        let mut out: Vec<(String, bool)> =
            moves.iter().map(|m| (m.unit.clone(), m.damaged)).collect();
        out.extend(
            loads
                .iter()
                .filter(|load| self.unit(&load.unit).is_some_and(|k| k.is_fighter()))
                .map(|load| (load.unit.clone(), load.damaged)),
        );
        out
    }

    fn descriptors(&self, arriving: &[(String, bool)]) -> Descriptors {
        let mut fleet: Vec<String> = self
            .present
            .iter()
            .flat_map(|(id, n)| std::iter::repeat_n(id.clone(), *n))
            .collect();
        fleet.extend(arriving.iter().map(|(id, _)| id.clone()));
        let mut damaged: Vec<String> = self
            .present_damaged
            .iter()
            .flat_map(|(id, n)| std::iter::repeat_n(id.clone(), *n))
            .collect();
        damaged.extend(
            arriving
                .iter()
                .filter(|(_, hurt)| *hurt)
                .map(|(id, _)| id.clone()),
        );
        fleet_strength::descriptors(
            self.seen.content(),
            self.seen.sources(),
            &self.faction,
            &tally(fleet),
            &tally(damaged),
        )
    }

    /// Ground forces the targets call for: one per undefended planet, defenders + 1 elsewhere.
    fn ground_wanted(&self) -> usize {
        self.targets
            .iter()
            .map(|(_, defenders)| defenders + 1)
            .sum()
    }

    /// Choose hulls in `order` under the ledger, stopping once `enough` says so, then fill their
    /// holds from their origins.
    fn assemble(
        &self,
        order: &[usize],
        enough: &Enough<'_>,
        fighters: bool,
        ground: usize,
    ) -> (Vec<HullMove>, Vec<Load>) {
        let mut moves: Vec<HullMove> = Vec::new();
        let mut chosen: Vec<usize> = Vec::new();
        let (mut gravity, mut ionian, mut room) = (false, false, self.supply_room);
        for &index in order {
            let hull = &self.hulls[index];
            if (hull.moving.gravity_drive && gravity) || (hull.moving.ionian && ionian) {
                continue;
            }
            if !hull.fighter {
                if room == 0 {
                    continue;
                }
                room -= 1;
            }
            gravity |= hull.moving.gravity_drive;
            ionian |= hull.moving.ionian;
            moves.push(hull.moving.clone());
            chosen.push(index);
            let loads = self.fill(&chosen, fighters, 0);
            if enough(self, &moves, &loads) {
                break;
            }
        }
        let loads = self.fill(&chosen, fighters, ground);
        (moves, loads)
    }

    /// Holds for `chosen` hulls: fighters first when asked, then up to `ground` ground forces.
    fn fill(&self, chosen: &[usize], fighters: bool, ground: usize) -> Vec<Load> {
        let mut pools: BTreeMap<&SystemId, Vec<&Cargo>> = BTreeMap::new();
        for (origin, cargo) in &self.cargo {
            pools.insert(origin, cargo.iter().collect());
        }
        // A fighter that moves on its own is not also cargo.
        for &index in chosen {
            let hull = &self.hulls[index];
            if !hull.fighter {
                continue;
            }
            if let Some(pool) = pools.get_mut(&hull.moving.origin)
                && let Some(at) = pool.iter().position(|c| {
                    c.fighter && c.unit == hull.moving.unit && c.damaged == hull.moving.damaged
                })
            {
                pool.remove(at);
            }
        }
        let mut loads = Vec::new();
        let mut ground_left = ground;
        for (slot, &index) in chosen.iter().enumerate() {
            let hull = &self.hulls[index];
            let mut space = hull.capacity;
            let Some(pool) = pools.get_mut(&hull.moving.origin) else {
                continue;
            };
            let mut take = |want_fighter: bool, limit: &mut usize, space: &mut usize| {
                while *space > 0 && *limit > 0 {
                    let Some(at) = pool.iter().position(|c| c.fighter == want_fighter) else {
                        break;
                    };
                    let cargo = pool.remove(at);
                    loads.push(Load {
                        hull: slot,
                        unit: cargo.unit.clone(),
                        damaged: cargo.damaged,
                        galvanized: cargo.galvanized,
                        planet: cargo.planet.clone(),
                    });
                    *space -= 1;
                    *limit -= 1;
                }
            };
            if fighters {
                let mut unlimited = usize::MAX;
                take(true, &mut unlimited, &mut space);
            }
            take(false, &mut ground_left, &mut space);
        }
        loads
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one fact after another, read top to bottom"
    )]
    fn price(&self, strategy: Strategy, moves: Vec<HullMove>, loads: Vec<Load>) -> Package {
        let arriving = self.arriving(&moves, &loads);
        let own = self.descriptors(&arriving);
        let mut facts = PackageFacts {
            fight: self.fight,
            strength_ratio: fleet_strength::matchup(&own, &self.enemy),
            hulls: moves.len(),
            capacity_used: loads.len(),
            ..PackageFacts::default()
        };
        facts.boosts_used = moves
            .iter()
            .map(|m| usize::from(m.gravity_drive) + usize::from(m.ionian))
            .sum();
        facts.cost = moves
            .iter()
            .filter_map(|m| self.unit(&m.unit).map(|k| k.cost()))
            .sum::<f64>()
            / 10.0;
        let ground: Vec<String> = loads
            .iter()
            .filter(|load| self.unit(&load.unit).is_some_and(|k| !k.is_fighter()))
            .map(|load| load.unit.clone())
            .collect();
        facts.ground_carried = ground.len();

        if let Some(predictor) = self.predictor {
            let version = predictor.feature_version;
            let refs: Vec<(&str, bool)> = arriving
                .iter()
                .map(|(id, hurt)| (id.as_str(), *hurt))
                .collect();
            match battle::fleet_query(self.seen, self.player, &self.system, &refs, version) {
                BattleQuery::NotApplicable => facts.fight = false,
                BattleQuery::Unsupported(_) => {
                    facts.fight = true;
                    facts.unsupported = true;
                }
                BattleQuery::Supported { attacker, defender } => {
                    facts.fight = true;
                    if let Ok(input) = battle::encode(version, &attacker, &defender) {
                        let p = predictor.predict(&input);
                        facts.space_win = Some(f64::from(p.attacker_wins));
                        facts.space_loss = Some(f64::from(p.defender_wins));
                        facts.own_cost_lost = Some(battle::space_cost_lost(
                            self.seen,
                            &attacker,
                            &p.attacker_survival,
                        ));
                        facts.enemy_cost_lost = Some(battle::space_cost_lost(
                            self.seen,
                            &defender,
                            &p.defender_survival,
                        ));
                    } else {
                        facts.unsupported = true;
                    }
                }
            }
            if !ground.is_empty() && predictor.has_ground() {
                let landing = tally(ground.iter().cloned());
                let mut hardest: Option<f64> = None;
                for (planet, _) in &self.targets {
                    let Ok(Some((attacker, defender))) = battle::landing_query(
                        self.seen,
                        self.player,
                        &self.system,
                        planet,
                        &landing,
                    ) else {
                        continue;
                    };
                    let take = battle::encode_ground(&attacker, &defender)
                        .ok()
                        .and_then(|input| predictor.predict_ground(&input))
                        .map(|p| f64::from(p.attacker_wins));
                    if let Some(take) = take {
                        hardest = Some(hardest.map_or(take, |h: f64| h.min(take)));
                    }
                }
                facts.ground_take_if_all_land = hardest;
            }
        }
        #[expect(clippy::cast_precision_loss, reason = "cargo counts are small")]
        let carried = facts.ground_carried as f64;
        facts.cargo_at_risk = carried * facts.space_loss.unwrap_or(0.0);

        let mut leaving: BTreeMap<&SystemId, usize> = BTreeMap::new();
        for m in &moves {
            *leaving.entry(&m.origin).or_default() += 1;
        }
        facts.origin_exposed = leaving
            .iter()
            .filter(|(origin, gone)| self.ships_at.get(**origin).copied().unwrap_or(0) <= **gone)
            .map(|(origin, _)| self.threat.get(*origin).copied().unwrap_or(0.0))
            .fold(0.0, f64::max)
            / 10.0;

        Package {
            system: self.system.clone(),
            strategy,
            moves,
            loads,
            facts,
        }
    }

    fn favoured(&self, moves: &[HullMove], loads: &[Load]) -> bool {
        if !self.fight {
            return true;
        }
        fleet_strength::matchup(&self.descriptors(&self.arriving(moves, loads)), &self.enemy) > 0.0
    }
}

/// Enemy fleet strength (firepower x durability) that could reach each of the seat's systems.
#[must_use]
pub fn threats(seen: &Observed<'_>, player: &PlayerId) -> BTreeMap<SystemId, f64> {
    let Some(galaxy) = seen.galaxy() else {
        return BTreeMap::new();
    };
    let (content, sources) = (seen.content(), seen.sources());
    let mut stacks: Vec<(String, f64)> = Vec::new();
    for id in galaxy.system_ids() {
        let here = seen.system(&SystemId::new(id));
        let mut by_owner: BTreeMap<&PlayerId, Vec<String>> = BTreeMap::new();
        for unit in &here.units {
            if &unit.owner == player {
                continue;
            }
            if ti4_content::units::unit_type(content, unit.type_id.as_str(), sources)
                .is_some_and(|k| k.is_ship())
            {
                by_owner
                    .entry(&unit.owner)
                    .or_default()
                    .push(unit.type_id.to_string());
            }
        }
        for (owner, ships) in by_owner {
            let faction = seen
                .seat(owner)
                .map(|seat| seat.faction.as_str().to_owned())
                .unwrap_or_default();
            let d = fleet_strength::descriptors(content, sources, &faction, &tally(ships), &[]);
            stacks.push((id.to_owned(), d.firepower * d.durability));
        }
    }
    let mut out = BTreeMap::new();
    for origin in seen.systems_with_units_of(player) {
        let worst = stacks
            .iter()
            .filter(|(at, _)| {
                galaxy
                    .distance(at, origin.as_str())
                    .is_some_and(|d| d <= THREAT_DISTANCE)
            })
            .map(|(_, weight)| *weight)
            .fold(0.0, f64::max);
        if worst > 0.0 {
            out.insert(origin.clone(), worst);
        }
    }
    out
}

/// The candidate fleets for activating `system`, deduplicated by commitment, in strategy order.
///
/// `predictor` prices each candidate; without one the facts carry only the strength index and the
/// counts. An empty menu means nothing can move there and there is nothing to hold.
#[must_use]
pub fn packages(
    seen: &Observed<'_>,
    player: &PlayerId,
    system: &SystemId,
    predictor: Option<&BattlePredictor>,
) -> Vec<Package> {
    let threat = threats(seen, player);
    packages_with(seen, player, system, predictor, &threat)
}

/// [`packages`] with the enemy threat map already computed, for pricing many destinations at once.
#[must_use]
pub fn packages_with(
    seen: &Observed<'_>,
    player: &PlayerId,
    system: &SystemId,
    predictor: Option<&BattlePredictor>,
    threat: &BTreeMap<SystemId, f64>,
) -> Vec<Package> {
    let (setting, can_hold) = setting(seen, player, system, predictor, threat);
    build(&setting, can_hold)
}

/// Every subset of the movable ships (each with its fighters, no ground forces), priced like the
/// efficient search, when there are at most `max_hulls` of them: the reference menu the shortlist
/// is measured against. `None` when there are more, or when nothing can move.
#[must_use]
pub fn exhaustive(
    seen: &Observed<'_>,
    player: &PlayerId,
    system: &SystemId,
    predictor: Option<&BattlePredictor>,
    threat: &BTreeMap<SystemId, f64>,
    max_hulls: usize,
) -> Option<Vec<Package>> {
    let (setting, _) = setting(seen, player, system, predictor, threat);
    let n = setting.hulls.len();
    if n == 0 || n > max_hulls {
        return None;
    }
    let never: &Enough<'_> = &|_, _, _| false;
    let mut out: Vec<Package> = Vec::new();
    for mask in 1..(1u32 << n) {
        let order: Vec<usize> = (0..n).filter(|i| mask & (1 << i) != 0).collect();
        let (moves, loads) = setting.assemble(&order, never, true, 0);
        if moves.len() != order.len() {
            continue; // the ledger refused part of this subset
        }
        let package = setting.price(Strategy::Efficient, moves, loads);
        if !out
            .iter()
            .any(|known| known.commitment() == package.commitment())
        {
            out.push(package);
        }
    }
    Some(out)
}

/// Gather what every strategy needs for one destination, and whether holding is meaningful.
#[expect(
    clippy::too_many_lines,
    reason = "gathers the whole setting in one pass; each part is a few lines"
)]
fn setting<'s, 'a>(
    seen: &'s Observed<'a>,
    player: &'s PlayerId,
    system: &SystemId,
    predictor: Option<&'s BattlePredictor>,
    threat: &BTreeMap<SystemId, f64>,
) -> (Setting<'s, 'a>, bool) {
    let (content, sources) = (seen.content(), seen.sources());
    let kind = |id: &str| ti4_content::units::unit_type(content, id, sources);
    let faction = seen
        .seat(player)
        .map(|seat| seat.faction.as_str().to_owned())
        .unwrap_or_default();
    let here = seen.system(system);

    let weight_of = |id: &str| {
        let d = fleet_strength::descriptors(content, sources, &faction, &[(id.to_owned(), 1)], &[]);
        d.firepower * d.durability
    };
    let hulls: Vec<Hull> = seen
        .movable_into(player, system)
        .into_iter()
        .filter_map(|movable| {
            let id = movable.unit.type_id.to_string();
            let unit = kind(&id)?;
            Some(Hull {
                capacity: usize::try_from(movable.capacity).unwrap_or(0),
                fighter: unit.is_fighter(),
                cost: unit.cost(),
                weight: weight_of(&id),
                moving: HullMove {
                    origin: movable.origin,
                    unit: id,
                    damaged: movable.unit.sustained_damage,
                    gravity_drive: movable.gravity_drive,
                    ionian: movable.ionian,
                },
            })
        })
        .collect();

    let origins: BTreeSet<SystemId> = hulls.iter().map(|h| h.moving.origin.clone()).collect();
    let mut cargo = BTreeMap::new();
    for origin in &origins {
        let list: Vec<Cargo> = seen
            .loadable(player, origin)
            .into_iter()
            .map(|c| Cargo {
                fighter: kind(c.unit.type_id.as_str()).is_some_and(|k| k.is_fighter()),
                unit: c.unit.type_id.to_string(),
                damaged: c.unit.sustained_damage,
                galvanized: c.unit.galvanized,
                planet: match c.source {
                    CargoSource::Space => None,
                    CargoSource::Planet(planet) => Some(planet),
                },
            })
            .collect();
        cargo.insert(origin.clone(), list);
    }

    let is_ship = |id: &str| kind(id).is_some_and(|k| k.is_ship());
    let is_fighter = |id: &str| kind(id).is_some_and(|k| k.is_fighter());
    let own_here: Vec<&ti4_model::units::Unit> = here
        .units
        .iter()
        .filter(|u| &u.owner == player && is_ship(u.type_id.as_str()))
        .collect();
    let present = tally(own_here.iter().map(|u| u.type_id.to_string()));
    let present_damaged = tally(
        own_here
            .iter()
            .filter(|u| u.sustained_damage)
            .map(|u| u.type_id.to_string()),
    );
    let non_fighters_here = own_here
        .iter()
        .filter(|u| !is_fighter(u.type_id.as_str()))
        .count();
    let limit = usize::try_from(seen.fleet_supply_limit(player)).unwrap_or(0);
    let supply_room = limit.saturating_sub(non_fighters_here);

    // The defence: other players' ships here, plus every other player's guns that can fire in.
    let enemy_ships: Vec<&ti4_model::units::Unit> = here
        .units
        .iter()
        .filter(|u| &u.owner != player && is_ship(u.type_id.as_str()))
        .collect();
    let enemy_faction = enemy_ships
        .first()
        .and_then(|u| seen.seat(&u.owner))
        .map(|seat| seat.faction.as_str().to_owned())
        .unwrap_or_default();
    let mut enemy = fleet_strength::descriptors(
        content,
        sources,
        &enemy_faction,
        &tally(enemy_ships.iter().map(|u| u.type_id.to_string())),
        &tally(
            enemy_ships
                .iter()
                .filter(|u| u.sustained_damage)
                .map(|u| u.type_id.to_string()),
        ),
    );
    let guns: Vec<String> = here
        .planet_units
        .values()
        .flatten()
        .filter(|u| {
            &u.owner != player && kind(u.type_id.as_str()).is_some_and(|k| k.has_space_cannon())
        })
        .map(|u| u.type_id.to_string())
        .collect();
    enemy.cannon += fleet_strength::descriptors(
        content,
        sources,
        &enemy_faction,
        &tally(guns.iter().cloned()),
        &[],
    )
    .cannon;
    let fight = !enemy_ships.is_empty() || !guns.is_empty();

    let controlled: BTreeSet<PlanetId> = seen
        .controlled_planets(player)
        .into_iter()
        .filter(|(at, _)| *at == system)
        .map(|(_, planet)| planet.clone())
        .collect();
    let targets: Vec<(PlanetId, usize)> =
        ti4_content::galaxy::system(content, system.as_str(), sources)
            .map(|tile| tile.planets())
            .unwrap_or_default()
            .into_iter()
            .map(PlanetId::new)
            .filter(|planet| !controlled.contains(planet))
            .map(|planet| {
                let defenders = here
                    .on_planet(&planet)
                    .iter()
                    .filter(|u| {
                        &u.owner != player
                            && kind(u.type_id.as_str()).is_some_and(|k| k.is_ground_force())
                    })
                    .count();
                (planet, defenders)
            })
            .collect();

    let mut ships_at: BTreeMap<SystemId, usize> = BTreeMap::new();
    for origin in &origins {
        let count = seen
            .system(origin)
            .units
            .iter()
            .filter(|u| &u.owner == player && is_ship(u.type_id.as_str()))
            .count();
        ships_at.insert(origin.clone(), count);
    }

    let setting = Setting {
        seen,
        player,
        system: system.clone(),
        faction,
        hulls,
        cargo,
        supply_room,
        present,
        present_damaged,
        enemy,
        fight,
        targets,
        threat: threat.clone(),
        ships_at,
        predictor,
    };
    (setting, !own_here.is_empty() || !controlled.is_empty())
}

/// The trade the efficient search maximises: predicted win against own losses in tens, or the
/// strength index when the fight is not priced.
#[must_use]
pub fn efficiency(package: &Package) -> f64 {
    if let (Some(win), Some(lost)) = (package.facts.space_win, package.facts.own_cost_lost) {
        win - lost
    } else {
        (package.facts.strength_ratio.clamp(-3.0, 3.0) / 3.0) - package.facts.cost
    }
}

fn build(setting: &Setting<'_, '_>, can_hold: bool) -> Vec<Package> {
    let hulls = &setting.hulls;
    let indices: Vec<usize> = (0..hulls.len()).collect();
    let by = |key: &dyn Fn(&Hull) -> f64, descending: bool| -> Vec<usize> {
        let mut order = indices.clone();
        order.sort_by(|a, b| {
            let (x, y) = (key(&hulls[*a]), key(&hulls[*b]));
            let ordering = x.total_cmp(&y);
            let ordering = if descending {
                ordering.reverse()
            } else {
                ordering
            };
            ordering.then_with(|| hulls[*a].moving.cmp(&hulls[*b].moving))
        });
        order
    };
    let strongest = by(&|h| h.weight, true);
    let cheapest = by(&|h| h.cost + if h.capacity > 0 { 0.0 } else { 0.01 }, false);
    let efficient_order = by(&|h| h.weight / h.cost.max(0.5), true);
    let wanted = setting.ground_wanted();
    let never: &Enough<'_> = &|_, _, _| false;
    let favoured: &Enough<'_> =
        &|s, m, l| s.favoured(m, l) && (s.fight || s.hull_capacity(m) >= s.ground_wanted().min(1));

    let mut out: Vec<Package> = Vec::new();
    let mut push = |package: Package| {
        if (!package.moves.is_empty() || package.strategy == Strategy::Hold)
            && !out
                .iter()
                .any(|known| known.commitment() == package.commitment())
        {
            out.push(package);
        }
    };

    if can_hold {
        push(setting.price(Strategy::Hold, Vec::new(), Vec::new()));
    }
    if hulls.is_empty() {
        return out;
    }

    let (light_moves, light_loads) = setting.assemble(&cheapest, favoured, true, 0);
    let light = setting.price(Strategy::Light, light_moves.clone(), light_loads);
    push(light);

    // Efficient: price growing prefixes of the efficiency order and keep the best trade.
    let efficient_moves = setting.efficient(&efficient_order);
    let efficient_order_used: Vec<usize> = efficient_order
        .iter()
        .copied()
        .filter(|i| efficient_moves.contains(&hulls[*i].moving))
        .collect();
    let (moves, loads) = setting.assemble(&efficient_order_used, never, true, 0);
    push(setting.price(Strategy::Efficient, moves, loads));

    let (moves, loads) = setting.assemble(&strongest, never, true, usize::MAX);
    push(setting.price(Strategy::Strong, moves, loads));

    if wanted > 0 {
        // Capture: the efficient ships (or the light ones when there is no fight), then more
        // transport until the planets' call for ground forces is met.
        let base: Vec<usize> = if setting.fight {
            efficient_order_used.clone()
        } else {
            cheapest
                .iter()
                .copied()
                .filter(|i| light_moves.contains(&hulls[*i].moving))
                .collect()
        };
        let mut order = base.clone();
        let mut by_capacity = by(
            &|h| f64::from(u32::try_from(h.capacity).unwrap_or(u32::MAX)),
            true,
        );
        by_capacity.retain(|i| !base.contains(i) && hulls[*i].capacity > 0);
        order.extend(by_capacity);
        let base_len = base.len();
        let enough = move |s: &Setting<'_, '_>, m: &[HullMove], _: &[Load]| {
            m.len() >= base_len && s.hull_capacity(m) >= s.ground_wanted()
        };
        let (moves, loads) = setting.assemble(&order, &enough, false, wanted);
        push(setting.price(Strategy::Capture, moves, loads));
    }

    // Origin-preserving: the efficient search without ships from threatened origins.
    let safe: Vec<usize> = efficient_order
        .iter()
        .copied()
        .filter(|i| !setting.threat.contains_key(&hulls[*i].moving.origin))
        .collect();
    if !safe.is_empty() && safe.len() < efficient_order.len() {
        let chosen = setting.efficient(&safe);
        let order: Vec<usize> = safe
            .iter()
            .copied()
            .filter(|i| chosen.contains(&hulls[*i].moving))
            .collect();
        let (moves, loads) = setting.assemble(&order, never, true, 0);
        push(setting.price(Strategy::OriginPreserving, moves, loads));
    }
    out
}

impl Setting<'_, '_> {
    fn hull_capacity(&self, moves: &[HullMove]) -> usize {
        moves
            .iter()
            .filter_map(|m| {
                self.hulls
                    .iter()
                    .find(|h| &h.moving == m)
                    .map(|h| h.capacity)
            })
            .sum()
    }

    /// The prefix of `order` with the best predicted win against own losses; the strength index
    /// stands in when there is no predictor or no fight. Returns the chosen moves.
    fn efficient(&self, order: &[usize]) -> Vec<HullMove> {
        let never: &Enough<'_> = &|_, _, _| false;
        let (all, _) = self.assemble(order, never, false, 0);
        if all.is_empty() {
            return all;
        }
        let n = all.len();
        let step = n.div_ceil(EFFICIENT_EVALUATIONS).max(1);
        let mut best: Option<(f64, usize)> = None;
        let mut k = 1;
        while k <= n {
            let prefix = &all[..k];
            let loads = self.fill(
                &order
                    .iter()
                    .copied()
                    .filter(|i| prefix.contains(&self.hulls[*i].moving))
                    .collect::<Vec<_>>(),
                true,
                0,
            );
            let score = if self.fight {
                let package = self.price(Strategy::Efficient, prefix.to_vec(), loads);
                efficiency(&package)
            } else {
                // No fight: the cheapest ship that carries anything is the efficient one.
                -f64::from(u32::try_from(k).unwrap_or(u32::MAX))
            };
            if best.is_none_or(|(s, _)| score > s + 1e-9) {
                best = Some((score, k));
            }
            if k == n {
                break;
            }
            k = (k + step).min(n);
        }
        let k = best.map_or(n, |(_, k)| k);
        all[..k].to_vec()
    }
}

/// Where a plan stands while the engine asks for its steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanState {
    /// Answering movement and cargo prompts from the plan.
    Executing,
    /// Movement finished as planned.
    Complete,
    /// Something the plan relied on was not offered. Whether an event explains it is the
    /// caller's call: with no intervening event it is a generation error.
    NotOffered(String),
}

/// A chosen package being carried out, one engine prompt at a time.
///
/// Answers only the movement step (`move…`, `done_moving`) and the holds that follow each move
/// (`load…`, `done_loading`); every other prompt is left to the caller. Options are matched by
/// what they carry (origin, unit, damage, boosts; cargo unit, source, damage), never by index.
#[derive(Debug, Clone)]
pub struct Execution {
    pub package: Package,
    pub state: PlanState,
    next_move: usize,
    /// The move whose hold is open, if any.
    loading: Option<usize>,
    loaded: Vec<bool>,
    dropped: usize,
}

fn text<'o>(option: &'o ti4_engine::choice::ChoiceOption, key: &str) -> Option<&'o str> {
    option.payload.get(key).and_then(serde_json::Value::as_str)
}

fn flag(option: &ti4_engine::choice::ChoiceOption, key: &str) -> bool {
    option
        .payload
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

impl Execution {
    #[must_use]
    pub fn new(package: Package) -> Self {
        let loaded = vec![false; package.loads.len()];
        Self {
            package,
            state: PlanState::Executing,
            next_move: 0,
            loading: None,
            loaded,
            dropped: 0,
        }
    }

    /// Whether this prompt is one the plan answers.
    #[must_use]
    pub fn handles(choice: &ti4_engine::choice::Choice) -> bool {
        choice.options.iter().any(|option| {
            option.kind == ti4_engine::tactical::MOVE_KIND
                || option.kind == ti4_engine::transit::LOAD_KIND
                || option.id == "done_moving"
                || option.id == "done_loading"
        })
    }

    /// The option id the plan picks, or `None` when the prompt is not the plan's to answer or
    /// the plan can no longer be followed (see [`Self::state`]).
    pub fn answer(&mut self, choice: &ti4_engine::choice::Choice) -> Option<String> {
        if self.state != PlanState::Executing || !Self::handles(choice) {
            return None;
        }
        if choice
            .options
            .iter()
            .any(|option| option.id == "done_loading")
        {
            return Some(self.load(choice));
        }
        if choice
            .options
            .iter()
            .any(|option| option.id == "done_moving")
        {
            self.loading = None;
            let Some(wanted) = self.package.moves.get(self.next_move) else {
                self.state = PlanState::Complete;
                return Some("done_moving".to_owned());
            };
            let found = choice.options.iter().find(|option| {
                option.kind == ti4_engine::tactical::MOVE_KIND
                    && text(option, "origin") == Some(wanted.origin.as_str())
                    && text(option, "unit") == Some(wanted.unit.as_str())
                    && flag(option, "damaged") == wanted.damaged
                    && flag(option, "gravity_drive") == wanted.gravity_drive
                    && flag(option, "ionian") == wanted.ionian
            });
            return if let Some(option) = found {
                self.loading = Some(self.next_move);
                self.next_move += 1;
                Some(option.id.clone())
            } else {
                self.state = PlanState::NotOffered(format!(
                    "move {} from {}{}{} not offered",
                    wanted.unit,
                    wanted.origin,
                    if wanted.damaged { " (damaged)" } else { "" },
                    if wanted.gravity_drive || wanted.ionian {
                        " with a boost"
                    } else {
                        ""
                    },
                ));
                None
            };
        }
        None
    }

    fn load(&mut self, choice: &ti4_engine::choice::Choice) -> String {
        let Some(hull) = self.loading else {
            return "done_loading".to_owned();
        };
        for (index, load) in self.package.loads.iter().enumerate() {
            if self.loaded[index] || load.hull != hull {
                continue;
            }
            let source = load.planet.as_ref().map(PlanetId::as_str);
            let found = choice.options.iter().find(|option| {
                option.kind == ti4_engine::transit::LOAD_KIND
                    && text(option, "unit") == Some(load.unit.as_str())
                    && text(option, "source") == source
                    && flag(option, "damaged") == load.damaged
                    && flag(option, "galvanized") == load.galvanized
            });
            if let Some(option) = found {
                self.loaded[index] = true;
                return option.id.clone();
            }
            // A planned load that is not offered is dropped rather than failing the move: the
            // ship still goes. Counted so a generator that plans impossible cargo shows up.
            self.loaded[index] = true;
            self.dropped += 1;
        }
        "done_loading".to_owned()
    }

    /// Planned loads the engine did not offer.
    #[must_use]
    pub const fn dropped_loads(&self) -> usize {
        self.dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::fixtures;

    fn seat(state: &mut ti4_model::state::GameState, id: &str, faction: &str) -> PlayerId {
        let player = PlayerId::new(id);
        state.player_mut(&player).unwrap().faction = ti4_model::id::FactionId::new(faction);
        player
    }

    #[test]
    fn a_contested_destination_gets_a_varied_executable_menu() {
        let hub = fixtures::plain_hub();
        let mut state = fixtures::game(&["a", "b"]);
        let a = seat(&mut state, "a", "letnev");
        let b = seat(&mut state, "b", "hacan");
        state.player_mut(&a).unwrap().fleet_tokens = 3;
        let centre = SystemId::new(hub.centre.clone());
        let home = SystemId::new(hub.outer[0].clone());
        let side = SystemId::new(hub.outer[3].clone());
        fixtures::put(&mut state, &home, "dreadnought", &a, 1);
        fixtures::put(&mut state, &home, "carrier", &a, 1);
        fixtures::put(&mut state, &home, "infantry", &a, 3);
        fixtures::put(&mut state, &home, "fighter", &a, 1);
        fixtures::put(&mut state, &side, "destroyer", &a, 2);
        fixtures::put(&mut state, &centre, "cruiser", &b, 1);
        let seen = Observed::new(
            &state,
            ti4_content::ContentStore::embedded(),
            ti4_model::POK,
            Some(&hub.galaxy),
        );

        let menu = packages(&seen, &a, &centre, None);
        let labels: Vec<&str> = menu.iter().map(|p| p.strategy.label()).collect();
        assert!(labels.contains(&"strong"), "{labels:?}");
        let strong = menu
            .iter()
            .find(|p| p.strategy == Strategy::Strong)
            .unwrap();
        // Armada: fleet supply 3 + 2, so all four non-fighters go.
        assert_eq!(strong.moves.len(), 4);
        let carried: usize = strong.loads.len();
        assert_eq!(
            carried, 4,
            "the carrier takes the fighter and all three infantry"
        );
        assert!(strong.facts.fight);
        assert!(strong.facts.strength_ratio > 0.0);
        for package in &menu {
            let mut seen_moves = package.moves.clone();
            seen_moves.dedup();
            assert!(package.moves.iter().filter(|m| m.gravity_drive).count() <= 1);
            for load in &package.loads {
                assert!(load.hull < package.moves.len());
            }
        }
        assert!(
            menu.iter().all(|p| p.strategy != Strategy::Hold),
            "nothing of a's is there"
        );
    }

    #[test]
    fn a_fighter_moving_on_its_own_is_not_also_loaded() {
        let hub = fixtures::plain_hub();
        let mut state = fixtures::game(&["a", "b"]);
        let a = seat(&mut state, "a", "hacan");
        let _ = seat(&mut state, "b", "sol");
        let centre = SystemId::new(hub.centre.clone());
        let home = SystemId::new(hub.outer[0].clone());
        fixtures::put(&mut state, &home, "carrier", &a, 1);
        fixtures::put(&mut state, &home, "fighter2", &a, 1);
        let seen = Observed::new(
            &state,
            ti4_content::ContentStore::embedded(),
            ti4_model::POK,
            Some(&hub.galaxy),
        );
        for package in packages(&seen, &a, &centre, None) {
            let moved = package
                .moves
                .iter()
                .filter(|m| m.unit == "fighter2")
                .count();
            let carried = package
                .loads
                .iter()
                .filter(|l| l.unit == "fighter2")
                .count();
            assert!(moved + carried <= 1, "{:?}", package.strategy);
        }
    }

    #[test]
    fn fleet_supply_caps_the_strong_package() {
        let hub = fixtures::plain_hub();
        let mut state = fixtures::game(&["a", "b"]);
        let a = seat(&mut state, "a", "hacan");
        let _ = seat(&mut state, "b", "sol");
        state.player_mut(&a).unwrap().fleet_tokens = 1;
        let centre = SystemId::new(hub.centre.clone());
        let home = SystemId::new(hub.outer[0].clone());
        fixtures::put(&mut state, &home, "destroyer", &a, 1);
        fixtures::put(&mut state, &home, "dreadnought", &a, 1);
        let seen = Observed::new(
            &state,
            ti4_content::ContentStore::embedded(),
            ti4_model::POK,
            Some(&hub.galaxy),
        );
        let menu = packages(&seen, &a, &centre, None);
        assert!(!menu.is_empty());
        assert!(
            menu.iter().all(|p| p.moves.len() == 1),
            "one non-fighter fits"
        );
        assert!(
            menu.iter().any(|p| p.moves[0].unit == "dreadnought"),
            "the strongest single ship is on the menu"
        );
    }

    #[test]
    fn an_owned_system_can_be_held_without_moving() {
        let hub = fixtures::plain_hub();
        let mut state = fixtures::game(&["a", "b"]);
        let a = seat(&mut state, "a", "sol");
        let _ = seat(&mut state, "b", "xxcha");
        let centre = SystemId::new(hub.centre.clone());
        fixtures::put(&mut state, &centre, "carrier", &a, 1);
        let seen = Observed::new(
            &state,
            ti4_content::ContentStore::embedded(),
            ti4_model::POK,
            Some(&hub.galaxy),
        );
        let menu = packages(&seen, &a, &centre, None);
        assert_eq!(menu.len(), 1);
        assert_eq!(menu[0].strategy, Strategy::Hold);
        assert!(menu[0].moves.is_empty());
    }
}
