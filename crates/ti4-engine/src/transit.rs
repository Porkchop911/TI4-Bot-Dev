//! Applying a move: cargo (LRR 95) and the gravity-rift roll (41.2).
//!
//! Ported from the oracle's `Game._load_cargo`, `_survives_gravity_rifts` and the body of
//! `_move_one`. [`crate::movement`] decides whether a move is *legal*; this decides what
//! actually happens when it is taken.
//!
//! The 41.2 destruction roll lives here rather than with the legality rules because it is a
//! consequence of moving, not a question about whether the move may be made.

use ti4_content::ContentStore;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};
use crate::dice::Dice;
use crate::movement::MovementRules;
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
    let types = catalogue(content, sources);
    let consumes = |unit: &Unit| {
        types
            .get(unit.type_id.as_str())
            .is_some_and(UnitType::consumes_capacity)
    };
    let system = state.system_state(origin);

    // 95.5: "Fighters and ground forces cannot be picked up from a system that contains one of
    // their faction's command tokens other than the active system."
    //
    // Ordinarily unreachable, because 58.4c stops a ship leaving such a system at all -- but the
    // Dominus Orb suspends exactly that, and a ship freed to leave must still not take the
    // garrison with it.
    if state.active_system.as_ref() != Some(origin) && system.command_tokens.contains(player) {
        return Vec::new();
    }

    let mut found: Vec<Cargo> = system
        .units_of(player)
        .into_iter()
        .filter(|unit| consumes(unit))
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
/// Units are taken from the system the ship starts in — its space area or a planet there — up
/// to the ship's capacity. Picking up en route (95.1) is not modelled.
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
    capacity: i64,
    closed: bool,
}

impl CargoWindow {
    /// Open a hold of `capacity` over everything loadable in the origin system.
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
            capacity,
            closed: capacity <= 0,
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
        // 95.1: "During a tactical action, it can pick up and transport units from the active
        // system, the system it started its movement in, and each system it moves through." The
        // path carries the systems between the two, and 95.5 is applied per system inside
        // `loadable`, so a system holding this player's command token contributes nothing.
        let mut candidates = loadable(state, content, sources, player, origin);
        let mut seen: std::collections::BTreeSet<String> =
            std::iter::once(origin.to_string()).collect();
        for step in path {
            if !seen.insert(step.clone()) {
                continue; // a route may revisit a system; its units are offered once
            }
            candidates.extend(loadable(
                state,
                content,
                sources,
                player,
                &SystemId::new(step.clone()),
            ));
        }
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
        Self {
            player: player.clone(),
            candidates,
            origin: Some(origin.clone()),
            ship_type: Some(ship.type_id.to_string()),
            ground,
            fighters,
            loaded: Vec::new(),
            capacity,
            closed: capacity <= 0,
        }
    }

    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.closed
            || self.loaded.len() >= self.candidates.len()
            || i64::try_from(self.loaded.len()).unwrap_or(i64::MAX) >= self.capacity
    }

    /// What has been loaded, in the order it was taken aboard.
    #[must_use]
    pub fn cargo(&self) -> Vec<Cargo> {
        self.loaded
            .iter()
            .map(|index| self.candidates[*index].clone())
            .collect()
    }

    /// Indices still free, in candidate order.
    fn free(&self) -> Vec<usize> {
        (0..self.candidates.len())
            .filter(|index| !self.loaded.contains(index))
            .collect()
    }

    /// The next pickup choice, or `None` once the hold is closed or full.
    ///
    /// Interchangeable units — same type, same damage, same source — are offered once. Beyond
    /// tidiness this matters because a sampling decider draws per option, so a pickup written
    /// three times would carry three times the weight of an equally good one written once.
    #[must_use]
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
            let mut option = ChoiceOption::labelled(
                format!("load|{index}"),
                LOAD_KIND,
                format!("load {} from {where_from}", cargo.unit.type_id),
            )
            .with("unit", cargo.unit.type_id.to_string())
            .with("source", source)
            .with("damaged", cargo.unit.sustained_damage)
            .with("galvanized", cargo.unit.galvanized)
            .with(
                "capacity_remaining",
                self.capacity - i64::try_from(self.loaded.len()).unwrap_or(i64::MAX),
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
            |ship| {
                format!(
                    "load {ship} ({} free)",
                    self.capacity - i64::try_from(self.loaded.len()).unwrap_or(i64::MAX)
                )
            },
        );
        Some(Choice::new(self.player.clone(), prompt, options))
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
        .filter(|system| rules.is_rift(system))
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
    for _ in rifts_exited(rules, path) {
        let roll = dice.roll(rng, 1, "gravity rift", Some(RIFT_DESTROYS_ON + 1));
        if roll
            .faces
            .first()
            .is_some_and(|face| *face <= RIFT_DESTROYS_ON)
        {
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
