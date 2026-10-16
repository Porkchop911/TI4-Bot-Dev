//! Power map: for one faction and every system, its **immediate defence** there and its
//! **projection** there -- the most force one activation could have in the system.
//!
//! Every quantity is a [`fleet_strength::Descriptors`]: expected hits per combat round, hits the
//! force can absorb (SUSTAIN DAMAGE included), and the opening fire around a combat (space
//! cannon, anti-fighter barrage, bombardment). [`fleet_strength::matchup`] compares two sides with
//! the planner's Lanchester index after opening fire; a calibration against the battle arena turns
//! that into a win probability elsewhere.
//!
//! - **Defence** is what stands there now: ships in the space area, and on the faction's planets
//!   its ground forces, space cannon and planetary shields.
//! - **Projection** is the maximum one activation could put there: the ships already there plus
//!   every ship [`Observed::movable_into`] says could move in, strongest first, up to the faction's
//!   fleet supply. Space projection fills the arriving ships' capacity with fighters; ground
//!   projection fills the same capacity with ground forces. Each is its own maximum -- one move
//!   cannot do both -- because projection measures potential, not a plan.
//!
//! Projection ignores whether the faction could legally activate the system right now (a command
//! token already there, no tactic token left); [`SystemPower::can_activate`] reports that.
//!
//! Public information only: units on the board, command tokens and the map.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_engine::choice::Observed;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::units::Unit;

use crate::fleet_strength::{self, Descriptors};

/// Units by id with counts, and how many of each start damaged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Composition {
    pub units: Vec<(String, usize)>,
    pub damaged: Vec<(String, usize)>,
}

impl Composition {
    fn of(units: &[&Unit]) -> Self {
        let mut all: BTreeMap<String, usize> = BTreeMap::new();
        let mut hurt: BTreeMap<String, usize> = BTreeMap::new();
        for unit in units {
            *all.entry(unit.type_id.to_string()).or_default() += 1;
            if unit.sustained_damage {
                *hurt.entry(unit.type_id.to_string()).or_default() += 1;
            }
        }
        Self {
            units: all.into_iter().collect(),
            damaged: hurt.into_iter().collect(),
        }
    }

    /// Total units.
    #[must_use]
    pub fn count(&self) -> usize {
        self.units.iter().map(|(_, n)| n).sum()
    }
}

/// One faction's force at one system.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemPower {
    /// Ships in the space area now, with the faction's space cannon here firing first.
    pub space_defence: Descriptors,
    pub space_defence_units: Composition,
    /// Space cannon units (PDS and the like) on the faction's planets here, by id.
    pub guns: Vec<(String, usize)>,
    /// Ground forces on the faction's planets here now.
    pub ground_defence: Descriptors,
    pub ground_defence_units: Composition,
    /// A planetary shield stands on at least one of the faction's planets here.
    pub shielded: bool,
    /// The most ships one activation could have here, fighters filling spare capacity.
    pub space_projection: Descriptors,
    pub space_projection_units: Composition,
    /// The most ground forces one activation could have here: those already on the faction's
    /// planets plus what the arriving ships could carry.
    pub ground_projection: Descriptors,
    pub ground_projection_units: Composition,
    /// Expected bombardment hits from the projected ships, before landing.
    pub bombardment: f64,
    /// Ships that would arrive from elsewhere (not counting those already here or fighters).
    pub arriving_ships: usize,
    /// The faction could activate this system now: no command token of its own here, and a
    /// tactic token to spend.
    pub can_activate: bool,
}

fn describe(
    content: &ContentStore,
    sources: SourceSet,
    faction: &str,
    comp: &Composition,
) -> Descriptors {
    fleet_strength::descriptors(content, sources, faction, &comp.units, &comp.damaged)
}

/// The Lanchester index of one unit alone, for "strongest first" ordering.
fn unit_strength(content: &ContentStore, sources: SourceSet, faction: &str, unit: &Unit) -> f64 {
    let d = describe(content, sources, faction, &Composition::of(&[unit]));
    d.firepower * d.durability.max(1.0) + d.cannon + d.capacity * 0.01
}

/// The power map of `player`: one entry per system on the board. Empty without a map.
#[must_use]
pub fn power_map(seen: &Observed<'_>, player: &PlayerId) -> BTreeMap<SystemId, SystemPower> {
    map_with(seen, player, true, None)
}

