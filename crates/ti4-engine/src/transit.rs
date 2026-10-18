//! Applying a move: cargo (LRR 95) and the gravity-rift roll (41.2).
//!
//! Ported from the oracle's `Game._load_cargo`, `_survives_gravity_rifts` and the body of
//! `_move_one`. [`crate::movement`] decides whether a move is *legal*; this decides what
//! actually happens when it is taken.
//!
//! The 41.2 destruction roll lives here rather than with the legality rules because it is a
//! consequence of moving, not a question about whether the move may be made.

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::{GameState, Phase};
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};
use crate::decision_context::{DecisionContext, DecisionSource, DecisionTarget};
use crate::dice::Dice;
use crate::movement::MovementRules;
use crate::preview::{Delta, Preview, Quantity};
use crate::rng::GameRng;

/// The choice kind for loading a unit into a ship's hold.
pub const LOAD_KIND: &str = "load";

/// A rift roll of this or less removes the ship from the board (41.2).
pub const RIFT_DESTROYS_ON: u32 = 3;

/// Where a carried unit came from, so a lost ship can put it back.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CargoSource {
    /// The space area of the origin system.
    Space,
    /// A planet in the origin system.
    Planet(PlanetId),
}

/// One unit in a ship's hold, paired with where it was picked up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cargo {
    pub unit: Unit,
    pub source: CargoSource,
    /// The system this unit was picked up from.
    ///
    /// 95.1 lets a ship load from the system it started in, every system it moves *through*, and
    /// the active system -- so cargo no longer all comes from one place, and whatever removes it
    /// has to know which system to take it out of.
    pub system: SystemId,
}

/// What happened to a ship that moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveOutcome {
    /// It arrived, with this many passengers.
    Arrived { cargo: Vec<Cargo> },
    /// 41.2: a gravity rift destroyed it, and 95.1b took its cargo with it.
    LostToGravityRift { cargo: Vec<Cargo> },
}

/// Every unit in `origin` this player could load, in a stable order.
///
/// Space area first, then planets in id order — the oracle's order, and the one that makes an
/// option list reproducible.
#[must_use]
pub fn loadable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    origin: &SystemId,
) -> Vec<Cargo> {
    loadable_by(state, content, sources, player, origin, false)
}

/// [`loadable`] for a ship that a module frees of 95.5 (`MovementHooks::ignores_command_tokens`,
/// Nomad hero Ahk-Syl Siven): units standing in a system with the player's command token may be
/// taken aboard.
fn loadable_by(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    origin: &SystemId,
    ignore_tokens: bool,
) -> Vec<Cargo> {
    let types = catalogue(content, sources);
    let consumes = |unit: &Unit| {
        types
            .get(unit.type_id.as_str())
            .is_some_and(UnitType::consumes_capacity)
    };
    let system = state.system_state(origin);
    if token_bars(state, player, origin, ignore_tokens) {
        return Vec::new();
    }

    // Miniaturization: a Ral Nel structure in the space area is cargo for any ship.
    let mut found: Vec<Cargo> = system
        .units_of(player)
        .into_iter()
        .filter(|unit| {
            consumes(unit)
                || crate::factions::ralnel::transportable_structure(state, content, sources, unit)
        })
        .map(|unit| Cargo {
            unit: unit.clone(),
            source: CargoSource::Space,
            system: origin.clone(),
        })
        .collect();
    for planet in system.planet_units.keys() {
        found.extend(
            system
                .on_planet_of(planet, player)
                .into_iter()
                .filter(|unit| consumes(unit))
                .map(|unit| Cargo {
                    unit: unit.clone(),
                    source: CargoSource::Planet(planet.clone()),
                    system: origin.clone(),
                }),
        );
    }
    found
}

/// 95.5: "Fighters and ground forces cannot be picked up from a system that contains one of
/// their faction's command tokens other than the active system."
///
/// Ordinarily unreachable for the origin, because 58.4c stops a ship leaving such a system at
/// all -- but the Dominus Orb suspends exactly that, and a ship freed to leave must still not
/// take the garrison with it. A system that a ship passes is barred the same way.
fn token_bars(
    state: &GameState,
    player: &PlayerId,
    system: &SystemId,
    ignore_tokens: bool,
) -> bool {
    !ignore_tokens
        && state.active_system.as_ref() != Some(system)
        && state.system_state(system).command_tokens.contains(player)
}

/// Why no ship of a movement loads a unit that uses capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Unloadable {
    /// The system has a command token of the seat: nothing is picked up there (95.5).
    CommandToken,
    /// The ships that start in the system or pass it move past other players' ships on the
    /// say-so of Myru Vos, and so transport nothing.
    UnladenRoute,
    /// No ship with capacity that moves starts in the system or passes it.
    NoCapacity,
}

/// What one ship's hold is offered, and the units of its route that it is not, with why.
struct Pickups {
    /// The hold's candidates, in the order of its options.
    offered: Vec<Cargo>,
    refused: Vec<(Cargo, Unloadable)>,
}

/// The pickups of one ship that moves along `path` from `origin` (95.1): the units of the
/// origin and of each system of the path, the active system too. [`CargoWindow::for_ship`]
/// offers [`Pickups::offered`], and [`unloadable`] reports [`Pickups::refused`]: one set of
/// checks for both.
fn pickups(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    origin: &SystemId,
    ship: &Unit,
    path: &[String],
) -> Pickups {
    let capacity = capacity_of(content, sources, ship);
    // 95.1: "During a tactical action, it can pick up and transport units from the active
    // system, the system it started its movement in, and each system it moves through." The
    // path carries the systems between the two, and 95.5 is applied per system, so a system
    // holding this player's command token contributes nothing.
    let ignore_tokens = crate::factions::hooks_movement::ignores_command_tokens(
        state,
        content,
        sources,
        player,
        ship.type_id.as_str(),
    );
    let mut offered = Vec::new();
    let mut refused = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for step in std::iter::once(origin.to_string()).chain(path.iter().cloned()) {
        if !seen.insert(step.clone()) {
            continue; // a route may revisit a system; its units are offered once
        }
        let system = SystemId::new(step);
        let found = loadable_by(state, content, sources, player, &system, true);
        if token_bars(state, player, &system, ignore_tokens) {
            refused.extend(
                found
                    .into_iter()
                    .map(|cargo| (cargo, Unloadable::CommandToken)),
            );
        } else {
            offered.extend(found);
        }
    }
    // Myru Vos: a ship moving through other players' ships on that agent's say-so carries
    // nothing ("if they are not transporting units").
    if crate::factions::firmament::unladen_route_forbids_cargo(
        state, content, sources, player, ship, path,
    ) {
        refused.extend(
            offered
                .drain(..)
                .map(|cargo| (cargo, Unloadable::UnladenRoute)),
        );
    }
    // A ship with no capacity (a destroyer) carries nothing that needs a slot, but its hold
    // stays open to cargo that never uses one (Ral Nel Miniaturization: structures).
    if capacity <= 0 {
        let types = catalogue(content, sources);
        let (slotted, unslotted): (Vec<Cargo>, Vec<Cargo>) =
            offered.into_iter().partition(|cargo| {
                types
                    .get(cargo.unit.type_id.as_str())
                    .is_some_and(UnitType::consumes_capacity)
            });
        offered = unslotted;
        refused.extend(
            slotted
                .into_iter()
                .map(|cargo| (cargo, Unloadable::NoCapacity)),
        );
    }
    Pickups { offered, refused }
}

/// One ship of a movement and the path that it moves along, as [`CargoWindow::for_ship`] takes
/// them: the path is the one of the move that the game offers it.
#[derive(Debug, Clone, Copy)]
pub struct Route<'a> {
    pub origin: &'a SystemId,
    pub ship: &'a Unit,
    pub path: &'a [String],
}

/// Units of one kind in one place that use capacity, that no ship of a movement loads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnloadedCargo {
    pub system: SystemId,
    pub source: CargoSource,
    pub unit: Unit,
    pub count: usize,
    pub reason: Unloadable,
}

/// The units of `player` that use capacity, in each system that a ship of `routes` starts in or
/// passes, outside `active`, that the hold of no ship is offered, with why: grouped by system,
/// place, type, damage and galvanization, in the order of the systems and of [`loadable`].
///
/// A unit that moves as a ship of `routes` itself (a fighter with a move of its own) is not
/// cargo. The reason is the one of a ship that passes the system, in the order of
/// [`Unloadable`]: a command token before Myru Vos before no capacity.
#[must_use]
pub fn unloadable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    active: &SystemId,
    routes: &[Route<'_>],
) -> Vec<UnloadedCargo> {
    let types = catalogue(content, sources);
    let mut found: Vec<UnloadedCargo> = Vec::new();
    let mut offered: Vec<Cargo> = Vec::new();
    let mut refused: Vec<(Cargo, Unloadable)> = Vec::new();
    for route in routes {
        let pickups = pickups(
            state,
            content,
            sources,
            player,
            route.origin,
            route.ship,
            route.path,
        );
        offered.extend(pickups.offered);
        refused.extend(pickups.refused);
    }
    let same =
        |a: &Cargo, b: &Cargo| a.system == b.system && a.source == b.source && a.unit == b.unit;
    for (cargo, reason) in refused {
        let uses_capacity = types
            .get(cargo.unit.type_id.as_str())
            .is_some_and(UnitType::consumes_capacity);
        if !uses_capacity
            || cargo.system == *active
            || offered.iter().any(|known| same(known, &cargo))
        {
            continue;
        }
        match found.iter_mut().find(|known| {
            known.system == cargo.system && known.source == cargo.source && known.unit == cargo.unit
        }) {
            Some(known) => known.reason = known.reason.min(reason),
            None => {
                // Every unit of the kind in that place, less the ones that move as ships.
                let all = loadable_by(state, content, sources, player, &cargo.system, true)
                    .iter()
                    .filter(|other| same(other, &cargo))
                    .count();
                let sailing = routes
                    .iter()
                    .filter(|route| {
                        *route.origin == cargo.system
                            && cargo.source == CargoSource::Space
                            && *route.ship == cargo.unit
                    })
                    .count();
                found.push(UnloadedCargo {
                    count: all.saturating_sub(sailing),
                    system: cargo.system,
                    source: cargo.source,
                    unit: cargo.unit,
                    reason,
                });
            }
        }
    }
    found.retain(|cargo| cargo.count > 0);
    found
}

/// The capacity of one ship.
#[must_use]
pub fn capacity_of(content: &ContentStore, sources: SourceSet, unit: &Unit) -> i64 {
    catalogue(content, sources)
        .get(unit.type_id.as_str())
        .map_or(0, UnitType::capacity)
}

/// A failure while loading a hold.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CargoError {
    #[error("the hold is full or closed")]
    Complete,
    #[error("option id {0:?} does not name a loadable unit")]
    UnknownCargo(String),
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

