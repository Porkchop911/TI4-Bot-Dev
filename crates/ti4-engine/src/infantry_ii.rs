//! Infantry II (`inf2`), effect 2, for the generic card (rules version `RULES_TECH_EFFECTS`):
//! "After this unit is destroyed, roll 1 die. If the result is 6 or greater, place the unit on this
//! card. At the start of your next turn, place each unit that is on this card on a planet you
//! control in your home system."
//!
//! The faction versions (Letani Warrior II, Spec Ops II, Crimson Legionnaire II) live in their
//! modules; this is the same shape for the generic infantry. Every path that destroys a ground
//! force announces `GROUND_FORCE_DESTROYED` (combat hits, action cards, agendas); cargo removed
//! over capacity is removed, not destroyed, and does not roll.
//!
//! The units on the card are a count in [`GameState::faction_marks`] and not on the board, which
//! is all it takes to keep them out of every other reader: infantry are uncapped in
//! `supply::plastic`, so nothing counts a unit on the card against the reinforcements. The count
//! comes back on the owner's `TURN_BEGAN`: each unit on the first planet the owner controls in the
//! home system (no question, as Spec Ops II); with none controlled, or no home system, they wait.
//! A player who no longer owns Infantry II loses what is on it.

use std::sync::Arc;

use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use crate::action_cards::{PlacementTarget, place_units_counted, placement_spots};
use crate::timing::{Ability, Relation};

/// The technology.
const TECH: &str = "inf2";
/// The unit type that rolls.
const UNIT: &str = "infantry2";
/// `GameState::faction_marks` key prefix: units on the card (a count, per player).
const CARD_PREFIX: &str = "inf2:card:";
/// The die result that puts the unit on the card.
pub const THRESHOLD: u32 = 6;

fn card_key(player: &PlayerId) -> String {
    format!("{CARD_PREFIX}{player}")
}

/// Whether the effect applies to this game and player: the rules version and the card.
fn active(state: &GameState, player: &PlayerId) -> bool {
    state.rules_version >= ti4_model::state::RULES_TECH_EFFECTS
        && crate::technology::owns_technology(state, player, TECH)
}

/// Infantry II units on the card.
#[must_use]
pub fn units_on_card(state: &GameState, player: &PlayerId) -> usize {
    state
        .faction_marks
        .get(&card_key(player))
        .and_then(|count| count.parse().ok())
        .unwrap_or(0)
}

fn set_on_card(state: &mut GameState, player: &PlayerId, count: usize) {
    if count == 0 {
        state.faction_marks.remove(&card_key(player));
    } else {
        state
            .faction_marks
            .insert(card_key(player), count.to_string());
    }
}

fn event_player(event: &crate::event::Event) -> Option<PlayerId> {
    event.text("player").map(PlayerId::new)
}

/// The two timing abilities of one seat; registered for every seat because the card can be gained
/// mid-game and the resolver has no late registration. The conditions limit them to the holder.
pub(crate) fn abilities(owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    vec![rolls(owner_name, seat), returns(owner_name, seat)]
}

/// After the owner's Infantry II is destroyed: roll a die, on a 6 or more it goes on the card.
fn rolls(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("technology:{owner_name}:inf2:GROUND_FORCE_DESTROYED:after"),
        seat.clone(),
        "GROUND_FORCE_DESTROYED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let roll = context
                .dice
                .roll_by(context.rng, 1, "infantry II", Some(THRESHOLD), &owner);
            if roll.faces.first().is_some_and(|face| *face >= THRESHOLD) {
                let now = units_on_card(context.state, &owner) + 1;
                set_on_card(context.state, &owner, now);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event_player(event).as_ref() == Some(&condition_owner)
            && event.text("unit") == Some(UNIT)
            && active(context.state, &condition_owner)
    }))
}

/// At the start of the owner's turn: every unit on the card goes to the home system.
fn returns(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("technology:{owner_name}:inf2_return:TURN_BEGAN:after"),
        seat.clone(),
        "TURN_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let waiting = units_on_card(context.state, &owner);
            if !crate::technology::owns_technology(context.state, &owner, TECH) {
                // The card is gone and so is what was on it.
                set_on_card(context.state, &owner, 0);
                return Ok(());
            }
            let Some(home) = context
                .state
                .player(&owner)
                .and_then(|seat| seat.home_system.clone())
            else {
                return Ok(());
            };
            let planet = placement_spots(
                context.state,
                context.content,
                context.sources,
                &owner,
                PlacementTarget::ControlledPlanet,
                Some(&home),
            )
            .into_iter()
            .find_map(|(_, planet)| planet);
            let Some(planet) = planet else {
                return Ok(());
            };
            let placed =
                place_units_counted(context, &owner, &home, Some(&planet), UNIT_BASE, waiting);
            set_on_card(context.state, &owner, waiting.saturating_sub(placed));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event_player(event).as_ref() == Some(&condition_owner)
            && state_has_card_units(context.state, &condition_owner)
    }))
}

/// The base type placed.
const UNIT_BASE: &str = "infantry";

