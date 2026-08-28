//! Coexistence (Thunder's Edge).
//!
//! Normally two players cannot both have ground forces on a planet: committing to a planet someone
//! else holds starts a ground combat. Some effects instead let the units *coexist*, sitting on the
//! same planet without fighting. One player still controls it; the others are coexisting.
//!
//! In the six-faction scope only three things grant it — the action cards `exchangeprogram` and
//! `crashlanding`, and the technology `sdn`. Everything else that grants coexistence belongs to
//! factions this project does not play. The mechanic is built first regardless, because those three
//! cannot be implemented on top of nothing.
//!
//! The rules implemented here:
//!
//! > **2.** One player controls the planet; all other players are coexisting on that planet.
//! >
//! > **3.1.** If a game effect instructs a player to coexist on a planet they do not control, the
//! > original controller of that planet retains control.
//! >
//! > **3.2.** If a game effect instructs a player to coexist on a planet they do control, then
//! > another player will gain control of that planet (which will exhaust that planet).
//! >
//! > **5.** If additional units are added to those already coexisting … the added units immediately
//! > coexist.
//! >
//! > **6.** If only one player has units on a planet that was in coexistence, the coexistence ends,
//! > and that player gains control of that planet if they were the coexisting player.
//! >
//! > **13.** A player is considered to control a planet they are coexisting on solely when scoring
//! > an objective. For any other game ability or effect, they are not considered to control that
//! > planet.
//!
//! Rule 13 is the one most likely to cause quiet damage if it leaks. A coexister must count for
//! objectives and for nothing else — not for spending the planet, not for voting with it, not for
//! the opening bar. It is therefore exposed as [`controls_for_scoring`], a deliberately awkward name
//! that no caller reaches for by accident, rather than folded into `controlled_planets`.

use std::collections::BTreeSet;

use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::GameState;

/// Everyone coexisting on a planet, the controller aside.
#[must_use]
pub fn coexisters(state: &GameState, system: &SystemId, planet: &PlanetId) -> BTreeSet<PlayerId> {
    state
        .system_state(system)
        .coexisting
        .get(planet)
        .cloned()
        .unwrap_or_default()
}

/// Whether this player is coexisting on this planet (and so does not control it).
#[must_use]
pub fn is_coexisting(
    state: &GameState,
    system: &SystemId,
    planet: &PlanetId,
    player: &PlayerId,
) -> bool {
    coexisters(state, system, planet).contains(player)
}

/// Whether a planet is in coexistence at all.
#[must_use]
pub fn in_coexistence(state: &GameState, system: &SystemId, planet: &PlanetId) -> bool {
    !coexisters(state, system, planet).is_empty()
}

/// Rule 13: control **solely** for the purpose of scoring an objective.
///
/// True for the controller and for every coexister. Nothing else in the engine may use this: a
/// coexister cannot spend the planet, vote with it, or count it toward the opening bar.
#[must_use]
pub fn controls_for_scoring(
    state: &GameState,
    system: &SystemId,
    planet: &PlanetId,
    player: &PlayerId,
) -> bool {
    let record = state.system_state(system);
    record.planet_control.get(planet) == Some(player)
        || record
            .coexisting
            .get(planet)
            .is_some_and(|others| others.contains(player))
}

/// Put `player` into coexistence on a planet (rules 3.1, 3.2, 5).
///
/// The two clauses differ only in who ends up controlling:
///
/// - coexisting onto a planet someone else holds leaves that holder in control (3.1);
/// - coexisting onto a planet **you** hold hands control to `taker`, and exhausts the planet (3.2).
///
/// 3.2 needs a taker because the rule says "another player will gain control", and which player
/// that is comes from the effect rather than from this rule. An effect that offers no taker cannot
/// put the controller into coexistence on their own planet, so that case is refused rather than
/// guessed at.
///
/// Rule 5 needs no code: units added later belong to a player who is already listed here, and
/// membership is per player rather than per unit.
///
/// # Errors
/// [`CoexistError`] when the controller is asked to coexist on their own planet with no taker.
pub fn begin(
    state: &mut GameState,
    system: &SystemId,
    planet: &PlanetId,
    player: &PlayerId,
    taker: Option<&PlayerId>,
) -> Result<(), CoexistError> {
    let holder = state
        .system_state(system)
        .planet_control
        .get(planet)
        .cloned();

    if holder.as_ref() == Some(player) {
        // 3.2: the controller steps aside.
        let Some(taker) = taker else {
            return Err(CoexistError::NoTaker(planet.clone()));
        };
        let record = state.system_mut(system);
        record.set_control(planet.clone(), taker.clone());
        record
            .coexisting
            .entry(planet.clone())
            .or_default()
            .insert(player.clone());
        record
            .coexisting
            .entry(planet.clone())
            .or_default()
            .remove(taker);
        state.exhausted_planets.insert(planet.clone());
        return Ok(());
    }

    // 3.1: the holder keeps control; the newcomer coexists.
    state
        .system_mut(system)
        .coexisting
        .entry(planet.clone())
        .or_default()
        .insert(player.clone());
    Ok(())
}

