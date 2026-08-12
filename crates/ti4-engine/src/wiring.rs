//! Wiring checks: does the driver actually reach each subsystem?
//!
//! Five modules in this project have arrived correct, fully tested, and called by nothing —
//! combat, invasion, fleet enforcement, production and leaders. Every one had green unit tests
//! the whole time, because a unit test proves a module *works*, never that anything *uses* it.
//!
//! The registry ledger catches missing content. This catches missing wiring: it drives a real
//! game and asserts each subsystem announced itself. A module that quietly stops being called
//! fails here rather than in a game months later.

#![cfg(test)]

use ti4_content::ContentStore;
use ti4_model::content_types::POK;
use ti4_model::id::{PlayerId, SystemId};

use crate::choice::{Scripted, Table};
use crate::fixtures::{plain_hub, put};
use crate::game::{Game, TACTICAL_ACTION_ID};
use crate::setup::start_game;

/// Drive a tactical action to completion on a real map, and return the events it produced.
fn events_from_a_tactical_action() -> Vec<String> {
    let hub = plain_hub();
    let players = [PlayerId::new("a"), PlayerId::new("b")];
    let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
    state.phase = ti4_model::state::Phase::Action;
    state.active = Some(PlayerId::new("a"));

    let origin = SystemId::new(hub.outer[0].clone());
    let target = SystemId::new(hub.centre.clone());
    // A carrier with troops aboard, moving onto a defended planet: this is the shortest run
    // that should touch movement, cargo, combat, invasion and production in one action.
    put(&mut state, &origin, "carrier", &PlayerId::new("a"), 1);
    put(&mut state, &origin, "infantry", &PlayerId::new("a"), 2);
    put(&mut state, &target, "fighter", &PlayerId::new("b"), 1);

    let table = Table::with_default(Box::new(Scripted::new([
        TACTICAL_ACTION_ID.to_owned(),
        target.to_string(),
        format!("move|{origin}|0"),
    ])));
    let mut game =
        Game::with_table(state, ContentStore::embedded(), table).with_galaxy(hub.galaxy.clone());

    for _ in 0..200 {
        if game
            .events
            .iter()
            .any(|event| event == "TACTICAL_ACTION_COMPLETE")
        {
            break;
        }
        if game.step().error.is_some() {
            break; // the script ran out, which is fine: we assert on what was reached
        }
    }
    game.events.clone()
}

#[test]
fn a_tactical_action_reaches_activation_and_movement() {
    let events = events_from_a_tactical_action();
    assert!(
        events.iter().any(|e| e == "TACTICAL_ACTION_BEGAN"),
        "the action opened; events {events:?}"
    );
    assert!(
        events.iter().any(|e| e.starts_with("SYSTEM_ACTIVATED:")),
        "activation was reached; events {events:?}"
    );
}

#[test]
fn a_driven_round_reaches_every_phase() {
    // The phase machine is the spine. If a phase stops being entered, every subsystem hanging
    // off it goes quiet at once and no unit test would notice.
    let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
    let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
    let mut game = Game::with_seeded_random(state, ContentStore::embedded(), 5);
    game.run(1, 500).expect("a round completes");

    for expected in [
        "ACTION_PHASE_BEGAN",
        "STATUS_PHASE_BEGAN",
        "STATUS_SCORING_BEGAN",
        "STATUS_PHASE_RESOLVED",
    ] {
        assert!(
            game.events.iter().any(|event| event == expected),
            "{expected} was never reached; events {:?}",
            game.events
        );
    }
}

#[test]
fn the_status_phase_reaches_the_token_gain() {
    // 81.5 is a real decision, and the only sign it happened is the event.
    let players = [PlayerId::new("a"), PlayerId::new("b")];
    let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
    let mut game = Game::with_seeded_random(state, ContentStore::embedded(), 9);
    game.run(1, 500).expect("a round completes");

    assert!(
        game.events
            .iter()
            .any(|event| event.starts_with("COMMAND_TOKEN_GAINED")),
        "81.5 was never reached; events {:?}",
        game.events
    );
}

#[test]
fn every_subsystem_the_driver_owns_is_reachable() {
    // The list is deliberately explicit. Adding a subsystem without adding it here is the
    // omission this test exists to make loud, and a name that no longer appears anywhere in
    // `game.rs` means the wiring was removed.
    let driver = include_str!("game.rs");
    for subsystem in [
        "crate::fleet::enforce",
        "crate::combat::space_cannon_offense",
        "crate::combat::CombatWindow",
        "crate::invasion::InvasionWindow",
        "crate::production::ProductionWindow",
        "crate::agenda_effects::resolve",
    ] {
        assert!(
            driver.contains(subsystem),
            "{subsystem} is no longer called by the driver"
        );
    }
}

#[test]
fn only_test_support_modules_are_test_gated() {
    // `exploration` was accidentally `#[cfg(test)]` for several commits: it compiled, its own
    // tests passed because they run under cfg(test), and nothing outside tests could call it.
    // A module that vanishes from the library is invisible to every other check here.
    let lib = include_str!("lib.rs");
    let gated: Vec<&str> = lib
        .lines()
        .zip(lib.lines().skip(1))
        .filter(|(attr, _)| attr.trim() == "#[cfg(test)]")
        .filter_map(|(_, decl)| decl.trim().strip_prefix("pub mod "))
        .map(|name| name.trim_end_matches(';'))
        .collect();

    assert_eq!(
        gated,
        vec!["fixtures"],
        "only test-support modules may be test-gated"
    );
}

#[test]
fn the_status_phase_still_readies_leaders() {
    // Leaders were built, tested, and uncalled for four commits. This is the guard.
    let status = include_str!("status.rs");
    assert!(
        status.contains("crate::leaders::ready_all"),
        "81.6 no longer readies leaders"
    );
}

#[test]
fn scoring_still_checks_leader_unlocks() {
    let objectives = include_str!("objectives.rs");
    assert!(
        objectives.contains("crate::leaders::check_unlocks"),
        "51.7 no longer fires on scoring"
    );
}

#[test]
fn the_laws_that_bite_are_still_consulted() {
    // A law enforced by nothing is the quietest failure in this engine: state.laws looks like
    // a rule change even when no rule reads it.
    for (module, source, hook) in [
        (
            "fleet",
            include_str!("fleet.rs"),
            "crate::laws::fleet_pool_cap",
        ),
        (
            "action_cards",
            include_str!("action_cards.rs"),
            "crate::laws::action_card_limit",
        ),
        (
            "movement",
            include_str!("movement.rs"),
            "crate::laws::nebulae_passable",
        ),
    ] {
        assert!(source.contains(hook), "{module} no longer consults {hook}");
    }
}