/// Filling one ship's hold before it moves (LRR 95).
///
/// Units are taken from the system the ship starts in — its space area or a planet there — and
/// from each system on its path, including the active system (95.1; see
/// [`CargoWindow::for_ship`]), up to the ship's capacity, preserving each candidate's pickup
/// system.
///
/// Candidates are tracked **by index, never by value**: units are plain data, so two infantry
/// compare equal, and filtering an "already taken" list by equality would silently make the
/// second one unloadable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoWindow {
    player: PlayerId,
    candidates: Vec<Cargo>,
    origin: Option<SystemId>,
    ship_type: Option<String>,
    ground: Vec<bool>,
    fighters: Vec<bool>,
    loaded: Vec<usize>,
    /// Per candidate: transporting it uses no capacity slot (`MovementHooks::free_cargo`). Empty
    /// (every unit pays) for holds built by [`Self::new`].
    free_cargo: Vec<bool>,
    capacity: i64,
    closed: bool,
    /// The phase and round this hold was opened in, for the typed context [`Self::pending_choice`]
    /// reports. Fixed at construction: loading answers no question about a later phase or round.
    phase: Phase,
    round: u32,
}

impl CargoWindow {
    /// Open a hold of `capacity` over everything loadable in the origin system.
    ///
    /// Test-only in practice — every real caller goes through [`Self::for_ship`], which is why
    /// `phase`/`round` default rather than take new required parameters here: a bare hold with no
    /// game position to read them from never reaches a real decider.
    #[must_use]
    pub const fn new(player: PlayerId, candidates: Vec<Cargo>, capacity: i64) -> Self {
        Self {
            player,
            candidates,
            origin: None,
            ship_type: None,
            ground: Vec::new(),
            fighters: Vec::new(),
            loaded: Vec::new(),
            free_cargo: Vec::new(),
            capacity,
            closed: capacity <= 0,
            phase: Phase::Action,
            round: 0,
        }
    }

    /// Open a hold for one ship, reading its capacity from the corpus.
    #[must_use]
    pub fn for_ship(
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        player: &PlayerId,
        origin: &SystemId,
        ship: &Unit,
        path: &[String],
    ) -> Self {
        let capacity = capacity_of(content, sources, ship);
        let candidates = pickups(state, content, sources, player, origin, ship, path).offered;
        let types = catalogue(content, sources);
        let ground = candidates
            .iter()
            .map(|cargo| {
                types
                    .get(cargo.unit.type_id.as_str())
                    .is_some_and(UnitType::is_ground_force)
            })
            .collect();
        let fighters = candidates
            .iter()
            .map(|cargo| {
                types
                    .get(cargo.unit.type_id.as_str())
                    .is_some_and(UnitType::is_fighter)
            })
            .collect();
        let free_cargo = if crate::factions::hooks_movement::any(|table| table.free_cargo.is_some())
        {
            candidates
                .iter()
                .map(|cargo| {
                    crate::factions::hooks_movement::free_cargo(
                        state,
                        content,
                        sources,
                        &cargo.unit,
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        let no_free_riders = !free_cargo.iter().any(|free| *free);
        Self {
            player: player.clone(),
            candidates,
            origin: Some(origin.clone()),
            ship_type: Some(ship.type_id.to_string()),
            ground,
            fighters,
            loaded: Vec::new(),
            free_cargo,
            capacity,
            closed: capacity <= 0 && no_free_riders,
            phase: state.phase,
            round: state.round,
        }
    }

    /// Whether candidate `index` rides without using a slot.
    fn rides_free(&self, index: usize) -> bool {
        self.free_cargo.get(index).copied().unwrap_or(false)
    }

    /// Slots still unused: capacity minus the loaded units that are not free cargo.
    fn slots_left(&self) -> i64 {
        let used = self
            .loaded
            .iter()
            .filter(|index| !self.rides_free(**index))
            .count();
        self.capacity - i64::try_from(used).unwrap_or(i64::MAX)
    }

    /// A full hold is still open to units that ride free (Argent Aerie Sentinel).
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.closed
            || self.loaded.len() >= self.candidates.len()
            || (self.slots_left() <= 0 && self.free().is_empty())
    }

    /// What has been loaded, in the order it was taken aboard.
    #[must_use]
    pub fn cargo(&self) -> Vec<Cargo> {
        self.loaded
            .iter()
            .map(|index| self.candidates[*index].clone())
            .collect()
    }

    /// Indices still loadable, in candidate order: not yet aboard, and either a slot is left or
    /// the unit rides free.
    fn free(&self) -> Vec<usize> {
        let room = self.slots_left() > 0;
        (0..self.candidates.len())
            .filter(|index| !self.loaded.contains(index) && (room || self.rides_free(*index)))
            .collect()
    }

    /// The next pickup choice, or `None` once the hold is closed or full.
    ///
    /// Interchangeable units — same type, condition, source, and pickup system — are offered once. Beyond
    /// tidiness this matters because a sampling decider draws per option, so a pickup written
    /// three times would carry three times the weight of an equally good one written once.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "one pass building every pickup option plus the typed context OBS-003d added"
    )]
    pub fn pending_choice(&self) -> Option<Choice> {
        if self.is_complete() {
            return None;
        }
        let free = self.free();
        if free.is_empty() {
            return None;
        }

        let mut seen = std::collections::BTreeSet::new();
        let mut options: Vec<ChoiceOption> = Vec::new();
        for index in &free {
            let cargo = &self.candidates[*index];
            let key = (
                cargo.unit.type_id.to_string(),
                cargo.unit.sustained_damage,
                cargo.unit.galvanized,
                cargo.source.clone(),
                cargo.system.clone(),
            );
            if !seen.insert(key) {
                continue;
            }
            let where_from = match &cargo.source {
                CargoSource::Space => "space".to_owned(),
                CargoSource::Planet(planet) => planet.to_string(),
            };
            let source = match &cargo.source {
                CargoSource::Space => serde_json::Value::Null,
                CargoSource::Planet(planet) => planet.to_string().into(),
            };
            // OBS-008a3: the hold's own remaining slots, before and after this one pickup.
            // `resolve` charges every accepted load exactly one slot regardless of the unit's
            // printed capacity cost (95.2's "capacity_remaining" bookkeeping counts loads, not
            // capacityUsed), so the preview states that same arithmetic rather than a corpus
            // lookup that could disagree with what accepting the option actually does.
            let capacity_remaining = self.slots_left();
            let slot_cost = i64::from(!self.rides_free(*index));
            let mut option = ChoiceOption::labelled(
                format!("load|{index}"),
                LOAD_KIND,
                format!("load {} from {where_from}", cargo.unit.type_id),
            )
            .with("unit", cargo.unit.type_id.to_string())
            .with("source", source)
            .with("pickup_system", cargo.system.to_string())
            .with("damaged", cargo.unit.sustained_damage)
            .with("galvanized", cargo.unit.galvanized)
            .with("capacity_remaining", capacity_remaining)
            .previewed(Preview::certain(vec![Delta::new(
                Quantity::CapacityFree,
                capacity_remaining,
                capacity_remaining - slot_cost,
            )]))
            .with(
                "loaded_ground",
                self.loaded
                    .iter()
                    .filter(|index| self.ground.get(**index) == Some(&true))
                    .count(),
            )
            .with(
                "loaded_fighters",
                self.loaded
                    .iter()
                    .filter(|index| self.fighters.get(**index) == Some(&true))
                    .count(),
            );
            if let Some(origin) = &self.origin {
                option = option.with("system", origin.to_string());
            }
            options.push(option);
        }
        let mut decline = ChoiceOption::labelled(
            "done_loading",
            crate::choice::DECLINE_KIND,
            "carry nothing further",
        )
        .with(
            "loaded_ground",
            self.loaded
                .iter()
                .filter(|index| self.ground.get(**index) == Some(&true))
                .count(),
        )
        .with(
            "loaded_fighters",
            self.loaded
                .iter()
                .filter(|index| self.fighters.get(**index) == Some(&true))
                .count(),
        )
        .with(
            "ground_available",
            free.iter()
                .filter(|index| self.ground.get(**index) == Some(&true))
                .count(),
        );
        if let Some(origin) = &self.origin {
            decline = decline.with("system", origin.to_string());
        }
        options.push(decline);
        let prompt = self.ship_type.as_ref().map_or_else(
            || "load which unit".to_owned(),
            |ship| format!("load {ship} ({} free)", self.slots_left()),
        );
        let mut context = DecisionContext::new(
            self.player.clone(),
            DecisionSource::Rule("95".to_owned()),
            "load_cargo",
            self.phase,
            self.round,
        );
        if let Some(origin) = &self.origin {
            context = context.about(DecisionTarget::System(origin.clone()));
        }
        Some(Choice::new(self.player.clone(), prompt, options).contextualized(context))
    }

    /// Take one unit aboard, or close the hold.
    ///
    /// # Errors
    /// [`CargoError::Complete`] when the hold is closed or full, [`CargoError::IllegalChoice`]
    /// when the answer was not offered, and [`CargoError::UnknownCargo`] when the option id
    /// does not name a candidate.
    pub fn resolve(&mut self, answer: ChoiceOption) -> Result<(), CargoError> {
        let choice = self.pending_choice().ok_or(CargoError::Complete)?;
        let option = validate(&choice, answer)?;
        if option.is_decline() {
            self.closed = true;
            return Ok(());
        }
        let index: usize = option
            .id
            .strip_prefix("load|")
            .and_then(|rest| rest.parse().ok())
            .filter(|index| *index < self.candidates.len())
            .ok_or_else(|| CargoError::UnknownCargo(option.id.clone()))?;
        self.loaded.push(index);
        Ok(())
    }
}

/// The systems a route *exits* that are gravity rifts.
///
/// The destination is never exited, so it never rolls — 41.2 speaks of moving *out of* a rift.
#[must_use]
pub fn rifts_exited(rules: &MovementRules<'_>, path: &[String]) -> Vec<String> {
    if path.is_empty() {
        return Vec::new();
    }
    path[..path.len() - 1]
        .iter()
        .filter(|system| rules.is_rift(system) && !rules.rift_roll_exempt.contains(*system))
        .cloned()
        .collect()
}

/// 41.2: one die per rift exited; `1`–`3` removes the ship from the board.
///
/// Nav Suite ignores the effect of anomalies, and being destroyed by a rift is one of those
/// effects — which is why `anomalies_ignored` is honoured here as well as in the legality
/// rules. Honouring it in only one of the two makes the card half work.
pub fn survives_gravity_rifts(
    dice: &mut Dice,
    rng: &mut GameRng,
    rules: &MovementRules<'_>,
    path: &[String],
) -> bool {
    if rules.anomalies_ignored || rules.rifts_ignored {
        return true;
    }
    for system in rifts_exited(rules, path) {
        let roll = dice.roll(rng, 1, "gravity rift", Some(RIFT_DESTROYS_ON + 1));
        let destroyed = roll
            .faces
            .first()
            .is_some_and(|face| *face <= RIFT_DESTROYS_ON);
        // For the public roll feed: which rift, and what the die did to the ship.
        dice.annotate_last(
            Some(&system),
            Some(if destroyed {
                "ship destroyed"
            } else {
                "ship survives"
            }),
        );
        if destroyed {
            return false;
        }
    }
    true
}

/// Move a ship and its cargo, or lose both to a rift.
///
/// # Errors
/// This cannot fail: an illegal move is refused before it reaches here, by
/// [`MovementRules`]. It returns what happened so a caller can announce it — including the
/// passengers by name, because a count cannot be acted on. A table told only that a ship was
/// lost cannot find the piece to take off, and troops that went down with it stay standing.
pub fn apply_move(
    state: &mut GameState,
    origin: &SystemId,
    destination: &SystemId,
    ship: &Unit,
    cargo: Vec<Cargo>,
    survives: bool,
) -> MoveOutcome {
    if survives {
        state.move_units(origin, destination, std::slice::from_ref(ship));
        for carried in &cargo {
            take_aboard(state, origin, destination, carried);
        }
        MoveOutcome::Arrived { cargo }
    } else {
        state.destroy_units(origin, std::slice::from_ref(ship));
        // 95.1b: whatever it was carrying goes down with it.
        for carried in &cargo {
            let system = state.system_mut(&carried.system);
            match &carried.source {
                CargoSource::Space => system.remove(std::slice::from_ref(&carried.unit)),
                CargoSource::Planet(planet) => {
                    system.remove_from_planet(planet, std::slice::from_ref(&carried.unit));
                }
            }
            crate::faction_techs::note_destroyed(state, &carried.unit);
        }
        MoveOutcome::LostToGravityRift { cargo }
    }
}

/// Lift one passenger out of the origin — space or planet — and into the destination's space.
fn take_aboard(state: &mut GameState, origin: &SystemId, destination: &SystemId, carried: &Cargo) {
    let unit = carried.unit.clone();
    // The cargo's *own* system, not the ship's origin: 95.1 lets a ship pick up en route, so a
    // passenger may have come from any system on the path.
    let from = &carried.system;
    let _ = origin;
    match &carried.source {
        CargoSource::Space => {
            state.move_units(from, destination, std::slice::from_ref(&unit));
        }
        CargoSource::Planet(planet) => {
            state
                .system_mut(from)
                .remove_from_planet(planet, std::slice::from_ref(&unit));
            // Ground forces arrive in the space area aboard their ship; landing is invasion,
            // a separate step, so they must not be dropped straight onto a planet here.
            state.system_mut(destination).units.push(unit);
        }
    }
}

// -- movement outside a tactical action ----------------------------------------------------------

/// The typed event announcing ships moved by an effect rather than by a tactical action.
///
/// Payload: `player`, `from`, `to`, `count` (int), `ships` (array of unit type ids, one per ship
/// moved) and `reason` (the effect's label, e.g. `"foresight"`). Emitted by
/// [`announce_relocation`] **after** the move, so "after another player moves ships into a
/// system" windows see the ships already there. Distinct from `SHIP_MOVED`, which only the
/// tactical action emits and which carries `player` alone.
pub const SHIPS_RELOCATED: &str = "SHIPS_RELOCATED";

/// A set of one player's ships to move out of turn.
#[derive(Debug, Clone, Copy)]
pub struct Relocation<'a> {
    /// Whose ships.
    pub player: &'a PlayerId,
    /// Where they are now (the space area).
    pub from: &'a SystemId,
    /// Where they go.
    pub to: &'a SystemId,
    /// The ships, as they stand (damage included). Interchangeable copies may repeat.
    pub ships: &'a [Unit],
    /// Require `to` to be adjacent to `from` for this player (`PlayerAdjacency`, so Quantum
    /// Entanglement and the like count).
    pub require_adjacent: bool,
    /// Refuse when `to` holds another player's ships ("an adjacent system that does not contain
    /// another player's ships").
    pub forbid_foreign_ships_at_destination: bool,
    /// Refuse when the arrival would leave the player over fleet supply (37) or over capacity
    /// (16.3) in `to`. Without it the effect resolves and 37.3 removes the excess at the end of
    /// the turn, as for any arrival.
    pub refuse_fleet_overflow: bool,
    /// The effect's label, carried into the event.
    pub reason: &'a str,
}