/// Units wait on the card in a game that has the effect.
fn state_has_card_units(state: &GameState, player: &PlayerId) -> bool {
    state.rules_version >= ti4_model::state::RULES_TECH_EFFECTS && units_on_card(state, player) > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::choice::{Scripted, Table};
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::{SystemId, TechnologyId};

    fn game() -> GameState {
        // Hacan has no infantry of its own, so Infantry II is the generic card.
        crate::fixtures::seated_game(&[("a", "hacan"), ("b", "sol")], POK)
    }

    fn a() -> PlayerId {
        PlayerId::new("a")
    }

    fn emit(
        state: &mut GameState,
        faces: &[u32],
        event_type: &str,
        pairs: &[(&str, &str)],
    ) -> usize {
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut dice = crate::dice::Dice::from_faces(faces.iter().copied());
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = Table::with_default(Box::new(Scripted::new(["never asked"])));
        let mut context = crate::timing::TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: POK,
            table: &mut table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy: None,
        };
        let payload = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
            .collect();
        let event = context
            .event_sequence
            .next(event_type, payload)
            .expect("an event id");
        resolver
            .emit_with_context(&mut context, event, |_, _| {})
            .expect("resolves");
        context.dice.history().len()
    }

    fn destroyed(state: &mut GameState, who: &str, unit: &str, faces: &[u32]) -> usize {
        emit(
            state,
            faces,
            "GROUND_FORCE_DESTROYED",
            &[
                ("player", who),
                ("unit", unit),
                ("system", "x"),
                ("planet", "y"),
            ],
        )
    }

    fn turn_began(state: &mut GameState) {
        emit(state, &[], "TURN_BEGAN", &[("player", "a")]);
    }

    fn home(state: &GameState) -> (SystemId, ti4_model::id::PlanetId) {
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        let planet = state
            .controlled_planets(&a())
            .into_iter()
            .find(|(system, _)| **system == home)
            .map(|(_, planet)| planet.clone())
            .expect("a home planet");
        (home, planet)
    }

    fn infantry_at_home(state: &GameState) -> usize {
        let (system, planet) = home(state);
        state
            .system_state(&system)
            .on_planet_of(&planet, &a())
            .iter()
            .filter(|unit| unit.type_id.as_str() == "infantry2")
            .count()
    }

    #[test]
    fn a_six_puts_the_unit_on_the_card_and_it_is_not_on_the_board() {
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("inf2"));
        let before = infantry_at_home(&state);
        assert_eq!(destroyed(&mut state, "a", "infantry2", &[6]), 1, "one die");
        assert_eq!(units_on_card(&state, &a()), 1);
        assert_eq!(
            infantry_at_home(&state),
            before,
            "on the card, not on a planet"
        );
        turn_began(&mut state);
        assert_eq!(units_on_card(&state, &a()), 0);
        assert_eq!(infantry_at_home(&state), before + 1);
    }

    #[test]
    fn a_five_or_the_wrong_unit_or_the_wrong_owner_does_nothing() {
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("inf2"));
        destroyed(&mut state, "a", "infantry2", &[5]);
        assert_eq!(units_on_card(&state, &a()), 0, "a 5 is not enough");
        assert_eq!(
            destroyed(&mut state, "a", "infantry", &[6]),
            0,
            "Infantry I does not roll"
        );
        assert_eq!(
            destroyed(&mut state, "b", "infantry2", &[6]),
            0,
            "another player's unit"
        );
        assert_eq!(units_on_card(&state, &a()), 0);
    }

    #[test]
    fn several_units_come_back_together() {
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("inf2"));
        let before = infantry_at_home(&state);
        for face in [6, 6, 3, 9] {
            destroyed(&mut state, "a", "infantry2", &[face]);
        }
        assert_eq!(units_on_card(&state, &a()), 3);
        turn_began(&mut state);
        assert_eq!(infantry_at_home(&state), before + 3);
        assert_eq!(units_on_card(&state, &a()), 0);
    }

    #[test]
    fn with_no_home_planet_the_units_wait_on_the_card() {
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("inf2"));
        let (system, planet) = home(&state);
        destroyed(&mut state, "a", "infantry2", &[6]);
        state.system_mut(&system).planet_control.clear();
        turn_began(&mut state);
        assert_eq!(units_on_card(&state, &a()), 1, "still waiting");
        state.system_mut(&system).set_control(planet, a());
        let before = infantry_at_home(&state);
        turn_began(&mut state);
        assert_eq!(units_on_card(&state, &a()), 0);
        assert_eq!(infantry_at_home(&state), before + 1);
    }

    #[test]
    fn a_player_who_lost_the_card_loses_what_was_on_it() {
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("inf2"));
        destroyed(&mut state, "a", "infantry2", &[6]);
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .remove(&TechnologyId::new("inf2"));
        let before = infantry_at_home(&state);
        turn_began(&mut state);
        assert_eq!(units_on_card(&state, &a()), 0);
        assert_eq!(infantry_at_home(&state), before, "nothing comes back");
    }

    #[test]
    fn legacy_games_do_not_roll() {
        let mut state = game();
        state.rules_version = ti4_model::state::RULES_TECH_EFFECTS - 1;
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("inf2"));
        assert_eq!(destroyed(&mut state, "a", "infantry2", &[6]), 0, "no die");
        assert_eq!(units_on_card(&state, &a()), 0);
    }

    #[test]
    fn a_destroyed_unit_without_the_card_does_not_roll() {
        let mut state = game();
        assert_eq!(destroyed(&mut state, "a", "infantry2", &[6]), 0);
    }
}