/// [`power_map`] with only the immediate-defence layer filled: no movement search, so cheap.
#[must_use]
pub fn defence_map(seen: &Observed<'_>, player: &PlayerId) -> BTreeMap<SystemId, SystemPower> {
    map_with(seen, player, false, None)
}

/// The map for `player`, restricted to `only` when given (the systems a cache must refresh).
#[expect(
    clippy::too_many_lines,
    reason = "one pass over the board, layer by layer"
)]
fn map_with(
    seen: &Observed<'_>,
    player: &PlayerId,
    project: bool,
    only: Option<&std::collections::BTreeSet<SystemId>>,
) -> BTreeMap<SystemId, SystemPower> {
    let mut out = BTreeMap::new();
    let Some(galaxy) = seen.galaxy() else {
        return out;
    };
    let content = seen.content();
    let sources = seen.sources();
    let Some(seat) = seen.seat(player) else {
        return out;
    };
    let faction = seat.faction.as_str().to_owned();
    let tactic_tokens = seat.tactic_tokens;
    let supply = usize::try_from(seen.fleet_supply_limit(player)).unwrap_or(0);
    let shift = fleet_strength::modifier(&faction);
    let kind = |unit: &Unit| ti4_content::units::unit_type(content, unit.type_id.as_str(), sources);
    let is_fighter = |unit: &Unit| kind(unit).is_some_and(|k| k.is_fighter());
    let is_ship = |unit: &Unit| kind(unit).is_some_and(|k| k.is_ship());
    let is_ground = |unit: &Unit| kind(unit).is_some_and(|k| k.is_ground_force());

    for id in galaxy.system_ids() {
        let system = SystemId::new(id);
        if only.is_some_and(|only| !only.contains(&system)) {
            continue;
        }
        let state = seen.system(&system);
        let mut power = SystemPower {
            can_activate: !state.command_tokens.contains(player) && tactic_tokens > 0,
            ..SystemPower::default()
        };

        // --- immediate defence ---------------------------------------------------------------
        let ships_here: Vec<&Unit> = state
            .units_of(player)
            .into_iter()
            .filter(|u| is_ship(u))
            .collect();
        power.space_defence_units = Composition::of(&ships_here);
        let mut ground_here: Vec<&Unit> = Vec::new();
        let mut guns: BTreeMap<String, usize> = BTreeMap::new();
        for units in state.planet_units.values() {
            for unit in units.iter().filter(|u| &u.owner == player) {
                let Some(k) = kind(unit) else { continue };
                if is_ground(unit) {
                    ground_here.push(unit);
                }
                if k.has_space_cannon() {
                    *guns.entry(unit.type_id.to_string()).or_default() += 1;
                }
                power.shielded |= k.planetary_shield();
            }
        }
        power.guns = guns.into_iter().collect();
        power.ground_defence_units = Composition::of(&ground_here);
        power.space_defence = describe(content, sources, &faction, &power.space_defence_units);
        power.ground_defence = describe(content, sources, &faction, &power.ground_defence_units);
        // Cannon on planets here fires before a space combat in this system and, as space cannon
        // defence, at ground forces landing on the planet it stands on.
        #[expect(clippy::cast_precision_loss, reason = "dice counts are small")]
        let planet_cannon: f64 = power
            .guns
            .iter()
            .filter_map(|(id, n)| {
                let k = ti4_content::units::unit_type(content, id, sources)?;
                Some(
                    *n as f64
                        * k.space_cannon_dice() as f64
                        * k.space_cannon_hits_on()
                            .map_or(0.0, |on| fleet_strength::hit_chance(on, shift)),
                )
            })
            .sum();
        // Ships' own space cannon already sits in `descriptors().cannon`; planet guns are added.
        power.space_defence.cannon += planet_cannon;
        power.ground_defence.cannon = planet_cannon;

        // --- projection ---------------------------------------------------------------------
        if !project {
            out.insert(system, power);
            continue;
        }
        let movers = seen.movable_into(player, &system);
        let mut arriving: Vec<&ti4_engine::tactical::Movable> = movers
            .iter()
            .filter(|m| m.origin != system && !is_fighter(&m.unit))
            .collect();
        arriving.sort_by(|a, b| {
            unit_strength(content, sources, &faction, &b.unit)
                .total_cmp(&unit_strength(content, sources, &faction, &a.unit))
        });
        let present_non_fighters = ships_here.iter().filter(|u| !is_fighter(u)).count();
        let room = supply.saturating_sub(present_non_fighters);
        arriving.truncate(room);
        power.arriving_ships = arriving.len();

        // Capacity of the arriving ships by origin; cargo is loaded at origins.
        let mut capacity_by_origin: BTreeMap<SystemId, i64> = BTreeMap::new();
        for m in &arriving {
            *capacity_by_origin.entry(m.origin.clone()).or_default() += m.capacity.max(0);
        }
        // Fighters that could fly in on their own (a move value of their own) also count.
        let self_moving_fighters: Vec<&Unit> = movers
            .iter()
            .filter(|m| m.origin != system && is_fighter(&m.unit))
            .map(|m| &m.unit)
            .collect();
        let mut carried_fighters: Vec<Unit> = Vec::new();
        let mut carried_ground: Vec<Unit> = Vec::new();
        for (origin, capacity) in &capacity_by_origin {
            let cap = usize::try_from(*capacity).unwrap_or(0);
            let cargo: Vec<Unit> = seen
                .loadable(player, origin)
                .into_iter()
                .map(|c| c.unit)
                .collect();
            let fighters: Vec<Unit> = cargo.iter().filter(|u| is_fighter(u)).cloned().collect();
            carried_fighters.extend(fighters.into_iter().take(cap));
            let mut ground: Vec<Unit> = cargo.into_iter().filter(|u| is_ground(u)).collect();
            ground.sort_by(|a, b| {
                unit_strength(content, sources, &faction, b)
                    .total_cmp(&unit_strength(content, sources, &faction, a))
            });
            carried_ground.extend(ground.into_iter().take(cap));
        }

        let mut space_units: Vec<&Unit> = ships_here.clone();
        space_units.extend(arriving.iter().map(|m| &m.unit));
        space_units.extend(self_moving_fighters.iter().copied());
        space_units.extend(carried_fighters.iter());
        power.space_projection_units = Composition::of(&space_units);
        power.space_projection =
            describe(content, sources, &faction, &power.space_projection_units);

        let mut ground_units: Vec<&Unit> = ground_here.clone();
        ground_units.extend(carried_ground.iter());
        power.ground_projection_units = Composition::of(&ground_units);
        power.ground_projection =
            describe(content, sources, &faction, &power.ground_projection_units);

        #[expect(clippy::cast_precision_loss, reason = "dice counts are small")]
        {
            power.bombardment = space_units
                .iter()
                .filter_map(|u| kind(u))
                .map(|k| {
                    k.bombard_dice() as f64
                        * k.bombard_hits_on()
                            .map_or(0.0, |on| fleet_strength::hit_chance(on, shift))
                })
                .sum();
        }
        out.insert(system, power);
    }
    out
}