/// What a relocation did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relocated {
    pub player: PlayerId,
    pub from: SystemId,
    pub to: SystemId,
    /// Type ids of the ships moved, in the order given.
    pub ships: Vec<String>,
    /// Type ids of the ground forces and fighters carried along, in the order given (empty for a
    /// plain relocation).
    pub cargo: Vec<String>,
    pub reason: String,
    /// Fleet headroom in `to` after the move (negative: ships 37.3 will remove).
    pub fleet_headroom: i64,
    /// Capacity-consuming units that cannot legally stay in `to` (16.3).
    pub capacity_excess: i64,
}

impl Relocated {
    /// The payload of [`SHIPS_RELOCATED`].
    #[must_use]
    pub fn payload(&self) -> std::collections::BTreeMap<String, serde_json::Value> {
        let mut payload = std::collections::BTreeMap::new();
        payload.insert("player".to_owned(), self.player.to_string().into());
        payload.insert("from".to_owned(), self.from.to_string().into());
        payload.insert("to".to_owned(), self.to.to_string().into());
        payload.insert(
            "count".to_owned(),
            i64::try_from(self.ships.len()).unwrap_or(i64::MAX).into(),
        );
        payload.insert(
            "ships".to_owned(),
            self.ships
                .iter()
                .map(|ship| serde_json::Value::String(ship.clone()))
                .collect::<Vec<_>>()
                .into(),
        );
        payload.insert("reason".to_owned(), self.reason.clone().into());
        if !self.cargo.is_empty() {
            payload.insert(
                "cargo".to_owned(),
                self.cargo
                    .iter()
                    .map(|unit| serde_json::Value::String(unit.clone()))
                    .collect::<Vec<_>>()
                    .into(),
            );
        }
        payload
    }
}

/// Why ships could not be relocated. Nothing has changed when one of these is returned (except
/// [`RelocateError::Timing`], which arises after the move; see [`announce_relocation`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RelocateError {
    #[error("no ships to move")]
    NothingToMove,
    #[error("the ships are already in {0}")]
    SameSystem(SystemId),
    #[error("{0} is not a ship")]
    NotAShip(String),
    #[error("player {player} does not have those ships in {system}")]
    NotThere { player: PlayerId, system: SystemId },
    #[error("system {0} is not on the map")]
    NotOnTheMap(SystemId),
    #[error("{to} is not adjacent to {from}")]
    NotAdjacent { from: SystemId, to: SystemId },
    #[error("{0} holds another player's ships")]
    ForeignShips(SystemId),
    #[error("ships cannot enter {0} (an anomaly or law bars it)")]
    CannotEnter(SystemId),
    #[error("the arrival would put the fleet over its supply or capacity in {0}")]
    FleetOverflow(SystemId),
    #[error("{0} cannot be carried (only the player's own ground forces and fighters can)")]
    NotCargo(String),
    #[error("the cargo is not where the ships are (or is not the player's)")]
    CargoNotThere,
    #[error("the cargo needs more capacity than the ships have")]
    OverCapacity,
    #[error("95.5: cargo cannot be picked up from {0}, which holds the player's command token")]
    CargoUnderCommandToken(SystemId),
    #[error(transparent)]
    Timing(#[from] crate::timing::TimingError),
}

/// Move a set of one player's ships from one system to another **outside a tactical action**
/// (Naalu Foresight: "move your ships from the active system into that system"). No route, move
/// value, rift roll or cargo: the ships are lifted and set down. Ground forces and fighters aboard
/// do not go unless they are in `ships`.
///
/// Everything is checked before anything changes, so a refusal leaves `state` untouched. Does not
/// announce; see [`announce_relocation`], which a caller with a `Resolving` runs next. (A faction
/// hook inside a timing window has no resolver to open a nested window with: it performs the move
/// and reports the [`Relocated`] to its caller.)
///
/// # Errors
/// [`RelocateError`] naming the first failed check.
pub fn relocate_ships(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    relocation: &Relocation<'_>,
) -> Result<Relocated, RelocateError> {
    relocate_ships_with_cargo(state, content, sources, galaxy, relocation, &[])
}