/// Rule 6: coexistence ends when only one player still has units on the planet.
///
/// That player takes control if they were a coexister. Called wherever ground forces are removed —
/// combat, bombardment, or a unit leaving — and it is idempotent, so calling it too often is safe
/// and calling it too rarely is the only failure mode.
///
/// A planet with **no** units left is not "one player": coexistence ends, and control stays where it
/// was, matching how planet control behaves generally.
pub fn reconcile(state: &mut GameState, system: &SystemId, planet: &PlanetId) -> bool {
    if !in_coexistence(state, system, planet) {
        return false;
    }
    let present: BTreeSet<PlayerId> = state
        .system_state(system)
        .on_planet(planet)
        .iter()
        .map(|unit| unit.owner.clone())
        .collect();

    if present.len() > 1 {
        return false;
    }
    let record = state.system_mut(system);
    record.coexisting.remove(planet);
    if let Some(sole) = present.into_iter().next() {
        record.set_control(planet.clone(), sole);
    }
    true
}

/// Re-evaluate every planet currently in coexistence.
pub fn reconcile_all(state: &mut GameState) -> bool {
    let pairs: Vec<(SystemId, PlanetId)> = state
        .board
        .iter()
        .flat_map(|(system, record)| {
            record
                .coexisting
                .keys()
                .map(move |planet| (system.clone(), planet.clone()))
        })
        .collect();
    let mut changed = false;
    for (system, planet) in pairs {
        changed |= reconcile(state, &system, &planet);
    }
    changed
}

/// A coexistence instruction that cannot be carried out.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoexistError {
    /// Rule 3.2 needs someone to take control, and the effect named nobody.
    #[error("coexisting on {0}, which this player controls, needs another player to take control")]
    NoTaker(PlanetId),
}

#[cfg(test)]
mod tests {
    use ti4_model::units::Unit;

    use super::*;

    fn infantry(owner: &PlayerId) -> Unit {
        Unit::new(ti4_model::id::UnitTypeId::new("infantry"), owner.clone())
    }

    fn fixture() -> (GameState, PlayerId, PlayerId, SystemId, PlanetId) {
        let mut state = crate::fixtures::game(&["a", "b"]);
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        let system = SystemId::new("19");
        let planet = PlanetId::new("p");
        state
            .system_mut(&system)
            .set_control(planet.clone(), a.clone());
        state
            .system_mut(&system)
            .planet_units
            .insert(planet.clone(), vec![infantry(&a)]);
        (state, a, b, system, planet)
    }

    #[test]
    fn coexisting_on_another_players_planet_leaves_them_in_control() {
        // Rule 3.1.
        let (mut state, a, b, system, planet) = fixture();
        begin(&mut state, &system, &planet, &b, None).unwrap();

        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&a),
            "the original controller retains control"
        );
        assert!(is_coexisting(&state, &system, &planet, &b));
        assert!(!is_coexisting(&state, &system, &planet, &a), "rule 2");
    }

    #[test]
    fn coexisting_on_your_own_planet_hands_control_over_and_exhausts_it() {
        // Rule 3.2.
        let (mut state, a, b, system, planet) = fixture();
        begin(&mut state, &system, &planet, &a, Some(&b)).unwrap();

        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&b),
            "another player gains control"
        );
        assert!(is_coexisting(&state, &system, &planet, &a));
        assert!(
            state.exhausted_planets.contains(&planet),
            "which will exhaust that planet"
        );
    }

    #[test]
    fn stepping_aside_with_nobody_to_take_it_is_refused() {
        let (mut state, a, _b, system, planet) = fixture();
        assert_eq!(
            begin(&mut state, &system, &planet, &a, None),
            Err(CoexistError::NoTaker(planet))
        );
    }

    #[test]
    fn coexistence_ends_when_one_player_is_left_and_they_take_control() {
        // Rule 6, in the case that matters: the survivor was the coexister, not the controller.
        let (mut state, a, b, system, planet) = fixture();
        begin(&mut state, &system, &planet, &b, None).unwrap();
        state
            .system_mut(&system)
            .planet_units
            .insert(planet.clone(), vec![infantry(&a), infantry(&b)]);

        // A loses their ground force.
        state
            .system_mut(&system)
            .planet_units
            .insert(planet.clone(), vec![infantry(&b)]);

        assert!(reconcile(&mut state, &system, &planet));
        assert!(!in_coexistence(&state, &system, &planet));
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&b),
            "the surviving coexister gains control"
        );
    }

    #[test]
    fn two_players_present_stays_in_coexistence() {
        let (mut state, a, b, system, planet) = fixture();
        begin(&mut state, &system, &planet, &b, None).unwrap();
        state
            .system_mut(&system)
            .planet_units
            .insert(planet.clone(), vec![infantry(&a), infantry(&b)]);

        assert!(!reconcile(&mut state, &system, &planet));
        assert!(in_coexistence(&state, &system, &planet));
    }

    #[test]
    fn a_coexister_controls_only_for_scoring() {
        // Rule 13, stated as the pair of facts that must both hold.
        let (mut state, a, b, system, planet) = fixture();
        begin(&mut state, &system, &planet, &b, None).unwrap();

        assert!(controls_for_scoring(&state, &system, &planet, &b));
        assert!(controls_for_scoring(&state, &system, &planet, &a));
        assert!(
            !state
                .controlled_planets(&b)
                .into_iter()
                .any(|(_, held)| *held == planet),
            "a coexister must not appear in ordinary control, or they could spend and vote with it"
        );
    }
}