/// Calibration of [`fleet_strength::matchup`] against the battle arena (2026-10-02, 135,355 space
/// and 98,179 ground matchups from greedy self-play of checkpoint-49232, 100 arena fights each):
/// `P(win) = σ(a · ln ratio + b)`. Mean absolute error 0.050 (space) and 0.044 (ground) against
/// the arena; the favourite is named correctly in 96.7% and 97.0% of matchups.
pub const SPACE_CALIBRATION: (f64, f64) = (1.7794, -0.1383);
/// See [`SPACE_CALIBRATION`].
pub const GROUND_CALIBRATION: (f64, f64) = (1.8012, -0.1328);

/// Calibrated win probability of `attacker` against `defender`.
#[must_use]
pub fn win_probability(
    attacker: &Descriptors,
    defender: &Descriptors,
    calibration: (f64, f64),
) -> f64 {
    let x = calibration.0 * fleet_strength::matchup(attacker, defender) + calibration.1;
    1.0 / (1.0 + (-x).exp())
}

/// Opportunity: the planet value (resources + influence) `player` could expect to take with one
/// activation, summed over systems. Per system: the chance its projected ships beat every
/// opponent's ships there (the weakest pairwise chance), times the value of opponents' planets
/// weighted by the chance its projected ground forces beat that owner's ground forces (with
/// bombardment unless shielded), plus uncontrolled planets when any ground force can land.
///
/// Projection is searched only for `player`; opponents contribute their immediate defence, which
/// needs no movement search.
#[must_use]
pub fn opportunity(seen: &Observed<'_>, player: &PlayerId) -> f64 {
    summary(seen, player).opportunity
}