/// [`relocate_ships`] that also carries ground forces and fighters, up to the ships' capacity
/// (Argent Flock Migration: "This can transport ground forces and fighters up to capacity, but
/// cannot land them").
///
/// `cargo` is read from [`loadable`] for `relocation.from` (each [`Cargo`] names the unit, whether
/// it stands in space or on a planet, and `system == from`); the units are checked against what is
/// actually there, copies counted, and against the capacity of the relocated ships: each unit
/// uses one slot unless a module rides it free (`MovementHooks::free_cargo`). Cargo set down in
/// the destination's space area, never on a planet. 95.5 applies: nothing is picked up from a
/// system holding the player's command token other than the active system. Everything is checked
/// before anything changes, so a refusal leaves `state` untouched. With no cargo this is exactly
/// [`relocate_ships`].
///
/// # Errors
/// [`RelocateError`] naming the first failed check.
#[expect(clippy::too_many_lines, reason = "one pass of checks, then the move")]
pub fn relocate_ships_with_cargo(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    relocation: &Relocation<'_>,
    cargo: &[Cargo],
) -> Result<Relocated, RelocateError> {
    let Relocation {
        player, from, to, ..
    } = *relocation;
    if relocation.ships.is_empty() {
        return Err(RelocateError::NothingToMove);
    }
    if from == to {
        return Err(RelocateError::SameSystem(from.clone()));
    }
    let types = catalogue(content, sources);
    for ship in relocation.ships {
        if !types
            .get(ship.type_id.as_str())
            .is_some_and(UnitType::is_ship)
        {
            return Err(RelocateError::NotAShip(ship.type_id.to_string()));
        }
        if &ship.owner != player {
            return Err(RelocateError::NotThere {
                player: player.clone(),
                system: from.clone(),
            });
        }
    }
    // The ships must all be there, copies counted: take them from a scratch copy of the area.
    let mut standing = state
        .board
        .get(from)
        .map(|system| system.units.clone())
        .unwrap_or_default();
    for ship in relocation.ships {
        let Some(index) = standing.iter().position(|held| held == ship) else {
            return Err(RelocateError::NotThere {
                player: player.clone(),
                system: from.clone(),
            });
        };
        standing.remove(index);
    }
    // The cargo must be the player's own capacity-using units, standing where the ships do, and
    // must fit: copies counted against what remains after the ships themselves are lifted.
    if !cargo.is_empty() {
        let mut planets = state
            .board
            .get(from)
            .map(|system| system.planet_units.clone())
            .unwrap_or_default();
        if state.active_system.as_ref() != Some(from)
            && state
                .board
                .get(from)
                .is_some_and(|system| system.command_tokens.contains(player))
        {
            return Err(RelocateError::CargoUnderCommandToken(from.clone()));
        }
        let mut paying = 0_i64;
        for carried in cargo {
            if !types
                .get(carried.unit.type_id.as_str())
                .is_some_and(UnitType::consumes_capacity)
            {
                return Err(RelocateError::NotCargo(carried.unit.type_id.to_string()));
            }
            let pool = match &carried.source {
                CargoSource::Space => Some(&mut standing),
                CargoSource::Planet(planet) => planets.get_mut(planet),
            };
            let found = pool.and_then(|units| {
                units
                    .iter()
                    .position(|held| *held == carried.unit)
                    .map(|index| units.remove(index))
            });
            if &carried.system != from || &carried.unit.owner != player || found.is_none() {
                return Err(RelocateError::CargoNotThere);
            }
            if !crate::factions::hooks_movement::free_cargo(state, content, sources, &carried.unit)
            {
                paying += 1;
            }
        }
        let room: i64 = relocation
            .ships
            .iter()
            .map(|ship| capacity_of(content, sources, ship))
            .sum();
        if paying > room {
            return Err(RelocateError::OverCapacity);
        }
    }
    for system in [from, to] {
        if galaxy.coord_of(system.as_str()).is_none()
            && !galaxy.wormhole_systems().contains(&system.as_str())
        {
            return Err(RelocateError::NotOnTheMap(system.clone()));
        }
    }
    if relocation.require_adjacent
        && !crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, player)
            .are_adjacent(from.as_str(), to.as_str())
    {
        return Err(RelocateError::NotAdjacent {
            from: from.clone(),
            to: to.clone(),
        });
    }
    // 86.1, 11.1, 59.1: the destination must be enterable under the same rules a tactical move
    // uses for this player (laws, technologies, faction effects).
    let mut rules = crate::movement::MovementRules::with_laws(
        galaxy,
        content,
        sources,
        to.as_str(),
        crate::movement::Board::for_player(state, content, sources, player),
        Some(state),
    );
    crate::action_cards::apply_movement_effects(&mut rules, state, player);
    if !rules.can_enter(to.as_str()) {
        return Err(RelocateError::CannotEnter(to.clone()));
    }
    if relocation.forbid_foreign_ships_at_destination
        && state.board.get(to).is_some_and(|system| {
            system.units.iter().any(|unit| {
                &unit.owner != player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(UnitType::is_ship)
            })
        })
    {
        return Err(RelocateError::ForeignShips(to.clone()));
    }

    let mut after = state.clone();
    after.move_units(from, to, relocation.ships);
    for carried in cargo {
        take_aboard(&mut after, from, to, carried);
    }
    let standing = crate::fleet::standing_using(&types, &after, content, player, to, None);
    if relocation.refuse_fleet_overflow
        && (standing.fleet_headroom() < 0 || standing.capacity_excess > 0)
    {
        return Err(RelocateError::FleetOverflow(to.clone()));
    }
    *state = after;
    Ok(Relocated {
        player: player.clone(),
        from: from.clone(),
        to: to.clone(),
        ships: relocation
            .ships
            .iter()
            .map(|ship| ship.type_id.to_string())
            .collect(),
        cargo: cargo
            .iter()
            .map(|carried| carried.unit.type_id.to_string())
            .collect(),
        reason: relocation.reason.to_owned(),
        fleet_headroom: standing.fleet_headroom(),
        capacity_excess: standing.capacity_excess,
    })
}

/// Announce a completed relocation as [`SHIPS_RELOCATED`] through the timing windows.
///
/// Returns whether the event stood (a WHEN cancel is reported, but the ships have already moved:
/// like the other off-turn emitters, the move is not undone by a cancel). Without a resolver in
/// `ctx` nothing can react and `Ok(true)` is returned.
///
/// # Errors
/// [`RelocateError::Timing`] for an illegal decider answer or an exhausted event id space; the
/// ships have still moved.
pub fn announce_relocation(
    state: &mut GameState,
    ctx: &mut crate::choice::Resolving<'_>,
    relocated: &Relocated,
) -> Result<bool, RelocateError> {
    Ok(ctx.emit(state, SHIPS_RELOCATED, relocated.payload())?)
}

#[cfg(test)]
mod tests {

    /// 95.1: a ship picks up from every system it moves *through*, not only where it started.
    ///
    /// This engine offered the origin alone, which is narrower than the rules and comes up often --
    /// a carrier passing a garrison could not collect it. Cargo now carries the system it came
    /// from, because `apply_move` has to take each passenger out of the right place.
    #[test]
    fn a_ship_picks_up_from_systems_it_passes_through() {
        let (mut state, origin, midpoint) = state_with_two_systems();
        state.board.entry(midpoint.clone()).or_default();
        state.system_mut(&midpoint).units.push(unit("infantry"));

        let ship = unit("carrier");
        state.system_mut(&origin).units.push(ship.clone());

        let hold = CargoWindow::for_ship(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &origin,
            &ship,
            &[midpoint.to_string()],
        );
        let offered: Vec<&SystemId> = hold.candidates.iter().map(|cargo| &cargo.system).collect();
        assert!(
            offered.iter().any(|system| **system == midpoint),
            "the infantry on the route is loadable: {offered:?}"
        );
    }

    /// OBS-003d: the load-cargo choice names its rule and the system it is asked in, taken from
    /// the state the hold was opened with rather than recomputed per question.
    #[test]
    fn obs003d_load_cargo_carries_its_typed_context() {
        let (mut state, origin, _) = state_with_two_systems();
        state.system_mut(&origin).units.push(unit("infantry"));
        let ship = unit("carrier");
        state.system_mut(&origin).units.push(ship.clone());

        let hold = CargoWindow::for_ship(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &origin,
            &ship,
            &[],
        );
        let choice = hold.pending_choice().expect("a pickup is offered");
        let context = choice.context.as_ref().expect("typed context");
        assert_eq!(context.source, DecisionSource::Rule("95".to_owned()));
        assert_eq!(context.subtype, "load_cargo");
        assert_eq!(context.target, Some(DecisionTarget::System(origin)));
    }

    /// OBS-008a3: each pickup option previews the hold's own remaining slots falling by exactly
    /// one, matching what `resolve` actually charges; the "carry nothing further" option has no
    /// preview; and the previewed `after` is what the next pickup choice's `before` actually is.
    #[test]
    fn obs008a3_load_options_preview_the_holds_own_capacity_falling_by_one() {
        let (mut state, origin, _) = state_with_two_systems();
        state.system_mut(&origin).units.push(unit("infantry"));
        state.system_mut(&origin).units.push(unit("fighter"));
        let ship = unit("carrier"); // capacity 4
        state.system_mut(&origin).units.push(ship.clone());

        let mut hold = CargoWindow::for_ship(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &origin,
            &ship,
            &[],
        );
        let choice = hold.pending_choice().expect("a pickup is offered");
        let mut saw_pickup = false;
        for option in &choice.options {
            if option.is_decline() {
                assert!(
                    option.preview.is_none(),
                    "declining to load anything has no preview"
                );
                continue;
            }
            saw_pickup = true;
            match &option
                .preview
                .as_ref()
                .expect("a pickup is previewed")
                .outcome
            {
                crate::preview::Outcome::Certain { deltas } => {
                    assert_eq!(
                        deltas,
                        &[Delta::new(Quantity::CapacityFree, 4, 3)],
                        "a fresh hold of capacity 4 previews 4 -> 3 for any first pickup"
                    );
                }
                other => panic!("a pickup preview is certain, got {other:?}"),
            }
        }
        assert!(saw_pickup, "the hold offers at least one pickup");

        // The previewed `after` is what `resolve` actually leaves: the next choice's `before`.
        let taken = choice
            .options
            .iter()
            .find(|option| !option.is_decline())
            .unwrap()
            .clone();
        hold.resolve(taken).unwrap();
        let next = hold.pending_choice().expect("capacity remains");
        let next_pickup = next
            .options
            .iter()
            .find(|option| !option.is_decline())
            .expect("a second pickup is still offered");
        match &next_pickup.preview.as_ref().unwrap().outcome {
            crate::preview::Outcome::Certain { deltas } => {
                assert_eq!(deltas, &[Delta::new(Quantity::CapacityFree, 3, 2)]);
            }
            other => panic!("a pickup preview is certain, got {other:?}"),
        }
    }

    /// A passenger taken aboard en route leaves the system it was standing in, not the origin.
    #[test]
    fn cargo_is_removed_from_the_system_it_was_picked_up_in() {
        let (mut state, origin, midpoint) = state_with_two_systems();
        let destination = SystemId::new(crate::fixtures::plain_systems(3)[2].clone());
        state.board.entry(midpoint.clone()).or_default();
        state.board.entry(destination.clone()).or_default();

        let ship = unit("carrier");
        let troops = unit("infantry");
        state.system_mut(&origin).units.push(ship.clone());
        state.system_mut(&midpoint).units.push(troops.clone());

        let cargo = vec![Cargo {
            unit: troops.clone(),
            source: CargoSource::Space,
            system: midpoint.clone(),
        }];
        apply_move(&mut state, &origin, &destination, &ship, cargo, true);

        assert!(
            state.system_state(&midpoint).units.is_empty(),
            "the passenger left the system it was picked up in"
        );
        assert!(
            state.system_state(&destination).units.contains(&troops),
            "and arrived with the ship"
        );
    }

    /// 95.5: nothing is picked up from a system holding your own command token.
    ///
    /// 58.4c usually makes this moot by stopping the ship leaving at all. The Dominus Orb suspends
    /// that, and the two rules are separate: a ship freed to leave still may not take the garrison
    /// with it. The active system is exempt, which is the other half of the rule.
    #[test]
    fn a_command_token_bars_pickup_unless_it_is_the_active_system() {
        let (mut state, origin, _) = state_with_two_systems();
        state.system_mut(&origin).units.push(unit("infantry"));
        state.active_system = Some(SystemId::new("somewhere_else"));

        assert!(
            !loadable(&state, ContentStore::embedded(), POK, &player(), &origin).is_empty(),
            "with no token there, the infantry is loadable"
        );

        state.system_mut(&origin).place_token(player());
        assert!(
            loadable(&state, ContentStore::embedded(), POK, &player(), &origin).is_empty(),
            "your own command token bars the pickup"
        );

        state.active_system = Some(origin.clone());
        assert!(
            !loadable(&state, ContentStore::embedded(), POK, &player(), &origin).is_empty(),
            "except in the active system, where the token is yours from activating it"
        );
    }

    /// Three plain systems: a ship starts in the first, passes the second, and the third is
    /// active.
    fn route_state() -> (GameState, SystemId, SystemId, SystemId) {
        let (mut state, origin, midpoint) = state_with_two_systems();
        let active = SystemId::new(plain_systems(3)[2].clone());
        state.active_system = Some(active.clone());
        (state, origin, midpoint, active)
    }

    fn unloaded(
        state: &GameState,
        active: &SystemId,
        routes: &[(SystemId, Unit, Vec<String>)],
    ) -> Vec<UnloadedCargo> {
        let routes: Vec<Route<'_>> = routes
            .iter()
            .map(|(origin, ship, path)| Route { origin, ship, path })
            .collect();
        unloadable(
            state,
            ContentStore::embedded(),
            POK,
            &player(),
            active,
            &routes,
        )
    }

    /// 95.5: the infantry of a system with the seat's command token stay, and the engine says
    /// that the token is why; the infantry of the origin are loaded.
    #[test]
    fn a_command_token_is_why_a_passed_system_loads_nothing() {
        let (mut state, origin, midpoint, active) = route_state();
        let carrier = unit("carrier");
        state.system_mut(&origin).units.push(carrier.clone());
        state.system_mut(&origin).units.push(unit("infantry"));
        for _ in 0..2 {
            state.system_mut(&midpoint).units.push(unit("infantry"));
        }
        state.system_mut(&midpoint).place_token(player());
        let path = vec![origin.to_string(), midpoint.to_string(), active.to_string()];

        let found = unloaded(&state, &active, &[(origin, carrier, path)]);

        assert_eq!(
            found,
            vec![UnloadedCargo {
                system: midpoint,
                source: CargoSource::Space,
                unit: unit("infantry"),
                count: 2,
                reason: Unloadable::CommandToken,
            }]
        );
    }

    /// A ship with no capacity loads nothing that uses a slot: the fighters of its origin and the
    /// infantry of a planet that it passes stay, for want of capacity. A fighter that moves as a
    /// ship of its own is not cargo, and the units of the active system are not counted.
    #[test]
    fn a_ship_with_no_capacity_is_why_a_system_loads_nothing() {
        let (mut state, origin, midpoint, active) = route_state();
        let destroyer = unit("destroyer");
        let fighter = unit("fighter");
        state.system_mut(&origin).units.push(destroyer.clone());
        for _ in 0..3 {
            state.system_mut(&origin).units.push(fighter.clone());
        }
        let planet = PlanetId::new("somewhere");
        state
            .system_mut(&midpoint)
            .planet_units
            .insert(planet.clone(), vec![unit("infantry")]);
        state.system_mut(&active).units.push(unit("infantry"));
        let path = vec![origin.to_string(), midpoint.to_string(), active.to_string()];

        let found = unloaded(
            &state,
            &active,
            &[
                (origin.clone(), destroyer, path.clone()),
                (origin.clone(), fighter.clone(), path),
            ],
        );

        assert_eq!(
            found,
            vec![
                UnloadedCargo {
                    system: origin,
                    source: CargoSource::Space,
                    unit: fighter,
                    count: 2,
                    reason: Unloadable::NoCapacity,
                },
                UnloadedCargo {
                    system: midpoint,
                    source: CargoSource::Planet(planet),
                    unit: unit("infantry"),
                    count: 1,
                    reason: Unloadable::NoCapacity,
                },
            ]
        );
    }

    /// A unit that one ship is offered is not unloadable, even when another ship cannot load it.
    #[test]
    fn a_unit_that_one_ship_loads_is_not_unloadable() {
        let (mut state, origin, _, active) = route_state();
        let carrier = unit("carrier");
        let destroyer = unit("destroyer");
        state.system_mut(&origin).units.push(carrier.clone());
        state.system_mut(&origin).units.push(destroyer.clone());
        state.system_mut(&origin).units.push(unit("infantry"));
        let path = vec![origin.to_string(), active.to_string()];

        let found = unloaded(
            &state,
            &active,
            &[
                (origin.clone(), destroyer, path.clone()),
                (origin, carrier, path),
            ],
        );

        assert_eq!(found, Vec::new());
    }

    /// The report and the load offers are one set of checks: a unit that uses capacity, in a
    /// system that a route starts in or passes outside the active system, is either in the hold
    /// of a ship (`CargoWindow::for_ship`), a ship of a route, or unloadable, and never two of
    /// them.
    #[test]
    fn what_no_ship_loads_is_what_no_hold_is_offered() {
        let (mut state, origin, midpoint, active) = route_state();
        let carrier = unit("carrier");
        let destroyer = unit("destroyer");
        let fighter = unit("fighter");
        let planet = PlanetId::new("somewhere");
        {
            let start = state.system_mut(&origin);
            start
                .units
                .extend([carrier.clone(), destroyer.clone(), fighter.clone()]);
            start
                .units
                .extend([unit("infantry"), unit("fighter").sustained()]);
            start
                .planet_units
                .insert(planet.clone(), vec![unit("infantry"), unit("mech")]);
        }
        {
            let passed = state.system_mut(&midpoint);
            passed.units.extend([unit("infantry"), unit("fighter")]);
            passed.planet_units.insert(planet, vec![unit("infantry")]);
        }
        let types = catalogue(ContentStore::embedded(), POK);
        for token in [false, true] {
            if token {
                state.system_mut(&midpoint).place_token(player());
            }
            let short = vec![origin.to_string(), active.to_string()];
            let long = vec![origin.to_string(), midpoint.to_string(), active.to_string()];
            let routes = vec![
                (origin.clone(), carrier.clone(), short),
                (origin.clone(), destroyer.clone(), long.clone()),
                (origin.clone(), fighter.clone(), long),
            ];
            let found = unloaded(&state, &active, &routes);
            let held: Vec<Cargo> = routes
                .iter()
                .flat_map(|(origin, ship, path)| {
                    CargoWindow::for_ship(
                        &state,
                        ContentStore::embedded(),
                        POK,
                        &player(),
                        origin,
                        ship,
                        path,
                    )
                    .candidates
                })
                .collect();
            for system in [&origin, &midpoint] {
                let everything = loadable_by(
                    &state,
                    ContentStore::embedded(),
                    POK,
                    &player(),
                    system,
                    true,
                );
                let mut kinds: Vec<(CargoSource, Unit)> = Vec::new();
                for cargo in everything.iter().filter(|cargo| {
                    types
                        .get(cargo.unit.type_id.as_str())
                        .is_some_and(UnitType::consumes_capacity)
                }) {
                    let kind = (cargo.source.clone(), cargo.unit.clone());
                    if !kinds.contains(&kind) {
                        kinds.push(kind);
                    }
                }
                for (source, kind) in kinds {
                    let same = |cargo: &Cargo| {
                        cargo.system == *system && cargo.source == source && cargo.unit == kind
                    };
                    let total = everything.iter().filter(|cargo| same(cargo)).count();
                    let offered = held.iter().any(same);
                    let sailing = routes
                        .iter()
                        .filter(|(start, ship, _)| {
                            start == system && source == CargoSource::Space && *ship == kind
                        })
                        .count();
                    let reported: usize = found
                        .iter()
                        .filter(|cargo| {
                            cargo.system == *system && cargo.source == source && cargo.unit == kind
                        })
                        .map(|cargo| cargo.count)
                        .sum();
                    if offered {
                        assert_eq!(reported, 0, "{kind:?} in {system} is offered and reported");
                    } else {
                        assert_eq!(
                            reported,
                            total - sailing,
                            "{kind:?} in {system}: not offered, so every one that does not sail \
                             is reported"
                        );
                    }
                }
            }
            let at_midpoint: Vec<Unloadable> = found
                .iter()
                .filter(|cargo| cargo.system == midpoint)
                .map(|cargo| cargo.reason)
                .collect();
            let expected = if token {
                Unloadable::CommandToken
            } else {
                Unloadable::NoCapacity
            };
            assert!(!at_midpoint.is_empty(), "the passed system keeps its units");
            assert!(
                at_midpoint.iter().all(|reason| *reason == expected),
                "{at_midpoint:?}"
            );
        }
    }

    use ti4_content::galaxy::Galaxy;
    use ti4_model::content_types::POK;
    use ti4_model::id::UnitTypeId;

    use super::*;
    use crate::movement::Board;
    use crate::setup::start_game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn plain_systems(count: usize) -> Vec<String> {
        ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
            .iter()
            .filter(|(_, system)| !system.is_anomaly() && !system.is_hyperlane())
            .map(|(id, _)| (*id).to_owned())
            .take(count)
            .collect()
    }