/// One seat's power projection in three numbers.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Summary {
    /// [`opportunity`]: expected planet value one activation could take, summed over systems.
    pub opportunity: f64,
    /// The largest single-system term of that sum.
    pub best_target: f64,
    /// Systems with planets that some of the seat's ships could reach with one activation.
    pub reach_systems: f64,
}

/// [`opportunity`] together with its best single target and the number of systems in reach.
#[must_use]
pub fn summary(seen: &Observed<'_>, player: &PlayerId) -> Summary {
    let own = power_map(seen, player);
    let others: Vec<(PlayerId, BTreeMap<SystemId, SystemPower>)> = seen
        .players()
        .into_iter()
        .filter(|p| *p != player)
        .map(|p| (p.clone(), defence_map(seen, p)))
        .collect();
    summarize(seen, player, &own, &others)
}

/// The summary from maps already in hand: `own` with projection, `others` with defence.
#[must_use]
pub fn summarize(
    seen: &Observed<'_>,
    player: &PlayerId,
    own: &BTreeMap<SystemId, SystemPower>,
    others: &[(PlayerId, BTreeMap<SystemId, SystemPower>)],
) -> Summary {
    let by_system = opportunity_by_system(seen, player, own, others);
    Summary {
        opportunity: by_system.values().sum(),
        best_target: by_system.values().copied().fold(0.0, f64::max),
        #[expect(
            clippy::cast_precision_loss,
            reason = "a board has a few dozen systems"
        )]
        reach_systems: by_system.len() as f64,
    }
}

/// Each reachable system with planets, and the planet value `player` could expect to take there
/// with one activation. Systems out of reach are absent; [`summarize`] sums this map.
#[must_use]
pub fn opportunity_by_system(
    seen: &Observed<'_>,
    player: &PlayerId,
    own: &BTreeMap<SystemId, SystemPower>,
    others: &[(PlayerId, BTreeMap<SystemId, SystemPower>)],
) -> BTreeMap<SystemId, f64> {
    let mut found = BTreeMap::new();
    let Some(galaxy) = seen.galaxy() else {
        return found;
    };
    let content = seen.content();
    let sources = seen.sources();
    let value = |planet: &str| {
        ti4_content::galaxy::planet(content, planet, sources)
            .map_or(0, |p| p.resources() + p.influence())
    };
    for id in galaxy.system_ids() {
        let system = SystemId::new(id);
        let Some(mine) = own.get(&system) else {
            continue;
        };
        if mine.space_projection_units.count() == 0 {
            continue;
        }
        let state = seen.system(&system);
        let planets: Vec<&str> = ti4_content::galaxy::system(content, id, sources)
            .map(|s| s.planets())
            .unwrap_or_default();
        if planets.is_empty() {
            continue;
        }
        let mut control = 1.0_f64;
        for (_, map) in others {
            if let Some(theirs) = map.get(&system) {
                if theirs.space_defence.durability > 0.0 {
                    control = control.min(win_probability(
                        &mine.space_projection,
                        &theirs.space_defence,
                        SPACE_CALIBRATION,
                    ));
                }
            }
        }
        if control <= 0.0 {
            found.insert(system, 0.0);
            continue;
        }
        let has_ground = mine.ground_projection_units.count() > 0;
        let mut take = 0.0;
        for planet in planets {
            let owner = state
                .planet_control
                .get(&ti4_model::id::PlanetId::new(planet));
            #[expect(
                clippy::cast_precision_loss,
                reason = "planet values are single digits"
            )]
            let worth = value(planet) as f64;
            match owner {
                Some(owner) if owner == player => {}
                Some(owner) => {
                    let Some(theirs) = others
                        .iter()
                        .find(|(p, _)| p == owner)
                        .and_then(|(_, m)| m.get(&system))
                    else {
                        continue;
                    };
                    if !has_ground {
                        continue;
                    }
                    let chance = if theirs.ground_defence.durability > 0.0 {
                        let mut attack = mine.ground_projection;
                        attack.cannon = if theirs.shielded {
                            0.0
                        } else {
                            mine.bombardment
                        };
                        win_probability(&attack, &theirs.ground_defence, GROUND_CALIBRATION)
                    } else {
                        1.0
                    };
                    take += worth * chance;
                }
                None => {
                    if has_ground {
                        take += worth;
                    }
                }
            }
        }
        found.insert(system, control * take);
    }
    found
}

/// How far a change can reach: the most hexes one activation can carry a ship, with every
/// movement bonus the engine knows (gravity drive, the Ionian card, gravity rifts) stacked.
pub const CHANGE_RADIUS: usize = 5;

/// What the cache did on its last call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Board unchanged: the stored summary was returned as it was.
    pub hits: u64,
    /// Some systems changed: only they and their surroundings were recomputed.
    pub partial: u64,
    /// Something seat-wide changed, or no state yet: everything was recomputed.
    pub full: u64,
    /// Systems whose projection was recomputed across all partial refreshes.
    pub systems_refreshed: u64,
    /// A tactical action was open: the value from before the activation was returned.
    pub frozen: u64,
}

/// One seat's power map, kept as state and refreshed only where the board changed.
///
/// Every system carries a fingerprint of what lies in it (units with damage, planet control,
/// command tokens). A seat-wide fingerprint covers what changes movement or activation anywhere
/// (technologies and their exhaustion, relics, token pools, fleet supply). On each call:
/// - nothing changed: the stored summary is returned;
/// - systems changed: every opponent's defence is refreshed in those systems, and this seat's
///   projection in every system within [`CHANGE_RADIUS`] of one of them, since only ships that
///   close could move in;
/// - the seat-wide fingerprint changed: everything is recomputed.
///
/// While a tactical action is open the stored value is returned unchanged. Mid-action the engine
/// holds ships in transit outside the board, so a reading there describes half a move; projection
/// is a property of the board between actions, and it is refreshed once the action has resolved.
#[derive(Debug, Clone, Default)]
pub struct PowerCache {
    player: Option<PlayerId>,
    seat_key: u64,
    system_keys: BTreeMap<SystemId, u64>,
    token_keys: BTreeMap<SystemId, u64>,
    own: BTreeMap<SystemId, SystemPower>,
    others: Vec<(PlayerId, BTreeMap<SystemId, SystemPower>)>,
    summary: Summary,
    pub stats: CacheStats,
}