    fn state_with_two_systems() -> (GameState, SystemId, SystemId) {
        let players = [player(), PlayerId::new("b")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let ids = plain_systems(2);
        (
            state,
            SystemId::new(ids[0].clone()),
            SystemId::new(ids[1].clone()),
        )
    }

    fn unit(kind: &str) -> Unit {
        Unit::new(UnitTypeId::new(kind), player())
    }

    #[test]
    fn a_carrier_has_capacity_and_a_destroyer_does_not() {
        assert!(capacity_of(ContentStore::embedded(), POK, &unit("carrier")) > 0);
        assert_eq!(
            capacity_of(ContentStore::embedded(), POK, &unit("destroyer")),
            0
        );
    }

    #[test]
    fn a_ship_with_no_capacity_carries_nothing() {
        let window = CargoWindow::new(player(), Vec::new(), 0);
        assert!(window.is_complete());
        assert!(window.pending_choice().is_none());
    }

    #[test]
    fn only_units_that_consume_capacity_can_be_loaded() {
        let (mut state, origin, _) = state_with_two_systems();
        let system = state.system_mut(&origin);
        system.units.push(unit("infantry"));
        system.units.push(unit("fighter"));
        system.units.push(unit("carrier")); // a ship, not cargo

        let found = loadable(&state, ContentStore::embedded(), POK, &player(), &origin);
        let kinds: Vec<String> = found
            .iter()
            .map(|cargo| cargo.unit.type_id.to_string())
            .collect();

        assert!(kinds.contains(&"infantry".to_owned()));
        assert!(kinds.contains(&"fighter".to_owned()));
        assert!(
            !kinds.contains(&"carrier".to_owned()),
            "a hull is not cargo"
        );
    }

    #[test]
    fn identical_units_are_tracked_by_index_not_by_value() {
        // Two infantry compare equal. Filtering an "already taken" list by equality would
        // silently make the second one unloadable, and an invasion would then arrive short.
        let (mut state, origin, _) = state_with_two_systems();
        for _ in 0..3 {
            state.system_mut(&origin).units.push(unit("infantry"));
        }
        let candidates = loadable(&state, ContentStore::embedded(), POK, &player(), &origin);
        let mut window = CargoWindow::new(player(), candidates, 3);

        for _ in 0..3 {
            let choice = window.pending_choice().expect("the hold has room");
            let first = choice
                .options
                .iter()
                .find(|option| !option.is_decline())
                .unwrap()
                .clone();
            window.resolve(first).unwrap();
        }

        assert_eq!(window.cargo().len(), 3, "all three were loadable");
        assert!(window.is_complete());
    }

    #[test]
    fn interchangeable_units_are_offered_once() {
        let (mut state, origin, _) = state_with_two_systems();
        for _ in 0..4 {
            state.system_mut(&origin).units.push(unit("infantry"));
        }
        let candidates = loadable(&state, ContentStore::embedded(), POK, &player(), &origin);
        let window = CargoWindow::new(player(), candidates, 4);

        let choice = window.pending_choice().unwrap();
        assert_eq!(choice.options.len(), 2, "one pickup plus decline");
    }

    #[test]
    fn a_unit_on_a_planet_is_a_different_pickup_from_one_in_space() {
        let (mut state, origin, _) = state_with_two_systems();
        let planet = PlanetId::new("jord");
        state.system_mut(&origin).units.push(unit("infantry"));
        state
            .system_mut(&origin)
            .planet_units
            .entry(planet)
            .or_default()
            .push(unit("infantry"));

        let candidates = loadable(&state, ContentStore::embedded(), POK, &player(), &origin);
        let window = CargoWindow::new(player(), candidates, 2);

        let choice = window.pending_choice().unwrap();
        assert_eq!(
            choice.options.len(),
            3,
            "space, planet, and decline — where it stands is part of the choice"
        );
    }

    #[test]
    fn a_unit_that_rides_free_still_boards_a_full_hold() {
        let (mut state, origin, _) = state_with_two_systems();
        for _ in 0..3 {
            state.system_mut(&origin).units.push(unit("infantry"));
        }
        state.system_mut(&origin).units.push(unit("mech"));
        let ship = unit("carrier");
        state.system_mut(&origin).units.push(ship.clone());
        let content = ContentStore::embedded();
        let run = |state: &GameState| {
            let mut window =
                CargoWindow::for_ship(state, content, POK, &player(), &origin, &ship, &[]);
            while let Some(choice) = window.pending_choice() {
                let pick = choice
                    .options
                    .iter()
                    .find(|option| option.id.starts_with("load|"))
                    .cloned();
                let Some(pick) = pick else { break };
                window.resolve(pick).unwrap();
            }
            (
                window.cargo().len(),
                window.is_complete(),
                window.slots_left(),
            )
        };
        // Neutral: a carrier holds 4 and there are exactly 4 units.
        assert_eq!(run(&state), (4, true, 0));
        // With one more infantry the neutral hold stops at 4 of 5.
        state.system_mut(&origin).units.push(unit("infantry"));
        assert_eq!(run(&state), (4, true, 0));
        let hooks = crate::factions::hooks_movement::MovementHooks {
            free_cargo: Some(|_, _, _, unit| unit.type_id.as_str() == "mech"),
            ..crate::factions::hooks_movement::MovementHooks::NONE
        };
        crate::factions::hooks_movement::with_test_hooks(hooks, || {
            // The mech boards for free beside four infantry: 5 units on a hold of 4.
            assert_eq!(run(&state), (5, true, 0));
        });
    }

    #[test]
    fn a_hold_stops_at_capacity() {
        let (mut state, origin, _) = state_with_two_systems();
        for _ in 0..5 {
            state.system_mut(&origin).units.push(unit("fighter"));
        }
        let candidates = loadable(&state, ContentStore::embedded(), POK, &player(), &origin);
        let mut window = CargoWindow::new(player(), candidates, 2);

        for _ in 0..2 {
            let choice = window.pending_choice().unwrap();
            let pick = choice.options[0].clone();
            window.resolve(pick).unwrap();
        }

        assert!(window.is_complete(), "two of five, and the hold is full");
        assert!(window.pending_choice().is_none());
        assert_eq!(window.cargo().len(), 2);
    }

    #[test]
    fn a_hold_is_complete_when_every_available_unit_is_loaded() {
        // Jol-Nar starts with three loadable units beside a capacity-four carrier. The Python
        // reference breaks its loading loop when no candidates remain; if this window waits for
        // the fourth capacity slot instead, the tactical driver sees no choice and finishes the
        // action without ever sailing the carrier.
        let (mut state, origin, _) = state_with_two_systems();
        for _ in 0..3 {
            state.system_mut(&origin).units.push(unit("infantry"));
        }
        let candidates = loadable(&state, ContentStore::embedded(), POK, &player(), &origin);
        let mut window = CargoWindow::new(player(), candidates, 4);

        for _ in 0..3 {
            let choice = window
                .pending_choice()
                .expect("an available unit is offered");
            let pick = choice
                .options
                .iter()
                .find(|option| !option.is_decline())
                .expect("a pickup is offered")
                .clone();
            window.resolve(pick).unwrap();
        }

        assert!(
            window.is_complete(),
            "nothing remains to fill the spare slot"
        );
        assert!(window.pending_choice().is_none());
        assert_eq!(window.cargo().len(), 3);
    }

    #[test]
    fn declining_closes_the_hold_early() {
        let (mut state, origin, _) = state_with_two_systems();
        state.system_mut(&origin).units.push(unit("infantry"));
        let candidates = loadable(&state, ContentStore::embedded(), POK, &player(), &origin);
        let mut window = CargoWindow::new(player(), candidates, 4);

        let choice = window.pending_choice().unwrap();
        let decline = choice
            .options
            .iter()
            .find(|option| option.is_decline())
            .unwrap()
            .clone();
        window.resolve(decline).unwrap();

        assert!(window.is_complete());
        assert!(window.cargo().is_empty());
    }

    #[test]
    fn an_answer_that_was_not_offered_loads_nothing() {
        let (mut state, origin, _) = state_with_two_systems();
        state.system_mut(&origin).units.push(unit("infantry"));
        let candidates = loadable(&state, ContentStore::embedded(), POK, &player(), &origin);
        let mut window = CargoWindow::new(player(), candidates, 2);
        let before = window.clone();

        let error = window
            .resolve(ChoiceOption::new("load|99", LOAD_KIND))
            .unwrap_err();

        assert!(matches!(error, CargoError::IllegalChoice(_)));
        assert_eq!(window, before);
    }

    #[test]
    fn a_moved_ship_takes_its_cargo_with_it() {
        let (mut state, origin, destination) = state_with_two_systems();
        let ship = unit("carrier");
        let troops = unit("infantry");
        state.system_mut(&origin).units.push(ship.clone());
        state.system_mut(&origin).units.push(troops.clone());

        let cargo = vec![Cargo {
            unit: troops,
            source: CargoSource::Space,
            system: origin.clone(),
        }];
        let outcome = apply_move(&mut state, &origin, &destination, &ship, cargo, true);

        assert!(matches!(outcome, MoveOutcome::Arrived { .. }));
        assert!(state.system_state(&origin).units.is_empty(), "both left");
        assert_eq!(
            state.system_state(&destination).units.len(),
            2,
            "hull and passenger both arrived"
        );
    }

    #[test]
    fn a_passenger_from_a_planet_arrives_in_space_not_on_a_planet() {
        // Landing is invasion, a separate step. Dropping troops straight onto a planet here
        // would conquer it without anyone deciding to.
        let (mut state, origin, destination) = state_with_two_systems();
        let planet = PlanetId::new("jord");
        let ship = unit("carrier");
        let troops = unit("infantry");
        state.system_mut(&origin).units.push(ship.clone());
        state
            .system_mut(&origin)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(troops.clone());

        let cargo = vec![Cargo {
            unit: troops,
            source: CargoSource::Planet(planet.clone()),
            system: origin.clone(),
        }];
        apply_move(&mut state, &origin, &destination, &ship, cargo, true);

        assert!(state.system_state(&origin).on_planet(&planet).is_empty());
        assert_eq!(state.system_state(&destination).units.len(), 2);
        assert!(
            state
                .system_state(&destination)
                .planet_units
                .get(&planet)
                .is_none_or(Vec::is_empty),
            "it did not land"
        );
    }

    #[test]
    fn a_ship_lost_to_a_rift_takes_its_cargo_down_with_it() {
        // 95.1b. The troops must not stay standing in a system whose fleet has drowned.
        let (mut state, origin, destination) = state_with_two_systems();
        let ship = unit("carrier");
        let troops = unit("infantry");
        state.system_mut(&origin).units.push(ship.clone());
        state.system_mut(&origin).units.push(troops.clone());

        let cargo = vec![Cargo {
            unit: troops,
            source: CargoSource::Space,
            system: origin.clone(),
        }];
        let outcome = apply_move(&mut state, &origin, &destination, &ship, cargo, false);

        assert!(matches!(outcome, MoveOutcome::LostToGravityRift { .. }));
        assert!(state.system_state(&origin).units.is_empty(), "both gone");
        assert!(
            state.system_state(&destination).units.is_empty(),
            "nothing arrived"
        );
    }

    #[test]
    fn the_outcome_names_the_passengers_not_just_a_count() {
        // A count cannot be acted on: a table told only that a ship was lost cannot find the
        // piece to take off the board.
        let (mut state, origin, destination) = state_with_two_systems();
        let ship = unit("carrier");
        let troops = unit("infantry");
        state.system_mut(&origin).units.push(ship.clone());
        state.system_mut(&origin).units.push(troops.clone());

        let cargo = vec![Cargo {
            unit: troops.clone(),
            source: CargoSource::Space,
            system: origin.clone(),
        }];
        let outcome = apply_move(&mut state, &origin, &destination, &ship, cargo, false);

        let MoveOutcome::LostToGravityRift { cargo } = outcome else {
            panic!("expected a loss");
        };
        assert_eq!(cargo[0].unit.type_id, troops.type_id);
        assert_eq!(cargo[0].source, CargoSource::Space);
    }

    // -- gravity rift rolls ------------------------------------------------------------

    fn rift_setup() -> (Galaxy, String, Vec<String>) {
        let rift = ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, system)| system.is_gravity_rift())
            .map(|(id, _)| (*id).to_owned())
            .expect("the corpus has a rift");
        let mut ids = vec![plain_systems(1)[0].clone(), rift.clone()];
        ids.extend(plain_systems(7).into_iter().skip(1).take(5));
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let galaxy = Galaxy::build(ContentStore::embedded(), &refs, POK, 1).unwrap();
        (galaxy, rift, ids)
    }

    #[test]
    fn only_rifts_that_are_exited_roll() {
        let (galaxy, rift, ids) = rift_setup();
        let rules = MovementRules::new(
            &galaxy,
            ContentStore::embedded(),
            POK,
            &ids[0],
            Board::default(),
        );

        // Ending in the rift exits nothing: 41.2 speaks of moving *out of* one.
        assert!(rifts_exited(&rules, &[ids[0].clone(), rift.clone()]).is_empty());
        // Passing through it does.
        assert_eq!(
            rifts_exited(&rules, &[rift.clone(), ids[0].clone()]),
            vec![rift]
        );
    }

    #[test]
    fn a_rift_roll_of_three_or_less_destroys_the_ship() {
        let (galaxy, rift, ids) = rift_setup();
        let rules = MovementRules::new(
            &galaxy,
            ContentStore::embedded(),
            POK,
            &ids[0],
            Board::default(),
        );
        let path = vec![rift, ids[0].clone()];

        // Across many seeds both outcomes occur, and every roll is recorded.
        let mut survived = 0;
        let mut lost = 0;
        for seed in 0..60_u64 {
            let mut dice = Dice::new();
            let mut rng = GameRng::new(seed);
            if survives_gravity_rifts(&mut dice, &mut rng, &rules, &path) {
                survived += 1;
            } else {
                lost += 1;
            }
            assert_eq!(dice.count(), 1, "exactly one die per rift exited");
        }
        assert!(survived > 0 && lost > 0, "{survived} survived, {lost} lost");
    }

    #[test]
    fn a_rift_roll_names_its_rift_and_what_the_die_did() {
        // The public roll feed reads these; they are notes on a die already rolled.
        let (galaxy, rift, ids) = rift_setup();
        let rules = MovementRules::new(
            &galaxy,
            ContentStore::embedded(),
            POK,
            &ids[0],
            Board::default(),
        );
        let path = vec![rift.clone(), ids[0].clone()];
        for seed in 0..30_u64 {
            let mut dice = Dice::new();
            let mut rng = GameRng::new(seed);
            let survives = survives_gravity_rifts(&mut dice, &mut rng, &rules, &path);
            let roll = &dice.history()[0];
            assert_eq!(roll.system.as_deref(), Some(rift.as_str()));
            assert_eq!(
                roll.outcome.as_deref(),
                Some(if survives {
                    "ship survives"
                } else {
                    "ship destroyed"
                })
            );
            // Notes are not draws: the same seed rolls the same faces without them.
            let mut plain = Dice::new();
            let mut plain_rng = GameRng::new(seed);
            let bare = plain.roll(
                &mut plain_rng,
                1,
                "gravity rift",
                Some(RIFT_DESTROYS_ON + 1),
            );
            assert_eq!(bare.faces, roll.faces);
        }
    }

    #[test]
    fn the_circlet_owner_never_rolls_for_a_rift() {
        // The immunity is read where the roll happens, so it cannot be honoured in the
        // legality rules and forgotten here - which is exactly what Nav Suite nearly did.
        let (galaxy, rift, ids) = rift_setup();
        let mut rules = MovementRules::new(
            &galaxy,
            ContentStore::embedded(),
            POK,
            &ids[0],
            Board::default(),
        );
        rules.rifts_ignored = true;
        let path = vec![rift, ids[0].clone()];

        let mut dice = Dice::new();
        let mut rng = GameRng::new(1);
        assert!(survives_gravity_rifts(&mut dice, &mut rng, &rules, &path));
        assert_eq!(dice.count(), 0, "no die was even rolled");
    }

    #[test]
    fn ignoring_anomalies_survives_every_rift() {
        // Nav Suite must be honoured here as well as in the legality rules, or it half works.
        let (galaxy, rift, ids) = rift_setup();
        let mut rules = MovementRules::new(
            &galaxy,
            ContentStore::embedded(),
            POK,
            &ids[0],
            Board::default(),
        );
        rules.anomalies_ignored = true;
        let path = vec![rift, ids[0].clone()];

        let mut dice = Dice::new();
        let mut rng = GameRng::new(1);
        assert!(survives_gravity_rifts(&mut dice, &mut rng, &rules, &path));
        assert_eq!(dice.count(), 0, "no die was even rolled");
    }

    #[test]
    fn a_route_with_no_rift_rolls_nothing() {
        let (galaxy, _, ids) = rift_setup();
        let rules = MovementRules::new(
            &galaxy,
            ContentStore::embedded(),
            POK,
            &ids[2],
            Board::default(),
        );
        let mut dice = Dice::new();
        let mut rng = GameRng::new(1);

        assert!(survives_gravity_rifts(
            &mut dice,
            &mut rng,
            &rules,
            &[ids[0].clone(), ids[2].clone()]
        ));
        assert_eq!(dice.count(), 0);
    }
}