fn hash_of<T: std::hash::Hash>(value: &T) -> u64 {
    use std::hash::Hasher as _;
    let mut hasher = std::hash::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn system_key(seen: &Observed<'_>, system: &SystemId) -> u64 {
    let state = seen.system(system);
    hash_of(&(
        &state.units,
        &state.command_tokens,
        &state.planet_control,
        &state.planet_units,
    ))
}

fn seat_key(seen: &Observed<'_>, player: &PlayerId) -> u64 {
    let Some(seat) = seen.seat(player) else {
        return 0;
    };
    // Every seat's token pools and supply decide can-activate and fleet supply caps; opponents'
    // faction modifiers are fixed for the game, so this seat's inventory is what can move.
    hash_of(&(
        seat.technologies,
        seat.exhausted_technologies,
        seat.relics,
        seat.exhausted_relics,
        seat.tactic_tokens,
        seat.fleet_tokens,
        seen.fleet_supply_limit(player),
        seen.players().len(),
        // Movement also reads state the observation does not expose -- laws, the per-activation
        // Gravity Drive flag, round-scoped effects -- and those change at phase boundaries, so a
        // new phase or round forces a full refresh.
        seen.round(),
        format!("{:?}", seen.phase()),
        // The Ionian Fuel Refinery's +1 is spent by exhausting Tempesta.
        seen.planet_is_ready(&ti4_model::id::PlanetId::new("tempesta")),
        // Structured diplomacy (ceasefires and the like) can close systems to movement.
        format!("{:?}", seen.public_diplomacy_deals().collect::<Vec<_>>()),
    ))
}

impl PowerCache {
    /// The summary for `player` in this position, refreshing only what changed since last time.
    pub fn summary(&mut self, seen: &Observed<'_>, player: &PlayerId) -> Summary {
        let Some(galaxy) = seen.galaxy() else {
            return Summary::default();
        };
        if seen.active_system().is_some() && self.player.as_ref() == Some(player) {
            self.stats.frozen += 1;
            return self.summary;
        }
        let ids: Vec<SystemId> = galaxy.system_ids().into_iter().map(SystemId::new).collect();
        let keys: BTreeMap<SystemId, u64> = ids
            .iter()
            .map(|id| (id.clone(), system_key(seen, id)))
            .collect();
        let seat = seat_key(seen, player);
        let tokens: BTreeMap<SystemId, u64> = ids
            .iter()
            .map(|id| (id.clone(), hash_of(&seen.system(id).command_tokens)))
            .collect();
        // Gravity Drive is spent per activation and renews with the next one, and every
        // activation places a command token: for a seat that owns it, any token placed or removed
        // anywhere can change what every ship can reach.
        let gravity = seen.seat(player).is_some_and(|s| {
            s.technologies
                .contains(&ti4_model::id::TechnologyId::new("gd"))
        });
        let activation_seen = gravity && tokens != self.token_keys;
        let opponents: Vec<PlayerId> = seen
            .players()
            .into_iter()
            .filter(|p| *p != player)
            .cloned()
            .collect();
        let same_seats = self.others.iter().map(|(p, _)| p).eq(opponents.iter());

        if self.player.as_ref() != Some(player)
            || self.seat_key != seat
            || !same_seats
            || activation_seen
        {
            self.token_keys = tokens;
            self.player = Some(player.clone());
            self.seat_key = seat;
            self.own = power_map(seen, player);
            self.others = opponents
                .iter()
                .map(|p| (p.clone(), defence_map(seen, p)))
                .collect();
            self.system_keys = keys;
            self.summary = summarize(seen, player, &self.own, &self.others);
            self.stats.full += 1;
            return self.summary;
        }
        let changed: std::collections::BTreeSet<SystemId> = keys
            .iter()
            .filter(|(id, key)| self.system_keys.get(*id) != Some(*key))
            .map(|(id, _)| id.clone())
            .collect();
        if changed.is_empty() {
            self.stats.hits += 1;
            return self.summary;
        }
        // Systems within reach of a change: breadth-first over the map's adjacency.
        let mut near = changed.clone();
        let mut frontier: Vec<SystemId> = changed.iter().cloned().collect();
        for _ in 0..CHANGE_RADIUS {
            let mut next = Vec::new();
            for id in &frontier {
                for n in galaxy.adjacent(id.as_str()) {
                    let n = SystemId::new(n);
                    if near.insert(n.clone()) {
                        next.push(n);
                    }
                }
            }
            frontier = next;
        }
        self.token_keys = tokens;
        self.own.extend(map_with(seen, player, true, Some(&near)));
        for (p, map) in &mut self.others {
            map.extend(map_with(seen, p, false, Some(&changed)));
        }
        self.system_keys = keys;
        self.summary = summarize(seen, player, &self.own, &self.others);
        self.stats.partial += 1;
        self.stats.systems_refreshed += near.len() as u64;
        self.summary
    }
}

impl PowerCache {
    /// Diagnostics: where the stored maps differ from a fresh computation of this position.
    #[must_use]
    pub fn differences(&self, seen: &Observed<'_>, player: &PlayerId) -> Vec<String> {
        let mut out = Vec::new();
        let fresh = power_map(seen, player);
        for (system, power) in &fresh {
            if self.own.get(system) != Some(power) {
                let key_now = system_key(seen, system);
                let key_then = self.system_keys.get(system).copied();
                let movers: Vec<String> = seen
                    .movable_into(player, system)
                    .iter()
                    .map(|m| {
                        let origin_key = if self.system_keys.get(&m.origin).copied()
                            == Some(system_key(seen, &m.origin))
                        {
                            "same"
                        } else {
                            "changed"
                        };
                        format!(
                            "{}@{}({origin_key},gd={},ion={})",
                            m.unit.type_id, m.origin, m.gravity_drive, m.ionian
                        )
                    })
                    .collect();
                out.push(format!("    movers now: {}", movers.join(" ")));
                out.push(format!(
                    "own {system}: key {} | cached sproj {:?} gproj {:?} | fresh sproj {:?} gproj {:?}",
                    if key_then == Some(key_now) { "same" } else { "changed" },
                    self.own.get(system).map(|p| (p.space_projection.firepower, p.arriving_ships)),
                    self.own.get(system).map(|p| p.ground_projection.durability),
                    (power.space_projection.firepower, power.arriving_ships),
                    power.ground_projection.durability,
                ));
            }
        }
        for (p, map) in &self.others {
            let now = defence_map(seen, p);
            for (system, power) in &now {
                if map.get(system) != Some(power) {
                    out.push(format!("defence {p} {system} differs"));
                }
            }
        }
        if seat_key(seen, player) != self.seat_key {
            out.push("seat key changed".to_owned());
        }
        out
    }
}