#[cfg(test)]
mod relocation_tests {
    use ti4_content::galaxy::Galaxy;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::UnitTypeId;

    use super::*;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }

    fn b() -> PlayerId {
        PlayerId::new("b")
    }

    fn ship(kind: &str, owner: &PlayerId) -> Unit {
        Unit::new(UnitTypeId::new(kind), owner.clone())
    }

    /// Centre 18 with a ring around it, two players, `a`'s two cruisers in the first ring tile.
    struct Table {
        state: GameState,
        galaxy: Galaxy,
        from: SystemId,
        beside: SystemId,
        far: SystemId,
    }

    fn table() -> Table {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let content = ContentStore::embedded();
        let ids: Vec<String> = crate::fixtures::plain_systems(12)
            .into_iter()
            .filter(|id| !state.board.contains_key(&SystemId::new(id.as_str())))
            .take(8)
            .collect();
        let mut tiles = vec!["18"];
        tiles.extend(ids.iter().map(String::as_str));
        let galaxy = Galaxy::build(content, &tiles, DEFAULT, 2).unwrap();
        let from = SystemId::new(ids[0].as_str());
        let beside = galaxy
            .adjacent(from.as_str())
            .into_iter()
            .find(|id| *id != "18" && ids.iter().any(|plain| plain == id))
            .map(SystemId::new)
            .expect("a ring neighbour");
        let far = ids
            .iter()
            .map(|id| SystemId::new(id.as_str()))
            .find(|id| id != &from && !galaxy.are_adjacent(from.as_str(), id.as_str()))
            .expect("a tile not beside the origin");
        state
            .system_mut(&from)
            .add(&[ship("cruiser", &a()), ship("cruiser", &a())]);
        Table {
            state,
            galaxy,
            from,
            beside,
            far,
        }
    }

    fn relocation<'x>(
        from: &'x SystemId,
        ships: &'x [Unit],
        to: &'x SystemId,
        a: &'x PlayerId,
    ) -> Relocation<'x> {
        Relocation {
            player: a,
            from,
            to,
            ships,
            require_adjacent: true,
            forbid_foreign_ships_at_destination: true,
            refuse_fleet_overflow: false,
            reason: "test",
        }
    }

    #[test]
    fn ships_move_between_adjacent_systems_and_are_reported() {
        let mut t = table();
        let from = t.from.clone();
        let (player, to) = (a(), t.beside.clone());
        let ships = [ship("cruiser", &player)];
        let done = relocate_ships(
            &mut t.state,
            ContentStore::embedded(),
            DEFAULT,
            &t.galaxy,
            &relocation(&from, &ships, &to, &player),
        )
        .unwrap();
        assert_eq!(t.state.ships_of(&player, &t.from).len(), 1);
        assert_eq!(t.state.ships_of(&player, &to).len(), 1);
        assert_eq!(done.ships, vec!["cruiser".to_owned()]);
        assert_eq!(done.payload()["count"], serde_json::json!(1));
        assert_eq!(done.payload()["to"], serde_json::json!(to.to_string()));
    }

    #[test]
    fn ships_cannot_be_relocated_into_a_supernova() {
        // 86.1: an out-of-turn move (Foresight) obeys the same entry rules as a tactical move.
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let supernova = crate::fixtures::a_system_where("supernova");
        let plain: Vec<String> = crate::fixtures::plain_systems(12)
            .into_iter()
            .filter(|id| !state.board.contains_key(&SystemId::new(id.as_str())))
            .take(5)
            .collect();
        let mut tiles = vec!["18", supernova.as_str()];
        tiles.extend(plain.iter().map(String::as_str));
        let galaxy = Galaxy::build(content, &tiles, DEFAULT, 2).unwrap();
        let from = SystemId::new(plain[0].as_str());
        let to = SystemId::new(supernova.as_str());
        let player = a();
        state.system_mut(&from).add(&[ship("cruiser", &player)]);
        let before = state.clone();
        let ships = [ship("cruiser", &player)];
        let mut moving = relocation(&from, &ships, &to, &player);
        moving.require_adjacent = false;
        let refused = relocate_ships(&mut state, content, DEFAULT, &galaxy, &moving);
        assert_eq!(refused.unwrap_err(), RelocateError::CannotEnter(to.clone()));
        assert_eq!(
            serde_json::to_value(&state).unwrap(),
            serde_json::to_value(&before).unwrap(),
            "nothing moved"
        );
    }

    #[test]
    fn every_refusal_leaves_the_state_untouched() {
        let mut t = table();
        let from = t.from.clone();
        let (player, other) = (a(), b());
        let content = ContentStore::embedded();
        let beside = t.beside.clone();
        t.state
            .system_mut(&beside)
            .add(&[ship("destroyer", &other)]);
        let before = t.state.clone();
        let one = [ship("cruiser", &player)];
        let far = t.far.clone();
        let cases: Vec<(Vec<Unit>, SystemId, RelocateError)> = vec![
            (vec![], beside.clone(), RelocateError::NothingToMove),
            (
                one.to_vec(),
                from.clone(),
                RelocateError::SameSystem(from.clone()),
            ),
            (
                vec![ship("infantry", &player)],
                far.clone(),
                RelocateError::NotAShip("infantry".to_owned()),
            ),
            (
                vec![ship("dreadnought", &player)],
                far.clone(),
                RelocateError::NotThere {
                    player: player.clone(),
                    system: from.clone(),
                },
            ),
            (
                one.to_vec(),
                far.clone(),
                RelocateError::NotAdjacent {
                    from: from.clone(),
                    to: far.clone(),
                },
            ),
            (
                one.to_vec(),
                beside.clone(),
                RelocateError::ForeignShips(beside.clone()),
            ),
            (
                one.to_vec(),
                SystemId::new("nowhere"),
                RelocateError::NotOnTheMap(SystemId::new("nowhere")),
            ),
        ];
        for (ships, to, expected) in cases {
            let got = relocate_ships(
                &mut t.state,
                content,
                DEFAULT,
                &t.galaxy,
                &relocation(&from, &ships, &to, &player),
            );
            assert_eq!(got.unwrap_err(), expected);
            assert!(t.state == before, "{expected:?} must change nothing");
            assert_eq!(
                t.state.system_state(&from).units,
                before.system_state(&from).units
            );
        }
        // More copies than stand there.
        let three = vec![ship("cruiser", &player); 3];
        let far = t.far.clone();
        let mut free = relocation(&from, &three, &far, &player);
        free.require_adjacent = false;
        assert!(matches!(
            relocate_ships(&mut t.state.clone(), content, DEFAULT, &t.galaxy, &free),
            Err(RelocateError::NotThere { .. })
        ));
    }

    #[test]
    fn the_flags_relax_adjacency_and_the_foreign_ship_bar() {
        let mut t = table();
        let from = t.from.clone();
        let player = a();
        let ships = [ship("cruiser", &player)];
        let far = t.far.clone();
        let mut free = relocation(&from, &ships, &far, &player);
        free.require_adjacent = false;
        relocate_ships(
            &mut t.state,
            ContentStore::embedded(),
            DEFAULT,
            &t.galaxy,
            &free,
        )
        .unwrap();
        assert_eq!(t.state.ships_of(&player, &far).len(), 1);
    }

    #[test]
    fn a_hook_adjacency_link_makes_a_far_system_legal_for_its_player_only() {
        let mut t = table();
        let from = t.from.clone();
        let (player, other) = (a(), b());
        let far = t.far.clone();
        t.state
            .faction_marks
            .insert("link_from".into(), t.from.to_string());
        t.state
            .faction_marks
            .insert("link_to".into(), far.to_string());
        let linked = crate::factions::hooks_movement::MovementHooks {
            linked_systems: Some(|state, _, _, _, who| {
                match (
                    who.as_str(),
                    state.faction_marks.get("link_from"),
                    state.faction_marks.get("link_to"),
                ) {
                    ("a", Some(x), Some(y)) => vec![(x.clone(), y.clone())],
                    _ => Vec::new(),
                }
            }),
            ..crate::factions::hooks_movement::MovementHooks::NONE
        };
        let content = ContentStore::embedded();
        let mine = [ship("cruiser", &player)];
        let theirs = [ship("cruiser", &other)];
        t.state.system_mut(&t.from).add(&theirs);

        // Without the hook neither is adjacent.
        let mut state = t.state.clone();
        assert!(matches!(
            relocate_ships(
                &mut state,
                content,
                DEFAULT,
                &t.galaxy,
                &relocation(&from, &mine, &far, &player)
            ),
            Err(RelocateError::NotAdjacent { .. })
        ));
        crate::factions::hooks_movement::with_test_hooks(linked, || {
            let mut state = t.state.clone();
            relocate_ships(
                &mut state,
                content,
                DEFAULT,
                &t.galaxy,
                &relocation(&from, &mine, &far, &player),
            )
            .expect("player a is linked");
            assert_eq!(state.ships_of(&player, &far).len(), 1);
            let mut state = t.state.clone();
            let request = Relocation {
                player: &other,
                ships: &theirs,
                ..relocation(&from, &theirs, &far, &other)
            };
            assert!(
                matches!(
                    relocate_ships(&mut state, content, DEFAULT, &t.galaxy, &request),
                    Err(RelocateError::NotAdjacent { .. })
                ),
                "player b has no link"
            );
        });
    }

    #[test]
    fn fleet_overflow_is_refused_only_on_request() {
        let mut t = table();
        let from = t.from.clone();
        let player = a();
        let to = t.beside.clone();
        // Fill the destination with the player's whole fleet pool first.
        let limit = {
            let types = catalogue(ContentStore::embedded(), DEFAULT);
            let st = crate::fleet::standing_using(
                &types,
                &t.state,
                ContentStore::embedded(),
                &player,
                &to,
                None,
            );
            st.fleet_limit
        };
        let crowd: Vec<Unit> = (0..limit).map(|_| ship("cruiser", &player)).collect();
        t.state.system_mut(&to).add(&crowd);
        let ships = [ship("cruiser", &player)];
        let mut request = relocation(&from, &ships, &to, &player);
        request.refuse_fleet_overflow = true;
        let before = t.state.clone();
        assert_eq!(
            relocate_ships(
                &mut t.state,
                ContentStore::embedded(),
                DEFAULT,
                &t.galaxy,
                &request
            ),
            Err(RelocateError::FleetOverflow(to.clone()))
        );
        assert!(t.state == before);
        request.refuse_fleet_overflow = false;
        let done = relocate_ships(
            &mut t.state,
            ContentStore::embedded(),
            DEFAULT,
            &t.galaxy,
            &request,
        )
        .unwrap();
        assert!(done.fleet_headroom < 0, "the excess is reported: {done:?}");
    }

    #[test]
    fn the_announcement_reaches_a_timing_ability_with_its_payload() {
        let mut t = table();
        let from = t.from.clone();
        let player = a();
        let to = t.beside.clone();
        let ships = [ship("cruiser", &player)];
        let done = relocate_ships(
            &mut t.state,
            ContentStore::embedded(),
            DEFAULT,
            &t.galaxy,
            &relocation(&from, &ships, &to, &player),
        )
        .unwrap();

        let mut resolver = crate::fixtures::armed_resolver(&t.state);
        let mut dice = Dice::new();
        let mut rng = GameRng::new(0);
        let mut table_ = crate::choice::Table::default();
        let mut sequence = crate::event::EventSequence::new();
        let mut ctx = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table_,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: Some(&t.galaxy),
            }),
        };
        assert_eq!(announce_relocation(&mut t.state, &mut ctx, &done), Ok(true));
        assert!(
            resolver_log_has(&resolver_log(&mut ctx), SHIPS_RELOCATED),
            "the event went through the resolver"
        );
        // Without a resolver the call is a quiet yes.
        let mut quiet = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: DEFAULT,
            dice: &mut Dice::new(),
            rng: &mut GameRng::new(0),
            table: &mut crate::choice::Table::default(),
            timing: None,
        };
        assert_eq!(
            announce_relocation(&mut t.state, &mut quiet, &done),
            Ok(true)
        );
    }

    fn resolver_log(ctx: &mut crate::choice::Resolving<'_>) -> Vec<String> {
        ctx.timing
            .as_ref()
            .map(|handle| handle.resolver.log().to_vec())
            .unwrap_or_default()
    }

    fn resolver_log_has(log: &[String], event: &str) -> bool {
        log.iter().any(|line| line.contains(event))
    }

    // -- relocation with cargo (Argent Flock Migration) -------------------------------------------

    fn cargo_of(state: &GameState, from: &SystemId, kinds: &[&str]) -> Vec<Cargo> {
        let mut offered = loadable(state, ContentStore::embedded(), DEFAULT, &a(), from);
        kinds
            .iter()
            .map(|kind| {
                let at = offered
                    .iter()
                    .position(|cargo| cargo.unit.type_id.as_str() == *kind)
                    .expect("that cargo is loadable");
                offered.remove(at)
            })
            .collect()
    }

    fn with_carrier(t: &mut Table, extra: &[&str]) {
        let from = t.from.clone();
        let mut units = vec![ship("carrier", &a())];
        units.extend(extra.iter().map(|kind| ship(kind, &a())));
        t.state.system_mut(&from).add(&units);
    }

    #[test]
    fn a_relocation_carries_cargo_up_to_capacity_and_reports_it() {
        let mut t = table();
        with_carrier(&mut t, &["infantry", "infantry", "fighter"]);
        let (from, to, player) = (t.from.clone(), t.beside.clone(), a());
        let cargo = cargo_of(&t.state, &from, &["infantry", "infantry", "fighter"]);
        let ships = [ship("carrier", &player)];
        let done = relocate_ships_with_cargo(
            &mut t.state,
            ContentStore::embedded(),
            DEFAULT,
            &t.galaxy,
            &relocation(&from, &ships, &to, &player),
            &cargo,
        )
        .unwrap();
        let at = |system: &SystemId, kind: &str| {
            t.state
                .ships_of(&player, system)
                .iter()
                .filter(|unit| unit.type_id.as_str() == kind)
                .count()
        };
        assert_eq!(at(&to, "carrier"), 1);
        assert_eq!(at(&to, "infantry"), 2);
        assert_eq!(at(&to, "fighter"), 1);
        assert_eq!(
            at(&from, "infantry") + at(&from, "fighter") + at(&from, "carrier"),
            0
        );
        assert_eq!(at(&from, "cruiser"), 2, "the rest stays");
        assert_eq!(
            done.payload()["cargo"],
            serde_json::json!(["infantry", "infantry", "fighter"])
        );
    }

    #[test]
    fn cargo_beyond_capacity_or_not_cargo_refuses_and_changes_nothing() {
        let mut t = table();
        with_carrier(&mut t, &["infantry"; 5]);
        let (from, to, player) = (t.from.clone(), t.beside.clone(), a());
        let ships = [ship("carrier", &player)];
        let before = serde_json::to_value(&t.state).unwrap();
        let content = ContentStore::embedded();
        let five = cargo_of(&t.state, &from, &["infantry"; 5]);
        let refused = relocate_ships_with_cargo(
            &mut t.state,
            content,
            DEFAULT,
            &t.galaxy,
            &relocation(&from, &ships, &to, &player),
            &five,
        );
        assert_eq!(refused.unwrap_err(), RelocateError::OverCapacity);
        let cruiser = Cargo {
            unit: ship("cruiser", &player),
            source: CargoSource::Space,
            system: from.clone(),
        };
        let refused = relocate_ships_with_cargo(
            &mut t.state,
            content,
            DEFAULT,
            &t.galaxy,
            &relocation(&from, &ships, &to, &player),
            &[cruiser],
        );
        assert_eq!(
            refused.unwrap_err(),
            RelocateError::NotCargo("cruiser".to_owned())
        );
        let phantom = Cargo {
            unit: ship("infantry", &b()),
            source: CargoSource::Space,
            system: from.clone(),
        };
        let refused = relocate_ships_with_cargo(
            &mut t.state,
            content,
            DEFAULT,
            &t.galaxy,
            &relocation(&from, &ships, &to, &player),
            &[phantom],
        );
        assert_eq!(refused.unwrap_err(), RelocateError::CargoNotThere);
        assert_eq!(serde_json::to_value(&t.state).unwrap(), before, "atomic");
    }

    #[test]
    fn units_that_ride_free_do_not_use_capacity_when_relocated() {
        let mut t = table();
        with_carrier(
            &mut t,
            &["infantry", "infantry", "infantry", "infantry", "mech"],
        );
        let (from, to, player) = (t.from.clone(), t.beside.clone(), a());
        let ships = [ship("carrier", &player)];
        let content = ContentStore::embedded();
        let all = cargo_of(
            &t.state,
            &from,
            &["infantry", "infantry", "infantry", "infantry", "mech"],
        );
        let neutral = relocate_ships_with_cargo(
            &mut t.state,
            content,
            DEFAULT,
            &t.galaxy,
            &relocation(&from, &ships, &to, &player),
            &all,
        );
        assert_eq!(
            neutral.unwrap_err(),
            RelocateError::OverCapacity,
            "5 units, 4 slots"
        );
        let hooks = crate::factions::hooks_movement::MovementHooks {
            free_cargo: Some(|_, _, _, unit| unit.type_id.as_str() == "mech"),
            ..crate::factions::hooks_movement::MovementHooks::NONE
        };
        crate::factions::hooks_movement::with_test_hooks(hooks, || {
            let done = relocate_ships_with_cargo(
                &mut t.state,
                content,
                DEFAULT,
                &t.galaxy,
                &relocation(&from, &ships, &to, &player),
                &all,
            )
            .unwrap();
            assert_eq!(done.cargo.len(), 5);
        });
        assert_eq!(t.state.ships_of(&player, &to).len(), 6);
    }
}
